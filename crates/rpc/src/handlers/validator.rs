use std::{collections::HashSet, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Path, Query},
};
use ream_beacon_api_types::{
    error::ApiError,
    id::{ID, ValidatorID},
    query::{AttestationQuery, IdQuery, StatusQuery},
    request::ValidatorsPostRequest,
    responses::{BeaconResponse, DataResponse},
    validator::{ValidatorBalance, ValidatorData, ValidatorStatus},
};
use ream_bls::PublicKey;
use ream_consensus::{
    attestation_data::AttestationData,
    checkpoint::Checkpoint,
    constants::SLOTS_PER_EPOCH,
    electra::beacon_state::BeaconState,
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch},
    validator::Validator,
};
use ream_fork_choice::store::Store;
use ream_operation_pool::OperationPool;
use ream_storage::{db::ReamDB, tables::Table};
use serde::Serialize;

use super::state::get_state_from_id;

const MAX_VALIDATOR_COUNT: usize = 100;

//  For slots in Electra and later, this AttestationData must have a committee_index of 0.
const ELECTRA_COMMITTEE_INDEX: u64 = 0;

fn build_validator_balances(
    validators: &[(Validator, u64)],
    filter_ids: Option<&Vec<ValidatorID>>,
) -> Vec<ValidatorBalance> {
    // Turn the optional Vec<ValidatorID> into an optional HashSet for O(1) lookups
    let filtered_ids = filter_ids.map(|ids| ids.iter().collect::<HashSet<_>>());

    validators
        .iter()
        .enumerate()
        .filter(|(idx, (validator, _))| match &filtered_ids {
            Some(ids) => {
                ids.contains(&ValidatorID::Index(*idx as u64))
                    || ids.contains(&ValidatorID::Address(validator.public_key.clone()))
            }
            None => true,
        })
        .map(|(idx, (_, balance))| ValidatorBalance {
            index: idx as u64,
            balance: *balance,
        })
        .collect()
}

#[get("/beacon/states/{state_id}/validator/{validator_id}")]
pub async fn get_validator_from_state(
    db: Data<ReamDB>,
    param: Path<(ID, ValidatorID)>,
) -> Result<impl Responder, ApiError> {
    let (state_id, validator_id) = param.into_inner();
    let state = get_state_from_id(state_id, &db).await?;

    let (index, validator) = {
        match &validator_id {
            ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                Some(validator) => (*i as usize, validator.to_owned()),
                None => {
                    return Err(ApiError::NotFound(format!(
                        "Validator not found for index: {i}"
                    )));
                }
            },
            ValidatorID::Address(public_key) => {
                match state
                    .validators
                    .iter()
                    .enumerate()
                    .find(|(_, validator)| validator.public_key == *public_key)
                {
                    Some((i, validator)) => (i, validator.to_owned()),
                    None => {
                        return Err(ApiError::NotFound(format!(
                            "Validator not found for public_key: {public_key:?}"
                        )))?;
                    }
                }
            }
        }
    };

    let balance = state.balances.get(index).ok_or(ApiError::NotFound(format!(
        "Validator not found for index: {index}"
    )))?;

    let status = validator_status(&validator, &db).await?;

    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator,
        ))),
    )
}

pub async fn validator_status(
    validator: &Validator,
    db: &ReamDB,
) -> Result<ValidatorStatus, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let state = get_state_from_id(ID::Slot(highest_slot), db).await?;

    if validator.exit_epoch < state.get_current_epoch() {
        Ok(ValidatorStatus::Offline)
    } else {
        Ok(ValidatorStatus::ActiveOngoing)
    }
}

#[get("/beacon/states/{state_id}/validators")]
pub async fn get_validators_from_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    id_query: Query<IdQuery>,
    status_query: Query<StatusQuery>,
) -> Result<impl Responder, ApiError> {
    if let Some(validator_ids) = &id_query.id {
        if validator_ids.len() >= MAX_VALIDATOR_COUNT {
            return Err(ApiError::TooManyValidatorsIds);
        }
    }

    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    let mut validators_data = Vec::new();
    let mut validator_indices_to_process = Vec::new();

    // First, collect all the validator indices we need to process
    if let Some(validator_ids) = &id_query.id {
        for validator_id in validator_ids {
            let (index, _) = {
                match validator_id {
                    ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                        Some(validator) => (*i as usize, validator.to_owned()),
                        None => {
                            return Err(ApiError::NotFound(format!(
                                "Validator not found for index: {i}"
                            )))?;
                        }
                    },
                    ValidatorID::Address(public_key) => {
                        match state
                            .validators
                            .iter()
                            .enumerate()
                            .find(|(_, validator)| validator.public_key == *public_key)
                        {
                            Some((i, validator)) => (i, validator.to_owned()),
                            None => {
                                return Err(ApiError::NotFound(format!(
                                    "Validator not found for public_key: {public_key:?}"
                                )))?;
                            }
                        }
                    }
                }
            };
            validator_indices_to_process.push(index);
        }
    } else {
        validator_indices_to_process = (0..state.validators.len()).collect();
    }

    for index in validator_indices_to_process {
        let validator = &state.validators[index];

        let status = validator_status(validator, &db).await?;

        if status_query.has_status() && !status_query.contains_status(&status) {
            continue;
        }

        let balance = state.balances.get(index).ok_or(ApiError::NotFound(format!(
            "Validator not found for index: {index}"
        )))?;

        validators_data.push(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator.clone(),
        ));
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

