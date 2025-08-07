use std::{net::IpAddr, path::PathBuf, sync::Arc, time::Duration};

use alloy_primitives::Address;
use clap::Parser;
use ream_network_spec::{cli::beacon_network_parser, networks::BeaconNetworkSpec};
use url::Url;

use crate::cli::constants::{
    DEFAULT_BEACON_API_ENDPOINT, DEFAULT_HTTP_ADDRESS, DEFAULT_KEY_MANAGER_HTTP_PORT,
    DEFAULT_NETWORK, DEFAULT_REQUEST_TIMEOUT,
};

#[derive(Debug, Parser)]
pub struct ValidatorNodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(long, help = "Set HTTP url of the beacon api endpoint", default_value = DEFAULT_BEACON_API_ENDPOINT)]
    pub beacon_api_endpoint: Url,

    #[arg(long, help = "Set HTTP request timeout for beacon api calls", default_value = DEFAULT_REQUEST_TIMEOUT, value_parser = duration_parser)]
    pub request_timeout: Duration,

    #[arg(long, help = "Set HTTP address of the key manager server", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub key_manager_http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port of the key manager server", default_value_t = DEFAULT_KEY_MANAGER_HTTP_PORT)]
    pub key_manager_http_port: u16,

    #[arg(
        long,
        help = "Choose mainnet, holesky, sepolia, hoodi, dev or provide a path to a YAML config file",
        default_value = DEFAULT_NETWORK,
        value_parser = beacon_network_parser
    )]
    pub network: Arc<BeaconNetworkSpec>,

    #[arg(long, help = "The directory for importing keystores")]
    pub import_keystores: PathBuf,

    #[arg(
        long,
        help = "The suggested fee recipient address where staking rewards would go to"
    )]
    pub suggested_fee_recipient: Address,

    #[arg(
        long,
        group = "password_source",
        help = "The plaintext password file to use for keystores"
    )]
    pub password_file: Option<PathBuf>,

    #[arg(
        long,
        group = "password_source",
        help = "The password to use for keystores. It's recommended to use password-file over this in order to prevent your keystore password from appearing in the shell history"
    )]
    pub password: Option<String>,

    #[arg(long, help = "Enable external block builder")]
    pub enable_builder: bool,

    #[arg(
        long,
        help = "Set HTTP url of MEV relay to connect to for external block building. Will only be used if `enable_builder` is passed.",
        requires = "enable_builder"
    )]
    pub mev_relay_url: Option<Url>,
}

pub fn duration_parser(duration_string: &str) -> Result<Duration, String> {
    Ok(Duration::from_secs(duration_string.parse().map_err(
        |err| format!("Could not parse the request timeout: {err:?}"),
    )?))
}
