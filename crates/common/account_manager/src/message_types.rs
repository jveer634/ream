#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    Attestation,
    Block,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::Attestation => write!(f, "Attestation"),
            MessageType::Block => write!(f, "Block"),
        }
    }
}

impl MessageType {
    /// Method to get all enum variants as an array
    pub const fn all() -> [MessageType; 2] {
        [MessageType::Attestation, MessageType::Block]
    }

    /// Iterator method to loop through all variants
    pub fn iter() -> impl Iterator<Item = MessageType> + 'static {
        Self::all().into_iter()
    }

    /// Method to return the number of enum variants
    pub const fn count() -> usize {
        Self::all().len()
    }
}
