use serde::Serialize;

pub const DEFAULT_MEMORY_RECALL_LIMIT: usize = 20;
pub const MAX_MEMORY_RECALL_LIMIT: usize = 100;
pub const DEFAULT_MEMORY_LIST_LIMIT: usize = 50;
pub const MAX_MEMORY_LIST_LIMIT: usize = 100;
pub const DEFAULT_MEMORY_REVIEW_LIMIT: usize = 10;
pub const MAX_MEMORY_REVIEW_LIMIT: usize = 25;

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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryListFact {
    pub path: String,
    pub hash: String,
    pub title: String,
    pub claim: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub subject: Option<String>,
    pub certainty: Option<String>,
    pub source: Option<String>,
    pub status: String,
    pub confirmed: Option<String>,
    pub supersedes: Option<String>,
    pub tags: Vec<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryListResponse {
    pub scope: Option<String>,
    pub status: String,
    pub limit: usize,
    pub match_count: usize,
    pub facts: Vec<MemoryListFact>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryReviewCandidate {
    pub path: String,
    pub hash: String,
    pub title: String,
    pub claim: String,
    pub scope: Option<String>,
    pub certainty: Option<String>,
    pub source: Option<String>,
    pub status: String,
    pub score: f32,
    pub rank: usize,
    pub lexical_rank: Option<usize>,
    pub semantic_rank: Option<usize>,
    pub exact_duplicate: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryMutationPreview {
    pub operation: String,
    pub path: String,
    pub content: String,
    pub hash: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryMutationPlan {
    pub applied: bool,
    pub confirmation_required: bool,
    pub preview_token: String,
    pub proposed: MemoryMutationPreview,
    pub candidates: Vec<MemoryReviewCandidate>,
}
