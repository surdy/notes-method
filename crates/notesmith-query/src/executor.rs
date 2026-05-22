use notesmith_index::VaultCache;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: usize,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Query execution failed: {0}")]
    ExecutionError(String),
    #[error("Only SELECT statements are allowed")]
    NotReadOnly,
}

pub fn execute_sql(cache: &VaultCache, sql: &str) -> Result<QueryResult, QueryError> {
    execute_sql_with_options(cache, sql, None)
}

pub fn execute_sql_with_options(
    cache: &VaultCache,
    sql: &str,
    max_rows: Option<usize>,
) -> Result<QueryResult, QueryError> {
    let trimmed = sql.trim().to_uppercase();
    if !trimmed.starts_with("SELECT") && !trimmed.starts_with("WITH") {
        return Err(QueryError::NotReadOnly);
    }

    let conn = cache.connection();
    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| QueryError::ExecutionError(err.to_string()))?;

    let columns = stmt
        .column_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let column_count = columns.len();

    let mut rows = Vec::new();
    let mut truncated = false;
    let mapped_rows = stmt
        .query_map([], |row| {
            let mut values = Vec::with_capacity(column_count);
            for index in 0..column_count {
                let value: rusqlite::types::Value = row.get(index)?;
                let json_value = match value {
                    rusqlite::types::Value::Null => JsonValue::Null,
                    rusqlite::types::Value::Integer(number) => JsonValue::Number(number.into()),
                    rusqlite::types::Value::Real(number) => serde_json::Number::from_f64(number)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::Null),
                    rusqlite::types::Value::Text(text) => JsonValue::String(text),
                    rusqlite::types::Value::Blob(bytes) => {
                        JsonValue::String(format!("<blob {} bytes>", bytes.len()))
                    }
                };
                values.push(json_value);
            }
            Ok(values)
        })
        .map_err(|err| QueryError::ExecutionError(err.to_string()))?;

    for row in mapped_rows {
        let row = row.map_err(|err| QueryError::ExecutionError(err.to_string()))?;
        if max_rows.is_some_and(|limit| rows.len() >= limit) {
            truncated = true;
            break;
        }
        rows.push(row);
    }

    let row_count = rows.len();
    Ok(QueryResult {
        columns,
        rows,
        row_count,
        truncated,
    })
}
