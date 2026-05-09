//! notesmith-config: Global and per-vault configuration loading and validation

pub mod detection;
pub mod error;
pub mod global;
pub mod vault;

pub use detection::{
    DetectedVault, DetectionSource, VAULT_CONFIG_DIR, VAULT_CONFIG_FILE, detect_vault,
    walk_up_for_vault,
};
pub use error::ConfigError;
pub use global::{DaemonConfig, GlobalConfig, VaultRegistration};
pub use vault::{DailyConfig, EditorConfig, GitConfig, HooksConfig, InboxConfig, VaultConfig};
