use crate::error::ConfigError;
use notesmith_core::PeriodKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current schema version for vault-local `vault.toml` files.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VaultConfig {
    pub schema_version: u32,
    pub name: String,
    pub homepage: Option<String>,
    pub capture: CaptureConfig,
    pub daily: DailyConfig,
    pub periodic: PeriodicConfig,
    pub editor: EditorConfig,
    pub appearance: AppearanceConfig,
    pub git: GitConfig,
    pub hooks: HooksConfig,
    pub embed: EmbedConfig,
    pub clip: ClipConfig,
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

impl Default for VaultConfig {
    fn default() -> Self {
        let periodic = PeriodicConfig::default();
        Self {
            schema_version: default_schema_version(),
            name: String::new(),
            homepage: None,
            capture: Default::default(),
            daily: periodic
                .daily
                .as_ref()
                .map(DailyConfig::from_periodic)
                .unwrap_or_default(),
            periodic,
            editor: Default::default(),
            appearance: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
            embed: Default::default(),
            clip: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureConfig {
    #[serde(default)]
    pub folder: String,
    #[serde(default = "default_capture_template")]
    pub template: String,
}

fn default_capture_template() -> String {
    "generic-note".to_string()
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            folder: String::new(),
            template: default_capture_template(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyConfig {
    #[serde(default = "default_daily_folder")]
    pub folder: String,
    #[serde(default = "default_daily_template")]
    pub template: String,
    #[serde(default = "default_daily_filename")]
    pub filename: String,
    #[serde(default)]
    pub generate_at: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub catch_up: bool,
}

fn default_daily_folder() -> String {
    String::new()
}

fn default_daily_template() -> String {
    "daily-note".to_string()
}

fn default_daily_filename() -> String {
    "{{ date }}".to_string()
}

fn default_weekly_filename() -> String {
    "Week {{ week }}".to_string()
}

fn default_monthly_filename() -> String {
    "{{ month }}".to_string()
}

fn default_quarterly_filename() -> String {
    "{{ quarter }}".to_string()
}

fn default_yearly_filename() -> String {
    "{{ year }}".to_string()
}

fn default_filename_for(kind: PeriodKind) -> String {
    match kind {
        PeriodKind::Daily => default_daily_filename(),
        PeriodKind::Weekly => default_weekly_filename(),
        PeriodKind::Monthly => default_monthly_filename(),
        PeriodKind::Quarterly => default_quarterly_filename(),
        PeriodKind::Yearly => default_yearly_filename(),
    }
}

impl Default for DailyConfig {
    fn default() -> Self {
        Self {
            folder: default_daily_folder(),
            template: default_daily_template(),
            filename: default_daily_filename(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        }
    }
}

impl DailyConfig {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn from_periodic(config: &PeriodKindConfig) -> Self {
        Self {
            folder: config.folder.clone(),
            template: config
                .template
                .clone()
                .unwrap_or_else(default_daily_template),
            filename: config.filename.clone(),
            generate_at: config.generate_at.clone(),
            timezone: config.timezone.clone(),
            catch_up: config.catch_up,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PeriodicConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily: Option<PeriodKindConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly: Option<PeriodKindConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly: Option<PeriodKindConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quarterly: Option<PeriodKindConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yearly: Option<PeriodKindConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeriodKindConfig {
    pub folder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default)]
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default)]
    pub catch_up: bool,
}

impl PeriodKindConfig {
    pub fn for_kind(kind: PeriodKind) -> Self {
        let mut config = Self {
            folder: String::new(),
            template: None,
            filename: String::new(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        };
        config.normalize(kind);
        config
    }

    pub fn from_daily_compat(daily: &DailyConfig) -> Self {
        let mut config = Self {
            folder: daily.folder.clone(),
            template: Some(daily.template.clone()),
            filename: daily.filename.clone(),
            generate_at: daily.generate_at.clone(),
            timezone: daily.timezone.clone(),
            catch_up: daily.catch_up,
        };
        config.normalize(PeriodKind::Daily);
        config
    }

    pub fn normalize(&mut self, kind: PeriodKind) {
        if self.filename.trim().is_empty() {
            self.filename = default_filename_for(kind);
        }
        if kind == PeriodKind::Daily && self.template.is_none() {
            self.template = Some(default_daily_template());
        }
    }

    pub fn extract_period_key(&self, kind: PeriodKind, stem: &str) -> Option<String> {
        let key = extract_key_from_filename_template(&self.filename, kind, stem)
            .unwrap_or_else(|| stem.to_string());
        kind.bounds_for_key(&key).map(|_| key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodicNoteMatch {
    pub kind: PeriodKind,
    pub key: String,
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
}

impl PeriodicConfig {
    pub fn normalize(&mut self) {
        self.normalize_kind(PeriodKind::Daily);
        self.normalize_kind(PeriodKind::Weekly);
        self.normalize_kind(PeriodKind::Monthly);
        self.normalize_kind(PeriodKind::Quarterly);
        self.normalize_kind(PeriodKind::Yearly);
    }

    fn normalize_kind(&mut self, kind: PeriodKind) {
        if let Some(config) = self.kind_config_mut(kind) {
            config.normalize(kind);
        }
    }

    pub fn kind_config(&self, kind: PeriodKind) -> Option<&PeriodKindConfig> {
        match kind {
            PeriodKind::Daily => self.daily.as_ref(),
            PeriodKind::Weekly => self.weekly.as_ref(),
            PeriodKind::Monthly => self.monthly.as_ref(),
            PeriodKind::Quarterly => self.quarterly.as_ref(),
            PeriodKind::Yearly => self.yearly.as_ref(),
        }
    }

    pub fn kind_config_mut(&mut self, kind: PeriodKind) -> Option<&mut PeriodKindConfig> {
        match kind {
            PeriodKind::Daily => self.daily.as_mut(),
            PeriodKind::Weekly => self.weekly.as_mut(),
            PeriodKind::Monthly => self.monthly.as_mut(),
            PeriodKind::Quarterly => self.quarterly.as_mut(),
            PeriodKind::Yearly => self.yearly.as_mut(),
        }
    }

    pub fn match_note_path(&self, path: &str) -> Option<PeriodicNoteMatch> {
        let (parent, stem) = split_parent_and_stem(path)?;
        for kind in PeriodKind::ALL {
            let Some(config) = self.kind_config(kind) else {
                continue;
            };
            if normalize_folder(parent) != normalize_folder(&config.folder) {
                continue;
            }
            let key = config.extract_period_key(kind, stem)?;
            let (period_start, period_end) = kind.bounds_for_key(&key)?;
            return Some(PeriodicNoteMatch {
                kind,
                key,
                period_start,
                period_end,
            });
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorConfig {
    #[serde(default = "default_true")]
    pub live_preview: bool,
    #[serde(default = "default_editor_mode")]
    pub default_mode: String,
    #[serde(default)]
    pub strict_line_breaks: bool,
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,
    #[serde(default = "default_true")]
    pub hide_duplicate_h1: bool,
    #[serde(default)]
    pub paste_url_image_whitelist: String,
}

fn default_true() -> bool {
    true
}

fn default_editor_mode() -> String {
    "source".to_string()
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            live_preview: default_true(),
            default_mode: default_editor_mode(),
            strict_line_breaks: false,
            show_line_numbers: default_true(),
            hide_duplicate_h1: default_true(),
            paste_url_image_whitelist: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppearanceConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "system".to_string()
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GitConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_commit_every: Option<String>,
    /// Commit a checkpoint after this much editor/working-tree inactivity
    /// (e.g. `"120s"`, `"2m"`). Local versioning; no remote required. The
    /// desktop editor flushes unsaved buffers to disk before committing.
    #[serde(default)]
    pub commit_on_inactivity: Option<String>,
    #[serde(default)]
    pub auto_pull_every: Option<String>,
    #[serde(default)]
    pub auto_push_every: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub on_note_create: Option<String>,
    #[serde(default)]
    pub on_note_update: Option<String>,
    #[serde(default)]
    pub on_note_route: Option<String>,
    #[serde(default)]
    pub on_periodic_create: Option<String>,
    #[serde(default)]
    pub on_task_change: Option<String>,
    #[serde(default)]
    pub on_field_change: Option<String>,
    /// For on_field_change: only fire when these fields change
    #[serde(default)]
    pub watch_fields: Option<Vec<String>>,
    /// Legacy alias for on_periodic_create (backward compat)
    #[serde(default)]
    pub on_daily_create: Option<String>,
}

/// Per-vault embedding / semantic-search settings (ADR 0018 §9.1). Gates both
/// the embed worker scheduler and the query-time hybrid search path for this
/// vault, in every build. Off by default so embedding cost (disk, worker CPU,
/// first-run model load) is paid only where it's wanted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EmbedConfig {
    /// When `true`, the daemon runs embed passes for this vault and serves
    /// hybrid (lexical + semantic) search. When `false` (default), the vault is
    /// lexical-only and no embedding work is scheduled. `#[serde(default)]` so
    /// older `vault.toml` files without an `[embed]` table still parse.
    #[serde(default)]
    pub enabled: bool,
}

/// Web-clipper configuration ([ADR 0020](../../docs/adr/0020-web-clipper.md)).
///
/// `#[serde(default)]` throughout so older `vault.toml` files without a `[clip]`
/// table still parse and clipping is available out of the box.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipConfig {
    /// When `true` (default), the vault accepts web clips via `POST /clip`.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Folder clips are written to. When empty, the capture folder is used.
    #[serde(default)]
    pub folder: String,
    /// When `true` (default), images in clipped pages are downloaded into the
    /// vault; when `false`, remote image URLs are kept. (Download is P2.)
    #[serde(default = "default_true")]
    pub download_images: bool,
}

impl Default for ClipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            folder: String::new(),
            download_images: true,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawVaultConfig {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    name: String,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    capture: CaptureConfig,
    #[serde(default)]
    daily: DailyConfig,
    #[serde(default)]
    periodic: PeriodicConfig,
    #[serde(default)]
    editor: EditorConfig,
    #[serde(default)]
    appearance: AppearanceConfig,
    #[serde(default)]
    git: GitConfig,
    #[serde(default)]
    hooks: HooksConfig,
    #[serde(default)]
    embed: EmbedConfig,
    #[serde(default)]
    clip: ClipConfig,
}

#[derive(Serialize)]
struct PersistedVaultConfig<'a> {
    schema_version: u32,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    homepage: &'a Option<String>,
    capture: &'a CaptureConfig,
    #[serde(skip_serializing_if = "periodic_config_is_empty")]
    periodic: &'a PeriodicConfig,
    editor: &'a EditorConfig,
    appearance: &'a AppearanceConfig,
    git: &'a GitConfig,
    hooks: &'a HooksConfig,
    #[serde(skip_serializing_if = "embed_config_is_default")]
    embed: &'a EmbedConfig,
    #[serde(skip_serializing_if = "clip_config_is_default")]
    clip: &'a ClipConfig,
}

fn embed_config_is_default(config: &EmbedConfig) -> bool {
    *config == EmbedConfig::default()
}

fn clip_config_is_default(config: &ClipConfig) -> bool {
    *config == ClipConfig::default()
}

fn periodic_config_is_empty(config: &PeriodicConfig) -> bool {
    config.daily.is_none()
        && config.weekly.is_none()
        && config.monthly.is_none()
        && config.quarterly.is_none()
        && config.yearly.is_none()
}

impl<'de> Deserialize<'de> for VaultConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawVaultConfig::deserialize(deserializer)?;
        let mut periodic = raw.periodic;
        periodic.normalize();

        if periodic.daily.is_none() && !raw.daily.is_default() {
            periodic.daily = Some(PeriodKindConfig::from_daily_compat(&raw.daily));
        } else if let Some(ref mut daily) = periodic.daily {
            if daily.folder.is_empty() && !raw.daily.folder.is_empty() {
                daily.folder = raw.daily.folder.clone();
            }
            if daily.template.is_none() && !raw.daily.template.is_empty() {
                daily.template = Some(raw.daily.template.clone());
            }
            if daily.filename.trim().is_empty() {
                daily.filename = raw.daily.filename.clone();
            }
            if daily.generate_at.is_none() {
                daily.generate_at = raw.daily.generate_at.clone();
            }
            if daily.timezone.is_none() {
                daily.timezone = raw.daily.timezone.clone();
            }
            if !daily.catch_up {
                daily.catch_up = raw.daily.catch_up;
            }
            daily.normalize(PeriodKind::Daily);
        }

        let daily = periodic
            .daily
            .as_ref()
            .map(DailyConfig::from_periodic)
            .unwrap_or_else(|| {
                if raw.daily.is_default() {
                    DailyConfig::default()
                } else {
                    raw.daily.clone()
                }
            });

        Ok(Self {
            schema_version: raw.schema_version,
            name: raw.name,
            homepage: raw.homepage,
            capture: raw.capture,
            daily,
            periodic,
            editor: raw.editor,
            appearance: raw.appearance,
            git: raw.git,
            hooks: raw.hooks,
            embed: raw.embed,
            clip: raw.clip,
        })
    }
}

impl VaultConfig {
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadError {
            path: path.to_path_buf(),
            source,
        })?;

        toml::from_str(&content).map_err(|error| ConfigError::ParseError {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    }

    pub fn load_from_vault(vault_root: &Path) -> Result<Self, ConfigError> {
        Self::load_from(&vault_root.join(".notesmith").join("vault.toml"))
    }

    pub fn save_to_vault(&self, vault_root: &Path) -> Result<(), ConfigError> {
        self.save_to(&vault_root.join(".notesmith").join("vault.toml"))
    }

    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            })?;
        }

        let periodic = self.periodic_for_persistence();
        let content = toml::to_string_pretty(&PersistedVaultConfig {
            schema_version: self.schema_version,
            name: &self.name,
            homepage: &self.homepage,
            capture: &self.capture,
            periodic: &periodic,
            editor: &self.editor,
            appearance: &self.appearance,
            git: &self.git,
            hooks: &self.hooks,
            embed: &self.embed,
            clip: &self.clip,
        })
        .map_err(|error| ConfigError::SerializeError {
            message: error.to_string(),
        })?;

        std::fs::write(path, content).map_err(|source| ConfigError::WriteError {
            path: path.to_path_buf(),
            source,
        })
    }

    fn periodic_for_persistence(&self) -> PeriodicConfig {
        let mut periodic = self.periodic.clone();
        if let Some(ref mut daily) = periodic.daily {
            if daily.folder.is_empty() && !self.daily.folder.is_empty() {
                daily.folder = self.daily.folder.clone();
            }
            if daily.template.is_none() && !self.daily.template.is_empty() {
                daily.template = Some(self.daily.template.clone());
            }
            if daily.filename.trim().is_empty() {
                daily.filename = self.daily.filename.clone();
            }
            if daily.generate_at.is_none() {
                daily.generate_at = self.daily.generate_at.clone();
            }
            if daily.timezone.is_none() {
                daily.timezone = self.daily.timezone.clone();
            }
            if !daily.catch_up {
                daily.catch_up = self.daily.catch_up;
            }
        } else if !self.daily.is_default() {
            periodic.daily = Some(PeriodKindConfig::from_daily_compat(&self.daily));
        }
        periodic.normalize();
        periodic
    }
}

fn normalize_folder(folder: &str) -> &str {
    folder.trim_end_matches('/')
}

fn split_parent_and_stem(path: &str) -> Option<(&str, &str)> {
    let (parent, file_name) = path.rsplit_once('/').unwrap_or(("", path));
    let stem = file_name.strip_suffix(".md")?;
    Some((parent, stem))
}

fn extract_key_from_filename_template(
    template: &str,
    kind: PeriodKind,
    stem: &str,
) -> Option<String> {
    let token = match kind {
        PeriodKind::Daily => "date",
        PeriodKind::Weekly => "week",
        PeriodKind::Monthly => "month",
        PeriodKind::Quarterly => "quarter",
        PeriodKind::Yearly => "year",
    };
    let (prefix, suffix) = split_template_around_token(template, token)?;
    if !stem.starts_with(&prefix) || !stem.ends_with(&suffix) {
        return None;
    }
    let key = &stem[prefix.len()..stem.len().saturating_sub(suffix.len())];
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn split_template_around_token(template: &str, token: &str) -> Option<(String, String)> {
    let mut cursor = 0;
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut found = false;

    while let Some(start) = template[cursor..].find("{{") {
        let start = cursor + start;
        let end = template[start + 2..].find("}}")? + start + 2;
        let variable = template[start + 2..end].trim();
        if found {
            return None;
        }
        if variable == token {
            prefix.push_str(&template[cursor..start]);
            suffix.push_str(&template[end + 2..]);
            found = true;
            break;
        }
        prefix.push_str(&template[cursor..start]);
        prefix.push_str(&template[start..=end + 1]);
        cursor = end + 2;
    }

    found.then_some((prefix, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_toml(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
    }

    fn sample_vault_toml() -> &'static str {
        r#"
name = "work"
homepage = "Dashboards/Home.md"

[capture]
folder = "Inbox"
template = "generic-note"

[daily]
folder = "Inbox/Daily"
template = "daily-note"
filename = "Daily {{ date }}"
generate_at = "06:30"
timezone = "UTC"
catch_up = true

[editor]
live_preview = true
default_mode = "source"
strict_line_breaks = false
show_line_numbers = true
hide_duplicate_h1 = false
paste_url_image_whitelist = "imgur\\.com"

[appearance]
theme = "light"

[git]
enabled = true
auto_commit_every = "15m"
commit_message = "notesmith: {{ operation }}"

[hooks]
on_note_create = "hooks/create.py"
"#
    }

    fn sample_vault_config() -> VaultConfig {
        let daily = DailyConfig {
            folder: "Inbox/Daily".to_string(),
            template: "daily-note".to_string(),
            filename: "Daily {{ date }}".to_string(),
            generate_at: Some("06:30".to_string()),
            timezone: Some("UTC".to_string()),
            catch_up: true,
        };
        VaultConfig {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: "work".to_string(),
            homepage: Some("Dashboards/Home.md".to_string()),
            capture: CaptureConfig {
                folder: "Inbox".to_string(),
                template: "generic-note".to_string(),
            },
            daily: daily.clone(),
            periodic: PeriodicConfig {
                daily: Some(PeriodKindConfig::from_daily_compat(&daily)),
                weekly: Some(PeriodKindConfig {
                    folder: "Inbox/Weekly".to_string(),
                    template: Some("weekly-note".to_string()),
                    filename: "Week {{ week }}".to_string(),
                    generate_at: None,
                    timezone: None,
                    catch_up: false,
                }),
                monthly: Some(PeriodKindConfig {
                    folder: "Inbox/Monthly".to_string(),
                    template: Some("monthly-note".to_string()),
                    filename: "Review {{ month }} Done".to_string(),
                    generate_at: None,
                    timezone: None,
                    catch_up: false,
                }),
                quarterly: None,
                yearly: None,
            },
            editor: EditorConfig {
                hide_duplicate_h1: false,
                paste_url_image_whitelist: "imgur\\.com".to_string(),
                ..EditorConfig::default()
            },
            appearance: AppearanceConfig {
                theme: "light".to_string(),
            },
            git: GitConfig {
                enabled: true,
                auto_commit_every: Some("15m".to_string()),
                commit_on_inactivity: None,
                auto_pull_every: None,
                auto_push_every: None,
                commit_message: Some("notesmith: {{ operation }}".to_string()),
            },
            hooks: HooksConfig {
                on_note_create: Some("hooks/create.py".to_string()),
                ..HooksConfig::default()
            },
            embed: EmbedConfig::default(),
            clip: ClipConfig::default(),
        }
    }

    #[test]
    fn load_from_reads_valid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("vault.toml");
        write_toml(&path, sample_vault_toml());

        let config = VaultConfig::load_from(&path).unwrap();

        assert_eq!(config.name, "work");
        assert_eq!(config.homepage.as_deref(), Some("Dashboards/Home.md"));
        assert_eq!(config.capture.folder, "Inbox");
        assert_eq!(config.daily.template, "daily-note");
        assert_eq!(config.daily.filename, "Daily {{ date }}");
        assert_eq!(config.daily.generate_at.as_deref(), Some("06:30"));
        assert_eq!(config.daily.timezone.as_deref(), Some("UTC"));
        assert!(config.daily.catch_up);
        assert_eq!(
            config
                .periodic
                .daily
                .as_ref()
                .and_then(|daily| daily.template.as_deref()),
            Some("daily-note")
        );
        assert_eq!(config.appearance.theme, "light");
        assert!(config.git.enabled);
        assert_eq!(
            config.hooks.on_note_create.as_deref(),
            Some("hooks/create.py")
        );
    }

    #[test]
    fn save_to_round_trips_through_disk() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nested").join("vault.toml");
        let expected = sample_vault_config();

        expected.save_to(&path).unwrap();
        let actual = VaultConfig::load_from(&path).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn load_from_vault_reads_notesmith_vault_file() {
        let temp_dir = TempDir::new().unwrap();
        let notesmith_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&notesmith_dir).unwrap();
        write_toml(&notesmith_dir.join("vault.toml"), sample_vault_toml());

        let config = VaultConfig::load_from_vault(temp_dir.path()).unwrap();

        assert_eq!(config.name, "work");
        assert_eq!(config.capture.template, "generic-note");
    }

    #[test]
    fn match_note_path_matches_daily_weekly_and_monthly_notes() {
        let mut config = PeriodicConfig::default();

        let mut daily = PeriodKindConfig::for_kind(PeriodKind::Daily);
        daily.folder = "Inbox/Daily/".to_string();
        daily.filename = "Daily {{ date }} Summary".to_string();
        config.daily = Some(daily);

        let mut weekly = PeriodKindConfig::for_kind(PeriodKind::Weekly);
        weekly.folder = "Inbox/Weekly".to_string();
        weekly.filename = "Week {{ week }}".to_string();
        config.weekly = Some(weekly);

        let mut monthly = PeriodKindConfig::for_kind(PeriodKind::Monthly);
        monthly.folder = "Inbox/Monthly".to_string();
        monthly.filename = "Review {{ month }} Done".to_string();
        config.monthly = Some(monthly);

        let daily_match = config
            .match_note_path("Inbox/Daily/Daily 2026-05-23 Summary.md")
            .unwrap();
        assert_eq!(daily_match.kind, PeriodKind::Daily);
        assert_eq!(daily_match.key, "2026-05-23");
        assert_eq!(
            (daily_match.period_start, daily_match.period_end),
            (
                NaiveDate::from_ymd_opt(2026, 5, 23).unwrap(),
                NaiveDate::from_ymd_opt(2026, 5, 23).unwrap()
            )
        );

        let weekly_match = config
            .match_note_path("Inbox/Weekly/Week 2026-W21.md")
            .unwrap();
        assert_eq!(weekly_match.kind, PeriodKind::Weekly);
        assert_eq!(weekly_match.key, "2026-W21");

        let monthly_match = config
            .match_note_path("Inbox/Monthly/Review 2026-05 Done.md")
            .unwrap();
        assert_eq!(monthly_match.kind, PeriodKind::Monthly);
        assert_eq!(monthly_match.key, "2026-05");
    }

    #[test]
    fn match_note_path_handles_root_paths_and_requires_markdown_extension() {
        let mut config = PeriodicConfig::default();
        config.daily = Some(PeriodKindConfig::for_kind(PeriodKind::Daily));

        let matched = config.match_note_path("2026-05-23.md").unwrap();
        assert_eq!(matched.kind, PeriodKind::Daily);
        assert_eq!(matched.key, "2026-05-23");

        assert!(config.match_note_path("2026-05-23").is_none());
        assert!(config.match_note_path("Inbox/Daily/2026-05-23").is_none());
        assert!(config.match_note_path("2026-05-23.txt").is_none());
    }

    #[test]
    fn normalize_sets_default_filename_and_daily_template() {
        let mut monthly = PeriodKindConfig {
            folder: "Inbox/Monthly".to_string(),
            template: Some("monthly-note".to_string()),
            filename: "  ".to_string(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        };
        monthly.normalize(PeriodKind::Monthly);
        assert_eq!(monthly.filename, "{{ month }}");

        let mut daily = PeriodKindConfig {
            folder: "Inbox/Daily".to_string(),
            template: None,
            filename: String::new(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        };
        daily.normalize(PeriodKind::Daily);
        assert_eq!(daily.filename, "{{ date }}");
        assert_eq!(daily.template.as_deref(), Some("daily-note"));
    }

    #[test]
    fn extract_period_key_uses_template_prefix_and_suffix() {
        let daily = PeriodKindConfig {
            folder: "Inbox/Daily".to_string(),
            template: Some("daily-note".to_string()),
            filename: "Daily ({{ date }}) done".to_string(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        };
        assert_eq!(
            daily.extract_period_key(PeriodKind::Daily, "Daily (2026-05-23) done"),
            Some("2026-05-23".to_string())
        );
        assert_eq!(
            daily.extract_period_key(PeriodKind::Daily, "2026-05-23"),
            Some("2026-05-23".to_string())
        );
        assert_eq!(
            daily.extract_period_key(PeriodKind::Daily, "Daily 2026-05-23 done"),
            None
        );

        let weekly = PeriodKindConfig {
            folder: "Inbox/Weekly".to_string(),
            template: Some("weekly-note".to_string()),
            filename: "Week {{ week }} Review".to_string(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        };
        assert_eq!(
            weekly.extract_period_key(PeriodKind::Weekly, "Week 2026-W21 Review"),
            Some("2026-W21".to_string())
        );
    }

    #[test]
    fn load_from_merges_legacy_daily_and_periodic_daily_sections() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("vault.toml");
        write_toml(
            &path,
            r#"
name = "work"

[daily]
folder = "Legacy/Daily"
template = "legacy-daily"
filename = "Legacy {{ date }}"
generate_at = "06:15"
timezone = "UTC"
catch_up = true

[periodic.daily]
folder = "Periodic/Daily"
filename = ""
"#,
        );

        let config = VaultConfig::load_from(&path).unwrap();
        let merged = config.periodic.daily.as_ref().unwrap();

        assert_eq!(merged.folder, "Periodic/Daily");
        assert_eq!(merged.template.as_deref(), Some("daily-note"));
        assert_eq!(merged.filename, "{{ date }}");
        assert_eq!(merged.generate_at.as_deref(), Some("06:15"));
        assert_eq!(merged.timezone.as_deref(), Some("UTC"));
        assert!(merged.catch_up);

        assert_eq!(config.daily.folder, "Periodic/Daily");
        assert_eq!(config.daily.template, "daily-note");
        assert_eq!(config.daily.filename, "{{ date }}");
        assert_eq!(config.daily.generate_at.as_deref(), Some("06:15"));
        assert_eq!(config.daily.timezone.as_deref(), Some("UTC"));
        assert!(config.daily.catch_up);
    }

    #[test]
    fn periodic_config_is_empty_only_when_all_kinds_are_none() {
        assert!(periodic_config_is_empty(&PeriodicConfig::default()));

        let mut config = PeriodicConfig::default();
        config.yearly = Some(PeriodKindConfig::for_kind(PeriodKind::Yearly));

        assert!(!periodic_config_is_empty(&config));
    }

    #[test]
    fn embed_is_disabled_by_default() {
        let config = VaultConfig::default();
        assert!(!config.embed.enabled);
    }

    #[test]
    fn embed_defaults_to_disabled_when_table_absent() {
        let toml = r#"
            name = "no-embed-table"
        "#;
        let config: VaultConfig = toml::from_str(toml).unwrap();
        assert!(!config.embed.enabled);
    }

    #[test]
    fn embed_enabled_parses_from_table() {
        let toml = r#"
            name = "with-embed"

            [embed]
            enabled = true
        "#;
        let config: VaultConfig = toml::from_str(toml).unwrap();
        assert!(config.embed.enabled);
    }

    #[test]
    fn embed_enabled_round_trips_through_disk() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("vault.toml");
        let mut config = VaultConfig::default();
        config.name = "round-trip".to_string();
        config.embed.enabled = true;

        config.save_to(&path).unwrap();
        let loaded = VaultConfig::load_from(&path).unwrap();
        assert!(loaded.embed.enabled);
    }
}
