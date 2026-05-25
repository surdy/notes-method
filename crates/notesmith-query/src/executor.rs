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

#[cfg(test)]
mod tests {
    use notesmith_index::VaultCache;
    use serde_json::json;

    use super::{QueryError, execute_sql, execute_sql_with_options};

    #[test]
    fn rejects_delete_statements() {
        assert_read_only_rejected("DELETE FROM notes");
    }

    #[test]
    fn rejects_insert_statements() {
        assert_read_only_rejected("INSERT INTO notes VALUES (1)");
    }

    #[test]
    fn rejects_update_statements() {
        assert_read_only_rejected("UPDATE notes SET title = 'x'");
    }

    #[test]
    fn rejects_drop_table_statements() {
        assert_read_only_rejected("DROP TABLE notes");
    }

    #[test]
    fn rejects_create_table_statements() {
        assert_read_only_rejected("CREATE TABLE notes (id INTEGER)");
    }

    #[test]
    fn rejects_alter_table_statements() {
        assert_read_only_rejected("ALTER TABLE notes ADD COLUMN title TEXT");
    }

    #[test]
    fn accepts_select_statements() {
        let cache = VaultCache::open_in_memory().unwrap();
        let result = execute_sql(&cache, "SELECT 1 AS value").unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.rows, vec![vec![json!(1)]]);
        assert_eq!(result.row_count, 1);
        assert!(!result.truncated);
    }

    #[test]
    fn accepts_with_cte_statements() {
        let cache = VaultCache::open_in_memory().unwrap();
        let result = execute_sql(
            &cache,
            "WITH cte AS (SELECT 1 AS value) SELECT value FROM cte",
        )
        .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.rows, vec![vec![json!(1)]]);
    }

    #[test]
    fn accepts_select_with_leading_whitespace() {
        let cache = VaultCache::open_in_memory().unwrap();
        let result = execute_sql(&cache, "  SELECT 1 AS value").unwrap();

        assert_eq!(result.rows, vec![vec![json!(1)]]);
    }

    #[test]
    fn accepts_lowercase_select_statements() {
        let cache = VaultCache::open_in_memory().unwrap();
        let result = execute_sql(&cache, "select 1 as value").unwrap();

        assert_eq!(result.rows, vec![vec![json!(1)]]);
    }

    #[test]
    fn max_rows_marks_results_as_truncated() {
        let cache = VaultCache::open_in_memory().unwrap();
        {
            let conn = cache.connection();
            conn.execute_batch(
                "CREATE TEMP TABLE sample (value INTEGER);
                 INSERT INTO sample (value) VALUES (1), (2), (3);",
            )
            .unwrap();
        }

        let result =
            execute_sql_with_options(&cache, "SELECT value FROM sample ORDER BY value", Some(2))
                .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.rows, vec![vec![json!(1)], vec![json!(2)]]);
        assert_eq!(result.row_count, 2);
        assert!(result.truncated);
    }

    fn assert_read_only_rejected(sql: &str) {
        let cache = VaultCache::open_in_memory().unwrap();
        let error = execute_sql(&cache, sql).unwrap_err();
        assert!(matches!(error, QueryError::NotReadOnly));
    }
}
