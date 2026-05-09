//! notesmith-vault: VaultEngine trait and native filesystem adapters

pub mod engine;
pub mod parser;

pub use engine::NativeVaultEngine;
pub use parser::{ParsedNote, parse_note};
