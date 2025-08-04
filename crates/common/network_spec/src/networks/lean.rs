use std::sync::{Arc, OnceLock};

use serde::Deserialize;

/// Static specification of the Lean Chain network.
pub static LEAN_NETWORK_SPEC: OnceLock<Arc<LeanNetworkSpec>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct LeanNetworkSpec {
    pub genesis_time: u64,
    pub seconds_per_slot: u64,
}

/// MUST be called only once at the start of the application to initialize static
/// [LeanNetworkSpec].
///
/// The static `LeanNetworkSpec` can be accessed using [lean_network_spec].
///
/// # Panics
///
/// Panics if this function is called more than once.
pub fn set_lean_network_spec(network_spec: Arc<LeanNetworkSpec>) {
    LEAN_NETWORK_SPEC
        .set(network_spec)
        .expect("LeanNetworkSpec should be set only once at the start of the application");
}

/// Returns the static [LeanNetworkSpec] initialized by [set_lean_network_spec].
///
/// # Panics
///
/// Panics if [set_lean_network_spec] wasn't called before this function.
pub fn lean_network_spec() -> Arc<LeanNetworkSpec> {
    LEAN_NETWORK_SPEC
        .get()
        .expect("LeanNetworkSpec wasn't set")
        .clone()
}
