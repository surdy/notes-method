use std::{collections::HashMap, fs, io::ErrorKind, path::Path};

use notesmith_config::{VaultConfig, migration};
use serde::{Deserialize, Serialize};

use crate::routes::{SidebarConfig, SidebarSection};

// ── Sidebar config ───────────────────────────────────────────────────────────

pub fn load_sidebar_config_from_root(root: &Path) -> anyhow::Result<SidebarConfig> {
    let path = root.join(".notesmith").join("sidebar.yaml");
    match fs::read_to_string(&path) {
        Ok(raw) => Ok(serde_yaml::from_str::<SidebarConfig>(&raw)?),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(SidebarConfig { views: vec![] }),
        Err(error) => Err(error.into()),
    }
}

pub fn compute_sidebar_config_hash(vault_root: &Path) -> anyhow::Result<String> {
    let path = vault_root.join(".notesmith").join("sidebar.yaml");
    match fs::read(&path) {
        Ok(content) => Ok(blake3::hash(&content).to_hex().to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

pub fn load_sidebar_config_with_hash(vault_root: &Path) -> anyhow::Result<(SidebarConfig, String)> {
    let path = vault_root.join(".notesmith").join("sidebar.yaml");
    match fs::read_to_string(&path) {
        Ok(content) => {
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let config: SidebarConfig = serde_yaml::from_str(&content)?;
            Ok((config, hash))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Ok((SidebarConfig { views: vec![] }, String::new()))
        }
        Err(error) => Err(error.into()),
    }
}

// ── Vault config ETags and loading ───────────────────────────────────────────

pub fn compute_config_hash(vault_root: &Path) -> anyhow::Result<String> {
    let path = vault_root.join(".notesmith").join("vault.toml");
    match fs::read(&path) {
        Ok(content) => Ok(blake3::hash(&content).to_hex().to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}

pub fn load_vault_config_with_hash(vault_root: &Path) -> anyhow::Result<(VaultConfig, String)> {
    let path = vault_root.join(".notesmith").join("vault.toml");
    match fs::read(&path) {
        Ok(_) => {
            let config = migration::load_and_migrate(vault_root)?;
            let hash = compute_config_hash(vault_root)?;
            Ok((config, hash))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let config = default_vault_config(vault_root);
            Ok((config, String::new()))
        }
        Err(e) => Err(e.into()),
    }
}

fn default_vault_config(vault_root: &Path) -> VaultConfig {
    let name = vault_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vault")
        .to_string();
    VaultConfig {
        name,
        ..Default::default()
    }
}

// ── Vault config validation ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub config: VaultConfig,
    pub hash: String,
    pub path: String,
    pub warnings: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigValidationError {
    pub errors: HashMap<String, String>,
}

/// Returns `(errors, warnings)` for a vault config.
pub fn validate_vault_config(
    config: &VaultConfig,
    vault_root: &Path,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut errors = HashMap::new();
    let mut warnings = HashMap::new();

    // Validate generate_at format (HH:MM)
    if let Some(ref time) = config.daily.generate_at {
        if !is_valid_time_format(time) {
            errors.insert(
                "daily.generate_at".into(),
                format!("Invalid time format '{time}', expected HH:MM"),
            );
        }
    }

    // Validate timezone
    if let Some(ref tz) = config.daily.timezone {
        if tz.parse::<chrono_tz::Tz>().is_err() {
            errors.insert("daily.timezone".into(), format!("Unknown timezone '{tz}'"));
        }
    }

    // Validate duration strings
    for (field, value) in [
        ("git.auto_commit_every", &config.git.auto_commit_every),
        ("git.auto_pull_every", &config.git.auto_pull_every),
        ("git.auto_push_every", &config.git.auto_push_every),
    ] {
        if let Some(dur) = value {
            if parse_duration_str(dur).is_none() {
                errors.insert(
                    field.into(),
                    format!("Invalid duration '{dur}', expected e.g. '5m', '1h', '30s'"),
                );
            }
        }
    }

    // Warn if folders don't exist (non-blocking)
    let capture_path = vault_root.join(&config.capture.folder);
    if !config.capture.folder.is_empty() && !capture_path.exists() {
        warnings.insert(
            "capture.folder".into(),
            format!("Folder '{}' does not exist", config.capture.folder),
        );
    }
    let daily_path = vault_root.join(&config.daily.folder);
    if !config.daily.folder.is_empty() && !daily_path.exists() {
        warnings.insert(
            "daily.folder".into(),
            format!("Folder '{}' does not exist", config.daily.folder),
        );
    }

    (errors, warnings)
}

/// Returns `(errors, warnings)` for a sidebar config.
pub fn validate_sidebar_config(
    config: &SidebarConfig,
    vault_root: &Path,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut errors = HashMap::new();
    let mut warnings = HashMap::new();
    let mut seen_ids = HashMap::new();

    for (view_index, view) in config.views.iter().enumerate() {
        let view_id_key = format!("views[{view_index}].id");
        let view_name_key = format!("views[{view_index}].name");

        if view.id.trim().is_empty() {
            errors.insert(view_id_key.clone(), "View ID cannot be empty".into());
        } else if let Some(previous_index) = seen_ids.insert(view.id.clone(), view_index) {
            errors.insert(
                view_id_key,
                format!(
                    "Duplicate view ID '{}' also used by views[{previous_index}]",
                    view.id
                ),
            );
        }

        if view.name.trim().is_empty() {
            errors.insert(view_name_key, "View name cannot be empty".into());
        }

        for (section_index, section) in view.sections.iter().enumerate() {
            match section {
                SidebarSection::RecentlyViewed { label, limit, .. } => {
                    let label_key = format!("views[{view_index}].sections[{section_index}].label");
                    if label.trim().is_empty() {
                        errors.insert(label_key, "Section label cannot be empty".into());
                    }
                    if *limit == 0 {
                        errors.insert(
                            format!("views[{view_index}].sections[{section_index}].limit"),
                            "Recently viewed limit must be greater than 0".into(),
                        );
                    }
                }
                SidebarSection::CustomFolders { label, folders } => {
                    let label_key = format!("views[{view_index}].sections[{section_index}].label");
                    if label.trim().is_empty() {
                        errors.insert(label_key, "Section label cannot be empty".into());
                    }
                    if folders.is_empty() {
                        errors.insert(
                            format!("views[{view_index}].sections[{section_index}].folders"),
                            "Custom folders section must include at least one folder".into(),
                        );
                    }
                    for (folder_index, folder) in folders.iter().enumerate() {
                        if !vault_root.join(folder).exists() {
                            warnings.insert(
                                format!(
                                    "views[{view_index}].sections[{section_index}].folders[{folder_index}]"
                                ),
                                format!("Folder '{folder}' does not exist"),
                            );
                        }
                    }
                }
                SidebarSection::CustomItems { label, .. } => {
                    let label_key = format!("views[{view_index}].sections[{section_index}].label");
                    if label.trim().is_empty() {
                        errors.insert(label_key, "Section label cannot be empty".into());
                    }
                }
            }
        }
    }

    (errors, warnings)
}

fn is_valid_time_format(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let hour: u32 = parts[0].parse().ok().unwrap_or(99);
    let minute: u32 = parts[1].parse().ok().unwrap_or(99);
    hour < 24 && minute < 60
}

fn parse_duration_str(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;
    match unit {
        "s" => Some(std::time::Duration::from_secs(num)),
        "m" => Some(std::time::Duration::from_secs(num * 60)),
        "h" => Some(std::time::Duration::from_secs(num * 3600)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::{
        FolderSort, ItemSource, RecentlyViewedMode, SidebarConfig, SidebarSection, SidebarView,
        SortDir,
    };
    use std::fs;
    use tempfile::TempDir;

    fn sample_sidebar_config() -> SidebarConfig {
        SidebarConfig {
            views: vec![SidebarView {
                id: "work".into(),
                name: "Work".into(),
                icon: "💼".into(),
                sections: vec![
                    SidebarSection::RecentlyViewed {
                        label: "Recent".into(),
                        mode: RecentlyViewedMode::Both,
                        limit: 5,
                    },
                    SidebarSection::CustomFolders {
                        label: "Folders".into(),
                        folders: vec!["Inbox".into()],
                    },
                ],
                badge_query: None,
            }],
        }
    }

    #[test]
    fn load_sidebar_config_returns_empty_when_file_missing() {
        let temp_dir = TempDir::new().unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        assert_eq!(config, SidebarConfig { views: vec![] });
    }

    #[test]
    fn load_sidebar_config_parses_full_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            r#"views:
  - id: customers
    name: Customers
    icon: "🏢"
    badge_query: "SELECT COUNT(*) FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' WHERE note_type.value = 'customer'"
    sections:
      - type: recently-viewed
        label: Recent
        mode: edited
        limit: 5
      - type: custom-folders
        label: Key Folders
        folders:
          - Customers
          - Projects
      - type: custom-items
        label: Dashboards
        items:
          - name: Pipeline
            icon: "📊"
            source:
              query: "SELECT n.path, n.title FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' WHERE note_type.value = 'dashboard'"
              title_column: title
"#,
        )
        .unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        assert_eq!(config.views.len(), 1);
        let view = &config.views[0];
        assert_eq!(view.id, "customers");
        assert_eq!(view.name, "Customers");
        assert_eq!(view.icon, "🏢");
        assert_eq!(
            view.badge_query,
            Some("SELECT COUNT(*) FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' WHERE note_type.value = 'customer'".to_string())
        );
        assert_eq!(view.sections.len(), 3);

        match &view.sections[0] {
            SidebarSection::RecentlyViewed { label, mode, limit } => {
                assert_eq!(label, "Recent");
                assert_eq!(*mode, RecentlyViewedMode::Edited);
                assert_eq!(*limit, 5);
            }
            other => panic!("expected RecentlyViewed, got {other:?}"),
        }

        match &view.sections[1] {
            SidebarSection::CustomFolders { label, folders } => {
                assert_eq!(label, "Key Folders");
                assert_eq!(folders, &["Customers", "Projects"]);
            }
            other => panic!("expected CustomFolders, got {other:?}"),
        }

        match &view.sections[2] {
            SidebarSection::CustomItems { label, items } => {
                assert_eq!(label, "Dashboards");
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "Pipeline");
            }
            other => panic!("expected CustomItems, got {other:?}"),
        }
    }

    #[test]
    fn load_sidebar_config_handles_folder_source() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            r#"views:
  - id: work
    name: Work
    icon: "💼"
    sections:
      - type: custom-items
        label: Active Projects
        items:
          - name: Projects
            icon: "📁"
            source:
              folder: Projects/Active
              recursive: true
              sort: name
              sort_dir: asc
"#,
        )
        .unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        let view = &config.views[0];
        match &view.sections[0] {
            SidebarSection::CustomItems { items, .. } => match &items[0].source {
                ItemSource::Folder(fs) => {
                    assert_eq!(fs.folder, "Projects/Active");
                    assert!(fs.recursive);
                    assert_eq!(fs.sort, FolderSort::Name);
                    assert_eq!(fs.sort_dir, SortDir::Asc);
                }
                other => panic!("expected FolderSource, got {other:?}"),
            },
            other => panic!("expected CustomItems, got {other:?}"),
        }
    }

    #[test]
    fn load_sidebar_config_handles_query_source() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            r#"views:
  - id: metrics
    name: Metrics
    icon: "📈"
    sections:
      - type: custom-items
        label: Reports
        items:
          - name: Weekly
            icon: "📅"
            source:
              query: "SELECT path, title, status FROM v_reports"
              title_column: title
              subtitle_column: status
              badge_columns:
                - status
"#,
        )
        .unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        let view = &config.views[0];
        match &view.sections[0] {
            SidebarSection::CustomItems { items, .. } => match &items[0].source {
                ItemSource::Query(qs) => {
                    assert_eq!(qs.query, "SELECT path, title, status FROM v_reports");
                    assert_eq!(qs.title_column, Some("title".to_string()));
                    assert_eq!(qs.subtitle_column, Some("status".to_string()));
                    assert_eq!(qs.badge_columns, vec!["status"]);
                }
                other => panic!("expected QuerySource, got {other:?}"),
            },
            other => panic!("expected CustomItems, got {other:?}"),
        }
    }

    #[test]
    fn load_sidebar_config_returns_error_on_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            "views:\n  - id: [invalid yaml\n",
        )
        .unwrap();

        let result = load_sidebar_config_from_root(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn compute_sidebar_config_hash_returns_hash() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        let content = "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n";
        fs::write(config_dir.join("sidebar.yaml"), content).unwrap();

        let hash = compute_sidebar_config_hash(temp_dir.path()).unwrap();

        assert_eq!(hash, blake3::hash(content.as_bytes()).to_hex().to_string());
    }

    #[test]
    fn compute_sidebar_config_hash_returns_empty_when_missing() {
        let temp_dir = TempDir::new().unwrap();

        let hash = compute_sidebar_config_hash(temp_dir.path()).unwrap();

        assert_eq!(hash, "");
    }

    #[test]
    fn load_sidebar_config_with_hash_returns_config_and_hash() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        let content = "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n";
        fs::write(config_dir.join("sidebar.yaml"), content).unwrap();

        let (config, hash) = load_sidebar_config_with_hash(temp_dir.path()).unwrap();

        assert_eq!(config.views.len(), 1);
        assert_eq!(config.views[0].id, "work");
        assert_eq!(hash, blake3::hash(content.as_bytes()).to_hex().to_string());
    }

    #[test]
    fn load_sidebar_config_with_hash_returns_empty_when_missing() {
        let temp_dir = TempDir::new().unwrap();

        let (config, hash) = load_sidebar_config_with_hash(temp_dir.path()).unwrap();

        assert_eq!(config, SidebarConfig { views: vec![] });
        assert_eq!(hash, "");
    }

    #[test]
    fn validate_sidebar_config_passes_for_valid_config() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("Inbox")).unwrap();
        let config = sample_sidebar_config();

        let (errors, warnings) = validate_sidebar_config(&config, temp_dir.path());

        assert!(errors.is_empty(), "errors: {errors:?}");
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
    }

    #[test]
    fn validate_sidebar_config_rejects_duplicate_view_ids() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = sample_sidebar_config();
        config.views.push(SidebarView {
            id: "work".into(),
            name: "Another".into(),
            icon: "📌".into(),
            sections: vec![],
            badge_query: None,
        });

        let (errors, _) = validate_sidebar_config(&config, temp_dir.path());

        assert!(errors.contains_key("views[1].id"));
    }

    #[test]
    fn validate_sidebar_config_rejects_empty_view_id() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = sample_sidebar_config();
        config.views[0].id = "   ".into();

        let (errors, _) = validate_sidebar_config(&config, temp_dir.path());

        assert!(errors.contains_key("views[0].id"));
    }

    #[test]
    fn validate_sidebar_config_warns_missing_folders() {
        let temp_dir = TempDir::new().unwrap();
        let config = sample_sidebar_config();

        let (errors, warnings) = validate_sidebar_config(&config, temp_dir.path());

        assert!(errors.is_empty(), "errors: {errors:?}");
        assert!(warnings.contains_key("views[0].sections[1].folders[0]"));
    }

    // ── Vault config validation tests ────────────────────────────────────────

    #[test]
    fn validate_vault_config_passes_for_defaults() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("Inbox")).unwrap();
        fs::create_dir_all(temp_dir.path().join("Inbox/Daily")).unwrap();

        let config = VaultConfig {
            name: "test".into(),
            ..Default::default()
        };

        let (errors, warnings) = validate_vault_config(&config, temp_dir.path());
        assert!(errors.is_empty(), "errors: {errors:?}");
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
    }

    #[test]
    fn validate_vault_config_rejects_bad_time_format() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = VaultConfig {
            name: "test".into(),
            ..Default::default()
        };
        config.daily.generate_at = Some("25:00".into());

        let (errors, _) = validate_vault_config(&config, temp_dir.path());
        assert!(errors.contains_key("daily.generate_at"));
    }

    #[test]
    fn validate_vault_config_rejects_bad_timezone() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = VaultConfig {
            name: "test".into(),
            ..Default::default()
        };
        config.daily.timezone = Some("Mars/Olympus".into());

        let (errors, _) = validate_vault_config(&config, temp_dir.path());
        assert!(errors.contains_key("daily.timezone"));
    }

    #[test]
    fn validate_vault_config_rejects_bad_duration() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = VaultConfig {
            name: "test".into(),
            ..Default::default()
        };
        config.git.auto_commit_every = Some("banana".into());

        let (errors, _) = validate_vault_config(&config, temp_dir.path());
        assert!(errors.contains_key("git.auto_commit_every"));
    }

    #[test]
    fn validate_vault_config_warns_missing_folders() {
        let temp_dir = TempDir::new().unwrap();
        let config = VaultConfig {
            name: "test".into(),
            ..Default::default()
        };

        let (errors, warnings) = validate_vault_config(&config, temp_dir.path());
        assert!(errors.is_empty());
        // Empty folder defaults ("") don't trigger warnings
        assert!(!warnings.contains_key("capture.folder"));
        assert!(!warnings.contains_key("daily.folder"));
    }

    #[test]
    fn is_valid_time_accepts_valid_times() {
        assert!(is_valid_time_format("00:00"));
        assert!(is_valid_time_format("23:59"));
        assert!(is_valid_time_format("08:30"));
    }

    #[test]
    fn is_valid_time_rejects_invalid_times() {
        assert!(!is_valid_time_format("24:00"));
        assert!(!is_valid_time_format("12:60"));
        assert!(!is_valid_time_format("noon"));
        assert!(!is_valid_time_format("12"));
    }

    #[test]
    fn parse_duration_str_parses_valid_durations() {
        assert_eq!(
            parse_duration_str("30s"),
            Some(std::time::Duration::from_secs(30))
        );
        assert_eq!(
            parse_duration_str("5m"),
            Some(std::time::Duration::from_secs(300))
        );
        assert_eq!(
            parse_duration_str("1h"),
            Some(std::time::Duration::from_secs(3600))
        );
    }

    #[test]
    fn parse_duration_str_rejects_invalid_durations() {
        assert_eq!(parse_duration_str(""), None);
        assert_eq!(parse_duration_str("5d"), None);
        assert_eq!(parse_duration_str("abc"), None);
    }

    // ── ETag hash tests ──────────────────────────────────────────────────────

    #[test]
    fn compute_config_hash_returns_blake3_hash() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        let content = "name = \"test\"\n";
        fs::write(config_dir.join("vault.toml"), content).unwrap();

        let hash = compute_config_hash(temp_dir.path()).unwrap();
        let expected = blake3::hash(content.as_bytes()).to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[test]
    fn compute_config_hash_returns_empty_when_file_missing() {
        let temp_dir = TempDir::new().unwrap();

        let hash = compute_config_hash(temp_dir.path()).unwrap();
        assert_eq!(hash, "");
    }

    #[test]
    fn load_vault_config_with_hash_returns_config_and_hash() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        let content = "name = \"my-vault\"\n";
        fs::write(config_dir.join("vault.toml"), content).unwrap();

        let (config, hash) = load_vault_config_with_hash(temp_dir.path()).unwrap();
        assert_eq!(config.name, "my-vault");
        assert_eq!(hash, blake3::hash(content.as_bytes()).to_hex().to_string());
    }

    #[test]
    fn load_vault_config_with_hash_returns_defaults_when_file_missing() {
        let temp_dir = TempDir::new().unwrap();

        let (config, hash) = load_vault_config_with_hash(temp_dir.path()).unwrap();
        assert_eq!(hash, "");
        assert_eq!(config.capture.folder, "");
        assert_eq!(config.daily.folder, "");
    }
}
