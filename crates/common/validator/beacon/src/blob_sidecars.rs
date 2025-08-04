use ream_network_spec::networks::beacon_network_spec;

pub fn compute_subnet_for_blob_sidecar(blob_index: u64) -> u64 {
    blob_index % beacon_network_spec().blob_sidecar_subnet_count_electra
}
