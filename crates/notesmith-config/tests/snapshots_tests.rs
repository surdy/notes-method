use notesmith_config::*;
use std::fs;

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
            filename: "{{ date }}".to_string(),
            generate_at: Some("06:30".to_string()),
            timezone: Some("America/Los_Angeles".to_string()),
            catch_up: true,
        },
        periodic: PeriodicConfig::default(),
        editor: EditorConfig::default(),
        appearance: AppearanceConfig {
            theme: "dark".to_string(),
            follow_system: Some(true),
            dark_theme: Some("split".to_string()),
            light_theme: Some("light".to_string()),
            visual_mode: Some("high-contrast".to_string()),
        },
        git: GitConfig {
            enabled: true,
            auto_commit_every: Some("15m".to_string()),
            commit_on_inactivity: None,
            auto_pull_every: Some("30m".to_string()),
            auto_push_every: Some("30m".to_string()),
            commit_message: Some("notesmith: {{ operation }}".to_string()),
        },
        hooks: HooksConfig {
            on_note_create: Some("Assets/scripts/on-note-create.py".to_string()),
            on_note_update: None,
            on_note_route: None,
            on_periodic_create: None,
            on_task_change: None,
            on_field_change: None,
            watch_fields: None,
            on_daily_create: Some("Assets/scripts/on-daily-create.py".to_string()),
        },
        embed: EmbedConfig::default(),
        clip: ClipConfig::default(),
        ingest: IngestConfig::default(),
        transcribe: TranscribeConfig::default(),
    };
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("vault.toml");
    config.save_to(&path).unwrap();
    let serialized = fs::read_to_string(path).unwrap();
    insta::assert_snapshot!("vault_config_full", serialized);
}
