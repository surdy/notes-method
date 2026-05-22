use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use crate::server::SharedAppState;

use super::helpers::internal_error;

pub async fn git_status(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    if !notesmith_git::ops::is_git_repo(&vault.root) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "vault is not a git repository" })),
        ));
    }

    let status = notesmith_git::ops::status(&vault.root).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(status).map_err(internal_error)?))
}

pub async fn git_sync(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    if !notesmith_git::ops::is_git_repo(&vault.root) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "vault is not a git repository" })),
        ));
    }

    let root = vault.root.clone();
    drop(state);

    let pull_result = notesmith_git::ops::pull_ff(&root, "origin").map_err(internal_error)?;
    if pull_result.conflict {
        return Ok(Json(json!({
            "pull": pull_result,
            "push": null,
            "error": "pull conflict, push skipped",
        })));
    }

    let push_result = notesmith_git::ops::push(&root, "origin").map_err(internal_error)?;
    Ok(Json(json!({
        "pull": pull_result,
        "push": push_result,
    })))
}
