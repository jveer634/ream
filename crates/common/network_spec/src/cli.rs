use std::{fs, sync::Arc};

use serde::de::DeserializeOwned;

use crate::networks::{BeaconNetworkSpec, DEV, HOLESKY, HOODI, LeanNetworkSpec, MAINNET, SEPOLIA};

pub fn beacon_network_parser(network_string: &str) -> Result<Arc<BeaconNetworkSpec>, String> {
    match network_string {
        "mainnet" => Ok(MAINNET.clone()),
        "holesky" => Ok(HOLESKY.clone()),
        "sepolia" => Ok(SEPOLIA.clone()),
        "hoodi" => Ok(HOODI.clone()),
        "dev" => Ok(DEV.clone()),
        path => read_network_spec(path),
    }
}

pub fn lean_network_parser(path: &str) -> Result<Arc<LeanNetworkSpec>, String> {
    read_network_spec(path)
}

fn read_network_spec<T: DeserializeOwned>(path: &str) -> Result<Arc<T>, String> {
    let contents = fs::read_to_string(path).map_err(|err| format!("Failed to read file: {err}"))?;
    Ok(Arc::new(serde_yaml::from_str(&contents).map_err(
        |err| format!("Failed to parse YAML from: {err}"),
    )?))
}
