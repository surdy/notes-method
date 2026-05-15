use notesmith_config::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// ========== Global Config Tests ==========

#[test]
fn global_config_defaults_when_file_missing() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    let config = GlobalConfig::load_from(&path).unwrap();
    assert_eq!(config.daemon.bind, "127.0.0.1:27183");
    assert!(config.daemon.auto_start);
    assert!(config.default_vault.is_none());
    assert!(config.vaults.is_empty());
}

#[test]
fn global_config_parses_full_example() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        r#"
default_vault = "work"

[daemon]
bind = "0.0.0.0:8080"
auto_start = false

[vaults.work]
path = "/home/user/notes/work"

[vaults.personal]
path = "/home/user/notes/personal"
"#,
    )
    .unwrap();

    let config = GlobalConfig::load_from(&path).unwrap();
    assert_eq!(config.daemon.bind, "0.0.0.0:8080");
    assert!(!config.daemon.auto_start);
    assert_eq!(config.default_vault.as_deref(), Some("work"));
    assert_eq!(config.vaults.len(), 2);
    assert_eq!(
        config.vaults["work"].path.to_str().unwrap(),
        "/home/user/notes/work"
    );
}

#[test]
fn global_config_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");

    let mut config = GlobalConfig::default();
    config.default_vault = Some("work".to_string());
    config.vaults.insert(
        "work".to_string(),
        VaultRegistration {
            path: "/tmp/work".into(),
        },
    );

    config.save_to(&path).unwrap();
    let loaded = GlobalConfig::load_from(&path).unwrap();
    assert_eq!(config, loaded);
}

#[test]
fn effective_default_single_vault() {
    let mut config = GlobalConfig::default();
    config.vaults.insert(
        "only-vault".to_string(),
        VaultRegistration {
            path: "/tmp/vault".into(),
        },
    );
    assert_eq!(config.effective_default(), Some("only-vault"));
}

#[test]
fn effective_default_multiple_vaults_requires_explicit() {
    let mut config = GlobalConfig::default();
    config.vaults.insert(
        "a".to_string(),
        VaultRegistration {
            path: "/tmp/a".into(),
        },
    );
    config.vaults.insert(
        "b".to_string(),
        VaultRegistration {
            path: "/tmp/b".into(),
        },
    );
    assert_eq!(config.effective_default(), None);

    config.default_vault = Some("a".to_string());
    assert_eq!(config.effective_default(), Some("a"));
}

#[test]
fn global_config_invalid_toml_returns_parse_error() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(&path, "invalid {{ toml").unwrap();
    let err = GlobalConfig::load_from(&path).unwrap_err();
    assert!(matches!(err, ConfigError::ParseError { .. }));
}

// ========== Vault Config Tests ==========

#[test]
fn vault_config_full_example() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("vault.toml");
    fs::write(
        &path,
        r#"
name = "work"
homepage = "Dashboards/Home.md"

[capture]
folder = "Inbox"
template = "generic-note"

[daily]
folder = "Inbox/Daily"
template = "daily-note"
generate_at = "06:30"
catch_up = true

[editor]
live_preview = true
default_mode = "source"

[git]
enabled = true
auto_commit_every = "15m"
auto_pull_every = "30m"
auto_push_every = "30m"
commit_message = "notesmith: {{ operation }} {{ summary }}"

[hooks]
on_note_create = "Assets/scripts/on-note-create.py"
on_daily_create = "Assets/scripts/on-daily-create.py"
"#,
    )
    .unwrap();

    let config = VaultConfig::load_from(&path).unwrap();
    assert_eq!(config.name, "work");
    assert_eq!(config.homepage.as_deref(), Some("Dashboards/Home.md"));
    assert_eq!(config.capture.folder, "Inbox");
    assert_eq!(config.daily.folder, "Inbox/Daily");
    assert_eq!(config.daily.generate_at.as_deref(), Some("06:30"));
    assert!(config.daily.catch_up);
    assert!(config.editor.live_preview);
    assert_eq!(config.editor.default_mode, "source");
    assert!(config.git.enabled);
    assert_eq!(config.git.auto_commit_every.as_deref(), Some("15m"));
    assert!(config.hooks.on_note_create.is_some());
}

#[test]
fn vault_config_minimal() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("vault.toml");
    fs::write(&path, r#"name = "personal""#).unwrap();

    let config = VaultConfig::load_from(&path).unwrap();
    assert_eq!(config.name, "personal");
    assert!(config.homepage.is_none());
    assert_eq!(config.capture.folder, "");
    assert_eq!(config.daily.folder, "");
    assert!(config.editor.live_preview);
    assert!(!config.git.enabled);
}

#[test]
fn vault_config_defaults_schema_version_for_existing_toml() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("vault.toml");
    fs::write(&path, r#"name = "personal""#).unwrap();

    let config = VaultConfig::load_from(&path).unwrap();
    assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
}

#[test]
fn vault_config_load_from_vault_root() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path();
    let config_dir = vault_root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("vault.toml"), r#"name = "test-vault""#).unwrap();

    let config = VaultConfig::load_from_vault(vault_root).unwrap();
    assert_eq!(config.name, "test-vault");
}

#[test]
fn vault_config_save_to_vault_root() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path();
    let config = VaultConfig {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: "saved-vault".to_string(),
        ..Default::default()
    };

    config.save_to_vault(vault_root).unwrap();

    let loaded = VaultConfig::load_from_vault(vault_root).unwrap();
    assert_eq!(loaded, config);
}

