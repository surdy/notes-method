//! Shared filesystem paths for the transcription queue.
//!
//! The transcribe worker (CLI) and the daemon's enqueue path must agree on
//! where `transcribe.db` lives. Mirrors `notesmith-embed`'s `paths` so the queue
//! resolves to the same durable location regardless of caller:
//! `data_dir/<vault>/transcribe.db` (ADR 0023 §5).

use std::path::PathBuf;

use crate::TranscribeError;

/// The durable, daemon-owned data directory (honours `XDG_DATA_HOME`, falling
/// back to the platform local-data dir), matching `notesmith-http` and
/// `notesmith-embed`.
pub fn data_dir() -> Result<PathBuf, TranscribeError> {
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| TranscribeError::Io("could not determine data directory".into()))?;
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

/// The canonical path to a vault's `transcribe.db`.
pub fn queue_db_path(vault_name: &str) -> Result<PathBuf, TranscribeError> {
    Ok(data_dir()?
        .join(sanitize_vault_name(vault_name))
        .join("transcribe.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_path_is_under_data_dir_and_sanitised() {
        let path = queue_db_path("work/notes").unwrap();
        assert!(path.ends_with("work_notes/transcribe.db"));
        assert!(path.starts_with(data_dir().unwrap()));
    }
}