#[post("/beacon/states/{state_id}/validators")]
pub async fn post_validators_from_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    request: Json<ValidatorsPostRequest>,
    _status_query: Json<StatusQuery>,
) -> Result<impl Responder, ApiError> {
    let ValidatorsPostRequest { ids, statuses, .. } = request.into_inner();
    let status_query = StatusQuery { status: statuses };

    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    let mut validators_data = Vec::new();
    let mut validator_indices_to_process = Vec::new();

    // First, collect all the validator indices we need to process
    if let Some(validator_ids) = &ids {
        for validator_id in validator_ids {
            let (index, _) = {
                match validator_id {
                    ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                        Some(validator) => (*i as usize, validator.to_owned()),
                        None => {
                            return Err(ApiError::NotFound(format!(
                                "Validator not found for index: {i}"
                            )))?;
                        }
                    },
                    ValidatorID::Address(public_key) => {
                        match state
                            .validators
                            .iter()
                            .enumerate()
                            .find(|(_, validator)| validator.public_key == *public_key)
                        {
                            Some((i, validator)) => (i, validator.to_owned()),
                            None => {
                                return Err(ApiError::NotFound(format!(
                                    "Validator not found for public_key: {public_key:?}"
                                )))?;
                            }
                        }
                    }
                }
            };
            validator_indices_to_process.push(index);
        }
    } else {
        validator_indices_to_process = (0..state.validators.len()).collect();
    }

    for index in validator_indices_to_process {
        let validator = &state.validators[index];

        let status = validator_status(validator, &db).await?;

        if status_query.has_status() && !status_query.contains_status(&status) {
            continue;
        }

        let balance = state.balances.get(index).ok_or(ApiError::NotFound(format!(
            "Validator not found for index: {index}"
        )))?;

        validators_data.push(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator.clone(),
        ));
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

#[derive(Debug, Serialize)]
struct ValidatorIdentity {
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    public_key: PublicKey,
    #[serde(with = "serde_utils::quoted_u64")]
    activation_epoch: u64,
}

#[post("/beacon/states/{state_id}/validator_identities")]
pub async fn post_validator_identities_from_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    validator_ids: Json<Vec<ValidatorID>>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    let validator_ids_set: HashSet<ValidatorID> = validator_ids.into_inner().into_iter().collect();

    let validator_identities: Vec<ValidatorIdentity> = state
        .validators
        .iter()
        .enumerate()
        .filter_map(|(index, validator)| {
            if validator_ids_set.contains(&ValidatorID::Index(index as u64))
                || validator_ids_set.contains(&ValidatorID::Address(validator.public_key.clone()))
            {
                Some(ValidatorIdentity {
                    index: index as u64,
                    public_key: validator.public_key.clone(),
                    activation_epoch: validator.activation_epoch,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validator_identities)))
}

#[get("/beacon/states/{state_id}/validator_balances")]
pub async fn get_validator_balances_from_state(
    state_id: Path<ID>,
    query: Query<IdQuery>,
    db: Data<ReamDB>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(build_validator_balances(
            &state
                .validators
                .into_iter()
                .zip(state.balances.into_iter())
                .collect::<Vec<_>>(),
            query.id.as_ref(),
        ))),
    )
}

#[post("/beacon/states/{state_id}/validator_balances")]
pub async fn post_validator_balances_from_state(
    state_id: Path<ID>,
    body: Json<IdQuery>,
    db: Data<ReamDB>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(build_validator_balances(
            &state
                .validators
                .into_iter()
                .zip(state.balances.into_iter())
                .collect::<Vec<_>>(),
            body.id.as_ref(),
        ))),
    )
}

#[derive(Debug, Serialize)]
pub struct ValidatorLivenessData {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    pub is_live: bool,
}

impl ValidatorLivenessData {
    pub fn new(index: u64, is_live: bool) -> Self {
        Self { index, is_live }
    }
}