#[test]
fn vault_config_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("vault.toml");

    let config = VaultConfig {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: "roundtrip".to_string(),
        homepage: Some("Home.md".to_string()),
        capture: CaptureConfig::default(),
        daily: DailyConfig::default(),
        editor: EditorConfig::default(),
        git: GitConfig::default(),
        hooks: HooksConfig::default(),
    };

    config.save_to(&path).unwrap();
    let loaded = VaultConfig::load_from(&path).unwrap();
    assert_eq!(config, loaded);
}

#[test]
fn load_and_migrate_rejects_future_schema_version() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path();
    let config_dir = vault_root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("vault.toml"),
        format!(
            "schema_version = {}\nname = \"future\"",
            CURRENT_SCHEMA_VERSION + 1
        ),
    )
    .unwrap();

    let err = notesmith_config::migration::load_and_migrate(vault_root).unwrap_err();
    assert!(err.to_string().contains(&format!(
        "Unknown schema version {}",
        CURRENT_SCHEMA_VERSION + 1
    )));
}

#[test]
fn migrate_is_noop_at_current_schema_version() {
    let mut config = VaultConfig {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: "current".to_string(),
        homepage: Some("Home.md".to_string()),
        ..Default::default()
    };

    let migrated = notesmith_config::migration::migrate(&mut config).unwrap();

    assert!(!migrated);
    assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(config.homepage.as_deref(), Some("Home.md"));
}

#[test]
fn load_and_migrate_loads_existing_vault_config() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path();
    let config_dir = vault_root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("vault.toml"),
        r#"
name = "work"
homepage = "Dashboards/Home.md"

[capture]
folder = "Inbox"
"#,
    )
    .unwrap();

    let config = notesmith_config::migration::load_and_migrate(vault_root).unwrap();

    assert_eq!(config.name, "work");
    assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(config.homepage.as_deref(), Some("Dashboards/Home.md"));
    assert_eq!(config.capture.folder, "Inbox");
}

// ========== Vault Detection Tests ==========

#[test]
fn detect_by_directory_walk() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("my-vault");
    let nested = vault_root.join("Customers").join("Acme");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(vault_root.join(".notesmith")).unwrap();
    fs::write(
        vault_root.join(".notesmith").join("vault.toml"),
        r#"name = "my-vault""#,
    )
    .unwrap();

    let global = GlobalConfig::default();
    let detected = detect_vault(&nested, None, &global).unwrap();
    assert_eq!(detected.name, "my-vault");
    assert_eq!(detected.root, vault_root);
    assert_eq!(detected.source, DetectionSource::DirectoryWalk);
}

#[test]
fn detect_by_explicit_flag() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("work-vault");
    fs::create_dir_all(vault_root.join(".notesmith")).unwrap();
    fs::write(
        vault_root.join(".notesmith").join("vault.toml"),
        r#"name = "work""#,
    )
    .unwrap();

    let mut global = GlobalConfig::default();
    global.vaults.insert(
        "work".to_string(),
        VaultRegistration {
            path: vault_root.clone(),
        },
    );

    let detected = detect_vault(tmp.path(), Some("work"), &global).unwrap();
    assert_eq!(detected.name, "work");
    assert_eq!(detected.source, DetectionSource::ExplicitFlag);
}

#[test]
fn detect_by_default_config() {
    let tmp = TempDir::new().unwrap();
    let vault_root = tmp.path().join("default-vault");
    fs::create_dir_all(vault_root.join(".notesmith")).unwrap();
    fs::write(
        vault_root.join(".notesmith").join("vault.toml"),
        r#"name = "default""#,
    )
    .unwrap();

    let mut global = GlobalConfig::default();
    global.default_vault = Some("default".to_string());
    global.vaults.insert(
        "default".to_string(),
        VaultRegistration {
            path: vault_root.clone(),
        },
    );

    let no_vault_dir = tmp.path().join("random");
    fs::create_dir_all(&no_vault_dir).unwrap();

    let detected = detect_vault(&no_vault_dir, None, &global).unwrap();
    assert_eq!(detected.name, "default");
    assert_eq!(detected.source, DetectionSource::DefaultConfig);
}

#[test]
fn detect_explicit_overrides_walk() {
    let tmp = TempDir::new().unwrap();

    let vault_a = tmp.path().join("vault-a");
    fs::create_dir_all(vault_a.join(".notesmith")).unwrap();
    fs::write(
        vault_a.join(".notesmith").join("vault.toml"),
        r#"name = "a""#,
    )
    .unwrap();

    let vault_b = tmp.path().join("vault-b");
    fs::create_dir_all(vault_b.join(".notesmith")).unwrap();
    fs::write(
        vault_b.join(".notesmith").join("vault.toml"),
        r#"name = "b""#,
    )
    .unwrap();

    let mut global = GlobalConfig::default();
    global.vaults.insert(
        "b".to_string(),
        VaultRegistration {
            path: vault_b.clone(),
        },
    );

    let detected = detect_vault(&vault_a, Some("b"), &global).unwrap();
    assert_eq!(detected.name, "b");
    assert_eq!(detected.source, DetectionSource::ExplicitFlag);
}

#[test]
fn detect_no_vault_returns_error() {
    let tmp = TempDir::new().unwrap();
    let global = GlobalConfig::default();
    let err = detect_vault(tmp.path(), None, &global).unwrap_err();
    assert!(matches!(err, ConfigError::NoVaultDetected));
}

#[test]
fn detect_unknown_vault_name_returns_error() {
    let global = GlobalConfig::default();
    let err = detect_vault(Path::new("/tmp"), Some("nonexistent"), &global).unwrap_err();
    assert!(matches!(err, ConfigError::VaultNotFound { .. }));
}
