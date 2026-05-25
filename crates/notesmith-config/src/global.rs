use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub default_vault: Option<String>,
    #[serde(default)]
    pub vaults: BTreeMap<String, VaultRegistration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

fn default_bind() -> String {
    "127.0.0.1:27183".to_string()
}

fn default_auto_start() -> bool {
    true
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            auto_start: default_auto_start(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultRegistration {
    pub path: PathBuf,
}

impl GlobalConfig {
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadError {
            path: path.to_path_buf(),
            source,
        })?;

        toml::from_str(&content).map_err(|error| ConfigError::ParseError {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    }

    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_path().ok_or(ConfigError::NoConfigDir)?;
        Self::load_from(&path)
    }

    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(dirs::config_dir)
            .map(|dir| dir.join("notesmith").join("config.toml"))
    }

    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            })?;
        }

        let content =
            toml::to_string_pretty(self).map_err(|error| ConfigError::SerializeError {
                message: error.to_string(),
            })?;

        std::fs::write(path, content).map_err(|source| ConfigError::WriteError {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn vault(&self, name: &str) -> Option<&VaultRegistration> {
        self.vaults.get(name)
    }

    pub fn effective_default(&self) -> Option<&str> {
        self.default_vault.as_deref().or_else(|| {
            if self.vaults.len() == 1 {
                self.vaults.keys().next().map(String::as_str)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_global_config() -> GlobalConfig {
        let mut config = GlobalConfig {
            daemon: DaemonConfig {
                bind: "0.0.0.0:8080".to_string(),
                auto_start: false,
            },
            default_vault: Some("work".to_string()),
            vaults: BTreeMap::new(),
        };
        config.vaults.insert(
            "work".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/work"),
            },
        );
        config.vaults.insert(
            "personal".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/personal"),
            },
        );
        config
    }

    #[test]
    fn load_from_returns_default_when_path_does_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("missing.toml");

        let config = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(config, GlobalConfig::default());
    }

    #[test]
    fn load_from_reads_valid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
default_vault = "work"

[daemon]
bind = "0.0.0.0:8080"
auto_start = false

[vaults.work]
path = "/vaults/work"

[vaults.personal]
path = "/vaults/personal"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(config.default_vault.as_deref(), Some("work"));
        assert_eq!(config.daemon.bind, "0.0.0.0:8080");
        assert!(!config.daemon.auto_start);
        assert_eq!(config.vaults["work"].path, PathBuf::from("/vaults/work"));
        assert_eq!(
            config.vaults["personal"].path,
            PathBuf::from("/vaults/personal")
        );
    }

    #[test]
    fn load_from_returns_parse_error_for_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(&path, "invalid {{ toml").unwrap();

        let error = GlobalConfig::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::ParseError { .. }));
    }

    #[test]
    fn save_to_round_trips_through_disk() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nested").join("config.toml");
        let expected = sample_global_config();

        expected.save_to(&path).unwrap();
        let actual = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn vault_looks_up_registered_entries_and_missing_names() {
        let config = sample_global_config();

        assert_eq!(
            config
                .vault("work")
                .map(|registration| registration.path.clone()),
            Some(PathBuf::from("/vaults/work"))
        );
        assert!(config.vault("missing").is_none());
    }

    #[test]
    fn effective_default_prefers_explicit_default_then_single_vault_fallback() {
        let config = sample_global_config();
        assert_eq!(config.effective_default(), Some("work"));

        let mut single = GlobalConfig::default();
        single.vaults.insert(
            "solo".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/solo"),
            },
        );
        assert_eq!(single.effective_default(), Some("solo"));

        let mut multiple = GlobalConfig::default();
        multiple.vaults.insert(
            "a".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/a"),
            },
        );
        multiple.vaults.insert(
            "b".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/b"),
            },
        );
        assert_eq!(multiple.effective_default(), None);
    }
}
