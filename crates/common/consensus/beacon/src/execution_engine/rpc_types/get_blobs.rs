use ream_consensus_misc::constants::BYTES_PER_BLOB;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, serde_utils::hex_fixed_vec, typenum::U131072};
use tree_hash_derive::TreeHash;

use crate::{blob_sidecar::BlobSidecar, polynomial_commitments::kzg_proof::KZGProof};

#[derive(
    Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Decode, Encode, TreeHash, Default,
)]
#[serde(transparent)]
pub struct Blob {
    #[serde(with = "hex_fixed_vec")]
    pub inner: FixedVector<u8, U131072>,
}

impl Blob {
    pub fn to_fixed_bytes(&self) -> [u8; BYTES_PER_BLOB] {
        let mut fixed_array = [0u8; BYTES_PER_BLOB];
        fixed_array.copy_from_slice(&self.inner);
        fixed_array
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Decode, Encode, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlobAndProofV1 {
    pub blob: Blob,
    pub proof: KZGProof,
}

impl From<BlobSidecar> for BlobAndProofV1 {
    fn from(blob_sidecar: BlobSidecar) -> Self {
        BlobAndProofV1 {
            blob: blob_sidecar.blob,
            proof: blob_sidecar.kzg_proof,
        }
    }
}
