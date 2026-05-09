use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// A vault-relative path to a note (e.g., "Customers/Acme/Acme Corp.md")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VaultPath(pub String);

impl VaultPath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }

    /// Returns the file name without extension
    pub fn stem(&self) -> Option<&str> {
        std::path::Path::new(&self.0)
            .file_stem()
            .and_then(|s| s.to_str())
    }

    /// Returns the parent directory path
    pub fn parent(&self) -> Option<&str> {
        std::path::Path::new(&self.0)
            .parent()
            .and_then(|p| p.to_str())
    }
}

impl fmt::Display for VaultPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for VaultPath {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A named vault identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VaultName(pub String);

impl VaultName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VaultName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_path_stem() {
        let p = VaultPath::new("Customers/Acme/Acme Corp.md");
        assert_eq!(p.stem(), Some("Acme Corp"));
    }

    #[test]
    fn vault_path_parent() {
        let p = VaultPath::new("Customers/Acme/Acme Corp.md");
        assert_eq!(p.parent(), Some("Customers/Acme"));
    }

    #[test]
    fn vault_path_display() {
        let p = VaultPath::new("Inbox/Daily/2025-01-15.md");
        assert_eq!(format!("{p}"), "Inbox/Daily/2025-01-15.md");
    }

    #[test]
    fn vault_path_from_str() {
        let p: VaultPath = "test.md".into();
        assert_eq!(p.as_str(), "test.md");
    }
}
