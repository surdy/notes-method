//! Shared filesystem paths for the embeddings store.
//!
//! The embed worker (CLI) and the daemon must agree on where `embeddings.db`
//! lives. This mirrors `notesmith-http`'s `data_dir()`/`sanitize_vault_name()`
//! so the store resolves to the same durable location regardless of caller:
//! `data_dir/<vault>/embeddings.db` (ADR 0018 §2/§8).

use std::path::PathBuf;

use crate::{EmbedError, Result};

/// The durable, daemon-owned data directory (honours `XDG_DATA_HOME`, falling
/// back to the platform local-data dir), matching `notesmith-http`.
pub fn data_dir() -> Result<PathBuf> {
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| {
            EmbedError::Io(std::io::Error::other("could not determine data directory"))
        })?;
    Ok(root.join("notesmith"))
}

/// Sanitise a vault name for use as a directory component (matches the daemon).
pub fn sanitize_vault_name(vault_name: &str) -> String {
    vault_name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

/// The canonical path to a vault's `embeddings.db`.
pub fn embeddings_db_path(vault_name: &str) -> Result<PathBuf> {
    Ok(data_dir()?
        .join(sanitize_vault_name(vault_name))
        .join("embeddings.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeddings_path_is_under_data_dir_and_sanitised() {
        let path = embeddings_db_path("work/notes").unwrap();
        assert!(path.ends_with("work_notes/embeddings.db"));
        assert!(path.starts_with(data_dir().unwrap()));
    }
}
