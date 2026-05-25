use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{CURRENT_SCHEMA_VERSION, VaultConfig};

type MigrationFn = fn(&mut VaultConfig) -> Result<()>;

fn next_migration(_version: u32) -> Option<MigrationFn> {
    None
}

/// Run all pending migrations on a vault config.
/// Returns true when the config was modified.
pub fn migrate(config: &mut VaultConfig) -> Result<bool> {
    let initial_version = config.schema_version;

    if initial_version > CURRENT_SCHEMA_VERSION {
        bail!(
            "Unknown schema version {}; cannot migrate vault config for '{}'",
            initial_version,
            config.name
        );
    }

    while let Some(step) = next_migration(config.schema_version) {
        step(config)?;
    }

    if config.schema_version != CURRENT_SCHEMA_VERSION {
        bail!(
            "Unknown schema version {}; cannot migrate vault config for '{}'",
            config.schema_version,
            config.name
        );
    }

    Ok(config.schema_version > initial_version)
}

/// Load config from a vault root, apply pending migrations, and write back when changed.
pub fn load_and_migrate(vault_root: &Path) -> Result<VaultConfig> {
    let mut config = VaultConfig::load_from_vault(vault_root)
        .with_context(|| format!("failed to load vault config from {}", vault_root.display()))?;
    let migrated = migrate(&mut config)?;

    if migrated {
        tracing::info!(
            "Migrated vault config '{}' to schema version {}",
            config.name,
            config.schema_version
        );
        config.save_to_vault(vault_root)?;
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_config(schema_version: u32) -> VaultConfig {
        VaultConfig {
            schema_version,
            name: "work".to_string(),
            ..VaultConfig::default()
        }
    }

    #[test]
    fn migrate_returns_false_for_current_schema_version() {
        let mut config = sample_config(CURRENT_SCHEMA_VERSION);
        let original = config.clone();

        let migrated = migrate(&mut config).unwrap();

        assert!(!migrated);
        assert_eq!(config, original);
    }

    #[test]
    fn migrate_returns_error_for_future_schema_version() {
        let mut config = sample_config(CURRENT_SCHEMA_VERSION + 1);

        let error = migrate(&mut config).unwrap_err().to_string();

        assert!(error.contains("Unknown schema version"));
        assert!(error.contains("work"));
    }

    #[test]
    fn load_and_migrate_loads_current_version_without_rewriting_file() {
        let temp_dir = TempDir::new().unwrap();
        let notesmith_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&notesmith_dir).unwrap();
        let path = notesmith_dir.join("vault.toml");
        let original = "schema_version = 1\nname = \"work\"\n";
        fs::write(&path, original).unwrap();

        let loaded = load_and_migrate(temp_dir.path()).unwrap();

        assert_eq!(loaded.name, "work");
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}
