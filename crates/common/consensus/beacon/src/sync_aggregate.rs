use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitVector, typenum::U512};
use tree_hash_derive::TreeHash;

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default,
)]
pub struct SyncAggregate {
    pub sync_committee_bits: BitVector<U512>,
    pub sync_committee_signature: BLSSignature,
}
