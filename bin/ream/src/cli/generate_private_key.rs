use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct GeneratePrivateKeyConfig {
    #[arg(long, help = "Output path for the hex encoded secp256k1 keypair")]
    pub output_path: PathBuf,
}
