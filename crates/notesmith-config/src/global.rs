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
