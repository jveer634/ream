use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_api_types_beacon::{error::ApiError, id::ID, responses::DataResponse};
use ream_consensus_beacon::{
    bls_to_execution_change::SignedBLSToExecutionChange, voluntary_exit::SignedVoluntaryExit,
};
use ream_network_manager::service::NetworkManagerService;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    channel::GossipMessage,
    gossipsub::beacon::{
        message,
        topics::{GossipTopic, GossipTopicKind},
    },
};
use ream_storage::db::ReamDB;
use ssz::Encode;

use crate::handlers::state::get_state_from_id;

/// GET /eth/v1/beacon/pool/bls_to_execution_changes
#[get("/beacon/pool/bls_to_execution_changes")]
pub async fn get_bls_to_execution_changes(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let signed_bls_to_execution_changes = operation_pool.get_signed_bls_to_execution_changes();
    Ok(HttpResponse::Ok().json(DataResponse::new(signed_bls_to_execution_changes)))
}

/// POST /eth/v1/beacon/pool/bls_to_execution_changes
#[post("/beacon/pool/bls_to_execution_changes")]
pub async fn post_bls_to_execution_changes(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<NetworkManagerService>,
    signed_bls_to_execution_change: Json<SignedBLSToExecutionChange>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let signed_bls_to_execution_change = signed_bls_to_execution_change.into_inner();

    beacon_state
    .validate_bls_to_execution_change(&signed_bls_to_execution_change)
    .map_err(|err| {
        ApiError::BadRequest(format!(
            "Invalid bls_to_execution_change, it will never pass validation so it's rejected: {err:?}"
        ))
    })?;

    operation_pool.insert_signed_bls_to_execution_change(signed_bls_to_execution_change.clone());

    network_manager
        .as_ref()
        .p2p_sender
        .send_gossip(GossipMessage {
            topic: GossipTopic {
                fork: beacon_state.fork.current_version,
                kind: GossipTopicKind::BlsToExecutionChange,
            },
            data: signed_bls_to_execution_change.as_ssz_bytes(),
        });
    Ok(HttpResponse::Ok())
}

/// GET /eth/v1/beacon/pool/voluntary_exits
#[get("/beacon/pool/voluntary_exits")]
pub async fn get_voluntary_exits(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let signed_voluntary_exits = operation_pool.get_signed_voluntary_exits();
    Ok(HttpResponse::Ok().json(DataResponse::new(signed_voluntary_exits)))
}

/// POST /eth/v1/beacon/pool/voluntary_exits
#[post("/beacon/pool/voluntary_exits")]
pub async fn post_voluntary_exits(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<NetworkManagerService>,
    signed_voluntary_exit: Json<SignedVoluntaryExit>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let signed_voluntary_exit = signed_voluntary_exit.into_inner();

    beacon_state
        .validate_voluntary_exit(&signed_voluntary_exit)
        .map_err(|err| {
            ApiError::BadRequest(format!(
                "Invalid voluntary exit, it will never pass validation so it's rejected: {err:?}"
            ))
        })?;

    operation_pool.insert_signed_voluntary_exit(signed_voluntary_exit.clone());

    network_manager
        .as_ref()
        .p2p_sender
        .send_gossip(GossipMessage {
            topic: GossipTopic {
                fork: beacon_state.fork.current_version,
                kind: GossipTopicKind::VoluntaryExit,
            },
            data: signed_voluntary_exit.as_ssz_bytes(),
        });

    Ok(HttpResponse::Ok())
}
