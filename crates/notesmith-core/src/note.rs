use crate::frontmatter::Frontmatter;
use crate::link::{Block, InlineField, Link};
use crate::task::Task;
use crate::types::{VaultName, VaultPath};
use serde::{Deserialize, Serialize};

/// The canonical parsed Note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub vault: VaultName,
    pub path: VaultPath,
    pub frontmatter: Option<Frontmatter>,
    pub raw_frontmatter: Option<String>,
    pub body: String,
    pub tasks: Vec<Task>,
    pub links: Vec<Link>,
    pub inline_fields: Vec<InlineField>,
    pub blocks: Vec<Block>,
    pub hash: String,
}
