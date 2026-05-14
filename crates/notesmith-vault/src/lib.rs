//! notesmith-vault: VaultEngine trait and native filesystem adapters

pub mod engine;
mod frontmatter;
pub mod parser;
pub mod save_pipeline;

pub use engine::NativeVaultEngine;
pub use frontmatter::extract_frontmatter;
pub use parser::parse_note;
pub use save_pipeline::{
    apply_save_pipeline, apply_save_pipeline_with_timestamp, parse_frontmatter_mapping,
    serialize_frontmatter, sort_mapping,
};
