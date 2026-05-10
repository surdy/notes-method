//! notesmith-vault: VaultEngine trait and native filesystem adapters

pub mod engine;
pub mod parser;
pub mod save_pipeline;

pub use engine::NativeVaultEngine;
pub use parser::{ParsedNote, parse_note};
pub use save_pipeline::{
    apply_save_pipeline, apply_save_pipeline_with_timestamp, extract_frontmatter,
    parse_frontmatter_mapping, serialize_frontmatter, sort_mapping,
};
