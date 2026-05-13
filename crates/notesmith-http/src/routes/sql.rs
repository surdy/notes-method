use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_query::{QueryResult, execute_sql};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;

use super::helpers::query_error;

#[derive(Debug, Deserialize)]
pub struct SqlQueryRequest {
    pub sql: String,
}

pub async fn execute_sql_query(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<SqlQueryRequest>,
) -> Result<Json<QueryResult>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    execute_sql(&vault.cache, &request.sql)
        .map(Json)
        .map_err(query_error)
}

pub(crate) fn format_query_as_markdown_table(result: &QueryResult) -> String {
    if result.row_count == 0 || result.rows.is_empty() {
        return "(no results)".to_string();
    }

    let header = format!("| {} |", result.columns.join(" | "));
    let separator = format!(
        "| {} |",
        result
            .columns
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let rows = result
        .rows
        .iter()
        .map(|row| {
            format!(
                "| {} |",
                row.iter()
                    .map(format_markdown_cell)
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("{header}\n{separator}\n{rows}")
}

fn format_markdown_cell(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    text.replace(['\n', '\r'], " ").replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_query_as_markdown_table_basic() {
        let result = QueryResult {
            columns: vec!["text".to_string(), "due".to_string()],
            rows: vec![vec![json!("Follow up"), json!("2026-05-10")]],
            row_count: 1,
        };

        assert_eq!(
            format_query_as_markdown_table(&result),
            "| text | due |\n| --- | --- |\n| Follow up | 2026-05-10 |"
        );
    }

    #[test]
    fn format_query_as_markdown_table_empty() {
        let result = QueryResult {
            columns: vec!["text".to_string()],
            rows: vec![],
            row_count: 0,
        };

        assert_eq!(format_query_as_markdown_table(&result), "(no results)");
    }
}
