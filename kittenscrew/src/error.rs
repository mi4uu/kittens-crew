//! `KittenError` — the crate's typed error + exit-code mapping (V1).

use crate::{spec, store};

#[derive(Debug, thiserror::Error)]
pub enum KittenError {
    #[error("{0}")]
    Validation(String),
    // V6: constructed by `init` when squeez is unreachable (→ exit 3).
    #[error("squeez binary not found in PATH or ~/.claude/squeez/bin/")]
    SqueezMissing,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("store: {0}")]
    Store(#[from] store::StoreError),
    #[error("spec: {0}")]
    Spec(#[from] spec::SpecError),
}

impl KittenError {
    /// V1: exit 0 ok, 2 validation, 3 squeez-missing, 1 other.
    pub fn exit_code(&self) -> u8 {
        match self {
            KittenError::Validation(_) => 2,
            KittenError::SqueezMissing => 3,
            _ => 1,
        }
    }
}
