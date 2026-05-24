//! notesmith-core: Core data model, parser traits, and OFM extensions for Notesmith

pub mod error;
pub mod frontmatter;
pub mod link;
pub mod note;
pub mod periodic;
pub mod task;
pub mod traits;
pub mod types;
pub mod url_actions;
pub mod url_scheme;

// Re-exports for convenience
pub use error::NotesmithError;
pub use frontmatter::Frontmatter;
pub use link::{Block, InlineField, Link, LinkType, SourcePosition};
pub use note::Note;
pub use periodic::PeriodKind;
pub use task::{StatusGroup, Task, TaskStatusConfig, TaskStatusMap};
pub use traits::{VaultEngine, WriteResult};
pub use types::{VaultName, VaultPath};
pub use url_actions::{ActionStep, UrlAction, UrlActionsFile};
pub use url_scheme::{NotesmithUrl, UrlParseError, parse_notesmith_url};
