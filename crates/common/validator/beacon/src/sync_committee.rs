use std::{cmp::max, collections::HashSet};

use alloy_primitives::B256;
use anyhow::{anyhow, bail, ensure};
use ream_bls::{
    BLSSignature, PrivateKey,
    traits::{Aggregatable, Signable},
};
use ream_consensus_beacon::{
    electra::{beacon_block::BeaconBlock, beacon_state::BeaconState},
    sync_aggregate::SyncAggregate,
};
use ream_consensus_misc::{
    constants::{DOMAIN_SYNC_COMMITTEE, EPOCHS_PER_SYNC_COMMITTEE_PERIOD, SYNC_COMMITTEE_SIZE},
    misc::{compute_domain, compute_epoch_at_slot, compute_signing_root},
};
use ream_network_spec::networks::beacon_network_spec;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitVector, typenum::U512};
use tree_hash_derive::TreeHash;

use crate::{
    constants::{SYNC_COMMITTEE_SUBNET_COUNT, TARGET_AGGREGATORS_PER_COMMITTEE},
    contribution_and_proof::SyncCommitteeContribution,
    hash_signature_prefix_to_u64,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, TreeHash)]
pub struct SyncAggregatorSelectionData {
    pub slot: u64,
    pub subcommittee_index: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommitteeMessage {
    pub slot: u64,
    pub beacon_block_root: B256,
    pub validator_index: u64,
    pub signature: BLSSignature,
}

pub fn compute_sync_committee_period(epoch: u64) -> u64 {
    epoch / EPOCHS_PER_SYNC_COMMITTEE_PERIOD
}

pub fn is_assigned_to_sync_committee(
    state: &BeaconState,
    epoch: u64,
    validator_index: u64,
) -> anyhow::Result<bool> {
    let sync_committee_period = compute_sync_committee_period(epoch);
    let current_epoch = state.get_current_epoch();
    let current_sync_committee_period = compute_sync_committee_period(current_epoch);
    let next_sync_committee_period = current_sync_committee_period + 1;
    ensure!(
        sync_committee_period == current_sync_committee_period
            || sync_committee_period == next_sync_committee_period,
        "Validator is not assigned to sync committee for period {sync_committee_period} (current: {current_sync_committee_period}, next: {next_sync_committee_period})"
    );

    let Some(validator) = state.validators.get(validator_index as usize) else {
        bail!("Validator index out of bounds: {validator_index}");
    };

    if sync_committee_period == current_sync_committee_period {
        Ok(state
            .current_sync_committee
            .public_keys
            .contains(&validator.public_key))
    } else {
        Ok(state
            .next_sync_committee
            .public_keys
            .contains(&validator.public_key))
    }
}

pub fn compute_subnets_for_sync_committee(
    state: &BeaconState,
    validator_index: u64,
) -> anyhow::Result<HashSet<u64>> {
    let next_slot_epoch = compute_epoch_at_slot(state.slot + 1);
    let sync_committee = if compute_sync_committee_period(state.get_current_epoch())
        == compute_sync_committee_period(next_slot_epoch)
    {
        &state.current_sync_committee
    } else {
        &state.next_sync_committee
    };

    let Some(target_validator) = state.validators.get(validator_index as usize) else {
        bail!("Validator index out of bounds: {validator_index}");
    };

    let sync_committee_indices: Vec<usize> = sync_committee
        .public_keys
        .iter()
        .enumerate()
        .filter(|(_, public_key)| **public_key == target_validator.public_key)
        .map(|(index, _)| index)
        .collect();

    Ok(sync_committee_indices
        .into_iter()
        .map(|index| index as u64 / (SYNC_COMMITTEE_SIZE / SYNC_COMMITTEE_SUBNET_COUNT))
        .collect())
}

pub fn process_sync_committee_contributions(
    block: &mut BeaconBlock,
    contributions: HashSet<SyncCommitteeContribution>,
) -> anyhow::Result<()> {
    let mut sync_committee_bits = BitVector::<U512>::new();
    let mut signatures = vec![];
    let sync_subcommittee_size = SYNC_COMMITTEE_SIZE / SYNC_COMMITTEE_SUBNET_COUNT;

    for contribution in contributions {
        for (index, participated) in contribution.aggregation_bits.iter().enumerate() {
            if participated {
                let participant_index =
                    sync_subcommittee_size * contribution.subcommittee_index + index as u64;
                sync_committee_bits
                    .set(participant_index as usize, true)
                    .map_err(|err| anyhow!("Failed to set sync committee bit: {err:?}"))?;
            }
        }
        signatures.push(contribution.signature);
    }

    block.body.sync_aggregate = SyncAggregate {
        sync_committee_bits,
        sync_committee_signature: BLSSignature::aggregate(
            &signatures.iter().collect::<Vec<&BLSSignature>>(),
        )?,
    };
    Ok(())
}

pub fn get_sync_committee_selection_proof(
    slot: u64,
    subcommittee_index: u64,
    private_key: &PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = compute_domain(
        DOMAIN_SYNC_COMMITTEE,
        Some(beacon_network_spec().electra_fork_version),
        None,
    );
    let signing_root = compute_signing_root(
        SyncAggregatorSelectionData {
            slot,
            subcommittee_index,
        },
        domain,
    );
    Ok(private_key.sign(signing_root.as_ref())?)
}

pub fn is_sync_committee_aggregator(signature: &BLSSignature) -> bool {
    hash_signature_prefix_to_u64(signature).is_multiple_of(max(
        1,
        SYNC_COMMITTEE_SIZE / SYNC_COMMITTEE_SUBNET_COUNT / TARGET_AGGREGATORS_PER_COMMITTEE,
    ))
}

pub fn get_sync_committee_message(
    state: &BeaconState,
    beacon_block_root: B256,
    validator_index: u64,
    private_key: PrivateKey,
) -> anyhow::Result<SyncCommitteeMessage> {
    let epoch = state.get_current_epoch();
    let domain = state.get_domain(DOMAIN_SYNC_COMMITTEE, Some(epoch));
    let signing_root = compute_signing_root(beacon_block_root, domain);

    Ok(SyncCommitteeMessage {
        slot: state.slot,
        beacon_block_root,
        validator_index,
        signature: private_key.sign(signing_root.as_ref())?,
    })
}
