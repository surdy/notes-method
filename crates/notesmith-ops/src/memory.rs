use serde::Serialize;

pub const DEFAULT_MEMORY_RECALL_LIMIT: usize = 20;
pub const MAX_MEMORY_RECALL_LIMIT: usize = 100;

#[derive(Debug, Clone)]
pub struct FactNoteMeta {
    pub path: String,
    pub title: String,
    pub claim: String,
    pub scope: Option<String>,
    pub certainty: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryRecallHit {
    pub path: String,
    pub title: String,
    pub claim: String,
    pub scope: Option<String>,
    pub certainty: Option<String>,
    pub source: Option<String>,
    pub snippet: String,
    pub score: f32,
    pub rank: usize,
    pub lexical_rank: Option<usize>,
    pub semantic_rank: Option<usize>,
    pub char_start: Option<i64>,
    pub char_end: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryRecallResponse {
    pub query: String,
    pub scope: Option<String>,
    pub limit: usize,
    pub match_count: usize,
    pub embeddings_used: bool,
    pub facts: Vec<MemoryRecallHit>,
}
