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
