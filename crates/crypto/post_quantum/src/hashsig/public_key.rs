use hashsig::signature::SignatureScheme;
use serde::{Deserialize, Serialize};

use crate::hashsig::HashSigScheme;

type HashSigPublicKey = <HashSigScheme as SignatureScheme>::PublicKey;

#[derive(Serialize, Deserialize)]
pub struct PublicKey {
    pub inner: HashSigPublicKey,
}

impl PublicKey {
    pub fn new(inner: HashSigPublicKey) -> Self {
        Self { inner }
    }
}
