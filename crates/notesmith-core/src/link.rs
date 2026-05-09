use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkType {
    WikiLink,
    Embed,
    HeadingRef,
    BlockRef,
    Anchor,
    MarkdownLink,
    ExternalLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub link_type: LinkType,
    pub target: String,
    pub display_text: Option<String>,
    pub position: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineField {
    pub key: String,
    pub value: String,
    pub position: SourcePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub length: usize,
}

impl SourcePosition {
    pub fn new(line: usize, column: usize, offset: usize, length: usize) -> Self {
        Self {
            line,
            column,
            offset,
            length,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub content: String,
    pub block_id: Option<String>,
    pub position: SourcePosition,
}
