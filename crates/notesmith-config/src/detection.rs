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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global::VaultRegistration;
    use std::fs;
    use tempfile::TempDir;

    fn write_vault(root: &Path, name: &str) {
        let notesmith_dir = root.join(VAULT_CONFIG_DIR);
        fs::create_dir_all(&notesmith_dir).unwrap();
        fs::write(
            notesmith_dir.join(VAULT_CONFIG_FILE),
            format!("name = \"{name}\"\n"),
        )
        .unwrap();
    }

    fn global_with_vault(name: &str, path: PathBuf, is_default: bool) -> GlobalConfig {
        let mut config = GlobalConfig::default();
        if is_default {
            config.default_vault = Some(name.to_string());
        }
        config
            .vaults
            .insert(name.to_string(), VaultRegistration { path });
        config
    }

    #[test]
    fn detect_vault_uses_registered_explicit_flag() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("work-vault");
        fs::create_dir_all(&root).unwrap();
        write_vault(&root, "work");
        let global_config = global_with_vault("work", root.clone(), false);

        let detected = detect_vault(temp_dir.path(), Some("work"), &global_config).unwrap();

        assert_eq!(
            detected,
            DetectedVault {
                root,
                name: "work".to_string(),
                source: DetectionSource::ExplicitFlag,
            }
        );
    }

    #[test]
    fn detect_vault_returns_vault_not_found_for_unknown_explicit_flag() {
        let error =
            detect_vault(Path::new("."), Some("missing"), &GlobalConfig::default()).unwrap_err();

        assert!(matches!(
            error,
            ConfigError::VaultNotFound { ref name } if name == "missing"
        ));
    }

    #[test]
    fn walk_up_for_vault_finds_parent_vault_config() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("vault-root");
        let nested = root.join("notes").join("projects");
        fs::create_dir_all(&nested).unwrap();
        write_vault(&root, "work");

        let detected = walk_up_for_vault(&nested).unwrap().unwrap();

        assert_eq!(
            detected,
            DetectedVault {
                root,
                name: "work".to_string(),
                source: DetectionSource::DirectoryWalk,
            }
        );
    }

    #[test]
    fn walk_up_for_vault_returns_none_when_no_config_exists() {
        let temp_dir = TempDir::new().unwrap();
        let start = temp_dir.path().join("notes").join("projects");
        fs::create_dir_all(&start).unwrap();

        assert_eq!(walk_up_for_vault(&start).unwrap(), None);
    }

    #[test]
    fn detect_vault_falls_back_to_default_config() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("default-vault");
        let outside = temp_dir.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        write_vault(&root, "default-vault");
        let global_config = global_with_vault("work", root.clone(), true);

        let detected = detect_vault(&outside, None, &global_config).unwrap();

        assert_eq!(detected.root, root);
        assert_eq!(detected.name, "default-vault");
        assert_eq!(detected.source, DetectionSource::DefaultConfig);
    }
}
