//! Relevant Notes endpoint (issue #201).
//!
//! `GET /api/v/{vault}/related/{*path}` returns notes related to the given note,
//! ranked by [`notesmith_ops::LocalOps::related_notes`] (embedding similarity
//! blended with link-graph proximity, degrading to graph-only when the vault
//! has no usable embeddings). It backs the Relevant Notes section of the
//! desktop right dock, which refreshes on active-note change.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::{SharedAppState, local_ops_for};

/// Default number of related notes returned when the client omits `limit`.
const DEFAULT_LIMIT: usize = 10;
/// Upper bound so a client can't request an unbounded scan-and-rank.
const MAX_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
pub struct RelatedQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn related_notes(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Query(params): Query<RelatedQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let ops = local_ops_for(&vault_name, vault);

    ops.related_notes(&note_path, limit)
        .map(Json)
        .map_err(|error| {
            let message = error.to_string();
            let status = if message.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": message })))
        })
}
