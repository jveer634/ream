use std::str::FromStr;

use bip39::Mnemonic;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use sha2::{Digest, Sha256};

/// Derives a seed for [ChaCha20Rng] based on the provided seed phrase, wallet index, and
/// passphrase.
pub fn derive_seed_with_user_input(
    seed_phrase: &str,
    wallet_index: u32,
    passphrase: &str,
) -> <ChaCha20Rng as SeedableRng>::Seed {
    // Parse the mnemonic phrase
    let mnemonic = Mnemonic::from_str(seed_phrase).expect("Invalid mnemonic phrase");

    // Generate seed from mnemonic using provided passphrase
    let seed = mnemonic.to_seed(passphrase);

    // Create a deterministic seed based on the original seed and wallet index
    let mut hasher = Sha256::new();
    hasher.update(seed);
    hasher.update(wallet_index.to_be_bytes());

    hasher.finalize().into()
}
