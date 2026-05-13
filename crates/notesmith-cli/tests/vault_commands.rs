//! Integration tests for `notesmith vault` subcommands.
//!
//! These test the vault command logic directly (not by spawning the binary),
//! using the same functions the CLI calls.

use notesmith_cli::commands::vault::{OutputFormat, VaultCommand};
use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
use std::fs;
use tempfile::TempDir;

/// Helper: create a vault on disk with a .notesmith/vault.toml
fn create_vault(root: &std::path::Path, name: &str) {
    let config = VaultConfig {
        name: name.to_string(),
        homepage: None,
        capture: notesmith_config::CaptureConfig {
            folder: "Inbox".to_string(),
            template: "generic-note".to_string(),
        },
        daily: notesmith_config::DailyConfig {
            folder: "Inbox/Daily".to_string(),
            ..Default::default()
        },
        editor: Default::default(),
        git: Default::default(),
        hooks: Default::default(),
    };
    let config_dir = root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    config.save_to(&config_dir.join("vault.toml")).unwrap();
}

/// Helper: create a GlobalConfig pointing at a vault
fn global_with_vault(name: &str, path: &std::path::Path) -> GlobalConfig {
    let mut global = GlobalConfig::default();
    global.vaults.insert(
        name.to_string(),
        VaultRegistration {
            path: path.to_path_buf(),
        },
    );
    global
}

// ========== vault list ==========

#[test]
fn vault_list_empty() {
    let global = GlobalConfig::default();
    // Should not error, just print a message
    let result = VaultCommand::List.run(
        &global,
        None,
        std::path::Path::new("/tmp"),
        OutputFormat::Text,
    );
    assert!(result.is_ok());
}

#[test]
fn vault_list_with_vaults() {
    let tmp = TempDir::new().unwrap();
    let vault_path = tmp.path().join("work");
    create_vault(&vault_path, "work");

    let global = global_with_vault("work", &vault_path);
    let result = VaultCommand::List.run(
        &global,
        None,
        std::path::Path::new("/tmp"),
        OutputFormat::Text,
    );
    assert!(result.is_ok());
}

#[test]
fn vault_list_json_format() {
    let tmp = TempDir::new().unwrap();
    let vault_path = tmp.path().join("notes");
    create_vault(&vault_path, "notes");

    let global = global_with_vault("notes", &vault_path);
    let result = VaultCommand::List.run(
        &global,
        None,
        std::path::Path::new("/tmp"),
        OutputFormat::Json,
    );
    assert!(result.is_ok());
}

// ========== vault detect ==========

#[test]
fn vault_detect_by_directory_walk() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("my-vault");
    let nested = vault_root.join("Customers").join("Acme");
    fs::create_dir_all(&nested).unwrap();
    create_vault(&vault_root, "my-vault");

    let global = GlobalConfig::default();
    let result = VaultCommand::Detect.run(&global, None, &nested, OutputFormat::Text);
    assert!(result.is_ok());
}

#[test]
fn vault_detect_by_explicit_flag() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("work");
    create_vault(&vault_root, "work");

    let global = global_with_vault("work", &vault_root);
    let result = VaultCommand::Detect.run(&global, Some("work"), tmp.path(), OutputFormat::Text);
    assert!(result.is_ok());
}

// ========== vault info ==========

#[test]
fn vault_info_shows_config() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("test-vault");
    create_vault(&vault_root, "test-vault");

    let global = global_with_vault("test-vault", &vault_root);
    let result =
        VaultCommand::Info.run(&global, Some("test-vault"), tmp.path(), OutputFormat::Text);
    assert!(result.is_ok());
}

#[test]
fn vault_info_json_format() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("json-vault");
    create_vault(&vault_root, "json-vault");

    let global = global_with_vault("json-vault", &vault_root);
    let result =
        VaultCommand::Info.run(&global, Some("json-vault"), tmp.path(), OutputFormat::Json);
    assert!(result.is_ok());
}

#[test]
fn vault_info_via_directory_walk() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("walk-vault");
    let subdir = vault_root.join("Inbox");
    fs::create_dir_all(&subdir).unwrap();
    create_vault(&vault_root, "walk-vault");

    let global = GlobalConfig::default();
    let result = VaultCommand::Info.run(&global, None, &subdir, OutputFormat::Text);
    assert!(result.is_ok());
}
