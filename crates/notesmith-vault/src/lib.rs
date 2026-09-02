//! notesmith-vault: VaultEngine trait and native filesystem adapters

pub mod engine;
mod frontmatter;
pub mod managed_section;
pub mod parser;
pub mod save_pipeline;
pub mod wikilink_rewrite;

pub use engine::NativeVaultEngine;
pub use frontmatter::extract_frontmatter;
pub use managed_section::{
    ManagedSectionError, ManagedSectionUpdate, begin_marker, end_marker, update_managed_section,
};
pub use parser::parse_note;
pub use save_pipeline::{
    apply_save_pipeline, apply_save_pipeline_with_timestamp, parse_frontmatter_mapping,
    serialize_frontmatter, sort_mapping,
};
pub use wikilink_rewrite::{WikilinkRewriteResult, rewrite_wikilinks};
