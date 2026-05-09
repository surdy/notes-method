use crate::types::VaultPath;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotesmithError {
    #[error("Note not found: {path}")]
    NoteNotFound { path: VaultPath },

    #[error("Vault not found: {name}")]
    VaultNotFound { name: String },

    #[error("Parse error in {path}: {message}")]
    ParseError { path: VaultPath, message: String },

    #[error("Write conflict on {path}: expected hash {expected}, found {actual}")]
    WriteConflict {
        path: VaultPath,
        expected: String,
        actual: String,
    },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}
