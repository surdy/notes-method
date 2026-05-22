use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use notesmith_query::{
    QueryFormat, QueryRequest, execute_sql_with_options, format_query_as_markdown_table,
};
use serde_json::{Value, json};

use crate::server::SharedAppState;

use super::helpers::query_error;

pub async fn execute_sql_query(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<QueryRequest>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let result = execute_sql_with_options(
        &vault.cache,
        &request.sql,
        Some(request.max_rows_or_default()),
    )
    .map_err(query_error)?;

    match request.format {
        QueryFormat::Json => Ok(Json(result).into_response()),
        QueryFormat::Markdown => Ok((
            StatusCode::OK,
            [("content-type", "text/markdown; charset=utf-8")],
            format_query_as_markdown_table(&result),
        )
            .into_response()),
    }
}
