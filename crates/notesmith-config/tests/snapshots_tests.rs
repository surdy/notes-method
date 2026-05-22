use notesmith_config::*;

#[test]
fn snapshot_global_config_default() {
    let config = GlobalConfig::default();
    let serialized = toml::to_string_pretty(&config).unwrap();
    insta::assert_snapshot!("global_config_default", serialized);
}

#[test]
fn snapshot_global_config_full() {
    let mut config = GlobalConfig::default();
    config.default_vault = Some("work".to_string());
    config.vaults.insert(
        "work".to_string(),
        VaultRegistration {
            path: "/Users/surdy/Notes/work".into(),
        },
    );
    config.vaults.insert(
        "personal".to_string(),
        VaultRegistration {
            path: "/Users/surdy/Notes/personal".into(),
        },
    );
    let serialized = toml::to_string_pretty(&config).unwrap();
    insta::assert_snapshot!("global_config_full", serialized);
}

#[test]
fn snapshot_vault_config_full() {
    let config = VaultConfig {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: "work".to_string(),
        homepage: Some("Dashboards/Home.md".to_string()),
        capture: CaptureConfig::default(),
        daily: DailyConfig {
            folder: "Inbox/Daily".to_string(),
            template: "daily-note".to_string(),
            generate_at: Some("06:30".to_string()),
            timezone: Some("America/Los_Angeles".to_string()),
            catch_up: true,
        },
        editor: EditorConfig::default(),
        git: GitConfig {
            enabled: true,
            auto_commit_every: Some("15m".to_string()),
            auto_pull_every: Some("30m".to_string()),
            auto_push_every: Some("30m".to_string()),
            commit_message: Some("notesmith: {{ operation }}".to_string()),
        },
        hooks: HooksConfig {
            on_note_create: Some("Assets/scripts/on-note-create.py".to_string()),
            on_daily_create: Some("Assets/scripts/on-daily-create.py".to_string()),
        },
    };
    let serialized = toml::to_string_pretty(&config).unwrap();
    insta::assert_snapshot!("vault_config_full", serialized);
}
