use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current schema version for vault-local `vault.toml` files.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub daily: DailyConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            name: String::new(),
            homepage: None,
            capture: Default::default(),
            daily: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
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

impl Default for DailyConfig {
    fn default() -> Self {
        Self {
            folder: default_daily_folder(),
            template: default_daily_template(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        }
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
    pub on_daily_create: Option<String>,
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

        let content =
            toml::to_string_pretty(self).map_err(|error| ConfigError::SerializeError {
                message: error.to_string(),
            })?;

        std::fs::write(path, content).map_err(|source| ConfigError::WriteError {
            path: path.to_path_buf(),
            source,
        })
    }
}
