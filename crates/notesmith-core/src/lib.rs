//! notesmith-core: Core data model, parser traits, and OFM extensions for Notesmith

pub mod error;
pub mod frontmatter;
pub mod link;
pub mod note;
pub mod task;
pub mod traits;
pub mod types;

// Re-exports for convenience
pub use error::NotesmithError;
pub use frontmatter::Frontmatter;
pub use link::{Block, InlineField, Link, LinkType, SourcePosition};
pub use note::Note;
pub use task::{Task, TaskPriority, TaskStatus};
pub use traits::{VaultEngine, WriteResult};
pub use types::{VaultName, VaultPath};