#[post("/validator/liveness/{epoch}")]
pub async fn post_validator_liveness(
    db: Data<ReamDB>,
    epoch: Path<u64>,
    validator_indices: Json<Vec<String>>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let validator_indices = validator_indices.into_inner();

    let slot = epoch * SLOTS_PER_EPOCH;
    let state = get_state_from_id(ID::Slot(slot), &db).await?;

    let mut liveness_data = Vec::new();

    for validator_index_str in validator_indices {
        let validator_index: u64 = validator_index_str
            .parse()
            .map_err(|_| ApiError::BadRequest("Invalid validator index".to_string()))?;
        let index = validator_index as usize;

        match state.validators.get(index) {
            Some(_validator) => {
                let is_live = check_validator_participation(&state, index, epoch)?;
                liveness_data.push(ValidatorLivenessData::new(validator_index, is_live));
            }
            None => continue,
        }
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(liveness_data)))
}

fn check_validator_participation(
    state: &BeaconState,
    validator_index: usize,
    epoch: u64,
) -> Result<bool, ApiError> {
    let validator = &state.validators[validator_index];
    if !validator.is_active_validator(epoch) {
        return Ok(false);
    }

    let current_epoch = state.get_current_epoch();

    if epoch == current_epoch {
        if let Some(participation) = state.current_epoch_participation.get(validator_index) {
            Ok(*participation > 0)
        } else {
            Ok(false)
        }
    } else if epoch == current_epoch - 1 {
        if let Some(participation) = state.previous_epoch_participation.get(validator_index) {
            Ok(*participation > 0)
        } else {
            Ok(false)
        }
    } else {
        Ok(validator.is_active_validator(epoch))
    }
}
#[get("/validator/attestation_data")]
pub async fn get_attestation_data(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
    query: Query<AttestationQuery>,
) -> Result<impl Responder, ApiError> {
    let store = Store {
        db: db.get_ref().clone(),
        operation_pool: operation_pool.get_ref().clone(),
    };
    let slot = query.slot;
    let current_slot = store.get_current_slot().map_err(|err| {
        ApiError::InternalError(format!("Failed to get current slot, error: {err:?}"))
    })?;

    if slot > current_slot + 1u64 {
        return Err(ApiError::InvalidParameter(format!(
            "slot too far in the future {slot:?}"
        )));
    }

    if slot == current_slot || slot == current_slot + 1u64 {
        let beacon_block_root = db
            .slot_index_provider()
            .get_highest_root()
            .map_err(|err| {
                ApiError::InternalError(format!("Failed to slot_index, error: {err:?}"))
            })?
            .ok_or(ApiError::InternalError(format!(
                "Failed to get block_root for {slot:?}"
            )))?;

        let source = db
            .justified_checkpoint_provider()
            .get_justified_checkpoint()
            .map_err(|err| {
                ApiError::InternalError(format!("Failed to get source checkpoint, error: {err:?}"))
            })?;
        let target = db
            .unrealized_justified_checkpoint_provider()
            .get_unrealized_checkpoint()
            .map_err(|err| {
                ApiError::InternalError(format!("Failed to target checkpoint, error: {err:?}"))
            })?;

        let data = AttestationData {
            slot,
            index: ELECTRA_COMMITTEE_INDEX,
            beacon_block_root,
            source,
            target,
        };

        return Ok(HttpResponse::Ok().json(DataResponse::new(data)));
    }

    let beacon_block_root = db
        .slot_index_provider()
        .get(slot)
        .map_err(|err| ApiError::InternalError(format!("Failed to slot_index, error: {err:?}")))?
        .ok_or(ApiError::InternalError(format!(
            "Failed to get block_root for {slot:?}"
        )))?;

    let source_epoch = compute_epoch_at_slot(slot);
    let target_epoch = source_epoch + 1;

    let epoch_start_slot = compute_start_slot_at_epoch(source_epoch);
    let epoch_end_slot = epoch_start_slot + SLOTS_PER_EPOCH;

    let source_root = db
        .slot_index_provider()
        .get(epoch_start_slot)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get source checkpoint, error: {err:?}"))
        })?
        .ok_or(ApiError::InternalError(
            "Failed to source checkpoint root".to_string(),
        ))?;

    let target_root = db
        .slot_index_provider()
        .get(epoch_end_slot)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to target checkpoint, error: {err:?}"))
        })?
        .ok_or(ApiError::InternalError(
            "Failed to target checkpoint root, error".to_string(),
        ))?;

    let data = AttestationData {
        slot,
        index: ELECTRA_COMMITTEE_INDEX,
        beacon_block_root,
        source: Checkpoint {
            epoch: source_epoch,
            root: source_root,
        },
        target: Checkpoint {
            epoch: target_epoch,
            root: target_root,
        },
    };

    Ok(HttpResponse::Ok().json(DataResponse::new(data)))
}
