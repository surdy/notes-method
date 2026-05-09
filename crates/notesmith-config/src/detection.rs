use crate::error::ConfigError;
use crate::global::GlobalConfig;
use crate::vault::VaultConfig;
use std::path::{Path, PathBuf};

pub const VAULT_CONFIG_DIR: &str = ".notesmith";
pub const VAULT_CONFIG_FILE: &str = "vault.toml";

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedVault {
    pub root: PathBuf,
    pub name: String,
    pub source: DetectionSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetectionSource {
    DirectoryWalk,
    ExplicitFlag,
    DefaultConfig,
}

pub fn detect_vault(
    start_dir: &Path,
    explicit_vault: Option<&str>,
    global_config: &GlobalConfig,
) -> Result<DetectedVault, ConfigError> {
    if let Some(vault_name) = explicit_vault {
        if let Some(registration) = global_config.vault(vault_name) {
            let vault_config = VaultConfig::load_from_vault(&registration.path)?;
            return Ok(DetectedVault {
                root: registration.path.clone(),
                name: vault_config.name,
                source: DetectionSource::ExplicitFlag,
            });
        }

        return Err(ConfigError::VaultNotFound {
            name: vault_name.to_string(),
        });
    }

    if let Some(detected) = walk_up_for_vault(start_dir)? {
        return Ok(detected);
    }

    if let Some(default_name) = global_config.effective_default()
        && let Some(registration) = global_config.vault(default_name)
    {
        let vault_config = VaultConfig::load_from_vault(&registration.path)?;
        return Ok(DetectedVault {
            root: registration.path.clone(),
            name: vault_config.name,
            source: DetectionSource::DefaultConfig,
        });
    }

    Err(ConfigError::NoVaultDetected)
}

pub fn walk_up_for_vault(start: &Path) -> Result<Option<DetectedVault>, ConfigError> {
    let mut current = start.to_path_buf();

    loop {
        let config_path = current.join(VAULT_CONFIG_DIR).join(VAULT_CONFIG_FILE);
        if config_path.exists() {
            let vault_config = VaultConfig::load_from(&config_path)?;
            return Ok(Some(DetectedVault {
                root: current,
                name: vault_config.name,
                source: DetectionSource::DirectoryWalk,
            }));
        }

        if !current.pop() {
            break;
        }
    }

    Ok(None)
}
