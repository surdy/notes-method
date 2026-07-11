//! notesmith-config: Global and per-vault configuration loading and validation

pub mod detection;
pub mod error;
pub mod global;
pub mod lockfile;
pub mod migration;
pub mod vault;

pub use detection::{
    DetectedVault, DetectionSource, VAULT_CONFIG_DIR, VAULT_CONFIG_FILE, detect_vault,
    walk_up_for_vault,
};
pub use error::ConfigError;
pub use global::{
    AgentEntry, AgentsConfig, CompanionMemoryConfig, DaemonConfig, GlobalConfig, McpConfig,
    McpServerEntry, VaultRegistration, expand_path_vars,
};
pub use lockfile::DaemonLockfile;
pub use vault::{
    AppearanceConfig, CURRENT_SCHEMA_VERSION, CaptureConfig, ClipConfig, ClipTemplate, DailyConfig,
    EditorConfig, EmbedConfig, GitConfig, HooksConfig, PeriodKindConfig, PeriodicConfig,
    PeriodicNoteMatch, VaultConfig,
};
