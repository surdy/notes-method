use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_ROWS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryRequest {
    pub sql: String,
    #[serde(default)]
    pub max_rows: Option<usize>,
    #[serde(default)]
    pub format: QueryFormat,
}

impl QueryRequest {
    pub fn max_rows_or_default(&self) -> usize {
        self.max_rows.unwrap_or(DEFAULT_MAX_ROWS)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QueryFormat {
    #[default]
    Json,
    Markdown,
}
