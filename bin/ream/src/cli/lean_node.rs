use std::sync::Arc;

use clap::Parser;
use ream_network_spec::{cli::lean_network_parser, networks::LeanNetworkSpec};

#[derive(Debug, Parser)]
pub struct LeanNodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(
      long,
      help = "Provide a path to a YAML config file",
      value_parser = lean_network_parser
  )]
    pub network: Arc<LeanNetworkSpec>,
}
