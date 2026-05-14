use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Could not read config at {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Could not write config at {path}: {source}")]
    WriteError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Could not parse config at {path}: {message}")]
    ParseError { path: PathBuf, message: String },

    #[error("Could not serialize config: {message}")]
    SerializeError { message: String },

    #[error("No config directory found")]
    NoConfigDir,

    #[error("No data directory found")]
    NoDataDir,

    #[error("Vault not found: {name}")]
    VaultNotFound { name: String },

    #[error("No vault detected — use --vault or set default_vault in config")]
    NoVaultDetected,
}
