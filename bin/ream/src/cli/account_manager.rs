use clap::Parser;

const DEFAULT_ACTIVATION_EPOCH: u32 = 0;
const DEFAULT_NUM_ACTIVE_EPOCHS: u32 = 1 << 18;

#[derive(Debug, Parser, Default)]
pub struct AccountManagerConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    /// Account lifetime in 2 ** lifetime slots
    #[arg(short, long, default_value_t = 18, value_parser = clap::value_parser!(u32).range(18..))]
    pub lifetime: u32,

    /// Chunk size for messages
    #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(4..))]
    pub chunk_size: u32,

    /// Seed phrase for key generation
    #[arg(short, long)]
    pub seed_phrase: Option<String>,

    /// Optional BIP39 passphrase used with the seed phrase
    #[arg(long)]
    pub passphrase: Option<String>,

    /// Activation epoch for the validator
    #[arg(long, default_value_t = DEFAULT_ACTIVATION_EPOCH)]
    pub activation_epoch: u32,

    /// Number of active epochs
    #[arg(long, default_value_t = DEFAULT_NUM_ACTIVE_EPOCHS)]
    pub num_active_epochs: u32,

    /// Path for keystore directory (relative to data-dir if not absolute)
    #[arg(long)]
    pub keystore_path: Option<String>,
}
