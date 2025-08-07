use std::time::Duration;

use ssz_types::typenum::{U4, U64};

/// The number of attestation subnets used in the gossipsub protocol.
pub type AttestationSubnetCount = U64;

/// The number of sync committee subnets used in the gossipsub aggregation protocol.
pub type SyncCommitteeSubnetCount = U4;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
