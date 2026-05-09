use crate::note::Note;
use crate::types::VaultPath;
use std::path::Path;

/// Result type alias for Notesmith operations
pub type Result<T> = std::result::Result<T, crate::error::NotesmithError>;

/// Low-level vault filesystem operations.
/// Isolates the filesystem implementation (previously TurboVault, now native).
pub trait VaultEngine: Send + Sync {
    fn scan(&self, root: &Path) -> Result<Vec<Note>>;
    fn read(&self, root: &Path, path: &VaultPath) -> Result<String>;
    fn write(
        &self,
        root: &Path,
        path: &VaultPath,
        expected_hash: Option<&str>,
        content: &str,
    ) -> Result<WriteResult>;
    fn delete(&self, root: &Path, path: &VaultPath) -> Result<()>;
    fn move_path(&self, root: &Path, from: &VaultPath, to: &VaultPath) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteResult {
    Written { hash: String },
    Conflict { expected: String, actual: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_result_variants() {
        let written = WriteResult::Written {
            hash: "abc123".to_string(),
        };
        assert!(matches!(written, WriteResult::Written { .. }));

        let conflict = WriteResult::Conflict {
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(matches!(conflict, WriteResult::Conflict { .. }));
    }
}
