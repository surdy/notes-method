use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;

use super::helpers::internal_error;

/// Default number of commits returned by `git/log` when no limit is given.
const DEFAULT_LOG_LIMIT: usize = 50;
/// Upper bound to keep history payloads bounded.
const MAX_LOG_LIMIT: usize = 500;

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

#[derive(Debug, Default, Deserialize)]
pub struct GitLogQuery {
    /// Maximum number of commits to return (clamped to `MAX_LOG_LIMIT`).
    pub limit: Option<usize>,
}

/// Rich commit history with per-commit diff stats, for the git-history UI.
pub async fn git_log(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Query(query): Query<GitLogQuery>,
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

    let limit = query
        .limit
        .unwrap_or(DEFAULT_LOG_LIMIT)
        .clamp(1, MAX_LOG_LIMIT);
    let root = vault.root.clone();
    drop(state);

    let entries = notesmith_git::ops::history(&root, limit).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(entries).map_err(internal_error)?))
}

/// The full file-level diff of a single commit, for the git-history UI.
pub async fn git_diff(
    State(state): State<SharedAppState>,
    Path((vault_name, sha)): Path<(String, String)>,
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

    let diff = notesmith_git::ops::commit_diff(&root, &sha).map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("unknown commit: {sha}") })),
        )
    })?;
    Ok(Json(serde_json::to_value(diff).map_err(internal_error)?))
}

#[derive(Debug, Default, Deserialize)]
pub struct GitCommitRequest {
    /// Optional explicit commit message. When omitted, a message is generated
    /// from the changed-file list.
    #[serde(default)]
    pub message: Option<String>,
}

/// Stage and commit the working tree (a "checkpoint"). Used by the desktop
/// inactivity-checkpoint driver after flushing unsaved editor buffers to disk,
/// and available for manual "commit now" actions. Requires `git.enabled`.
///
/// When no `message` is provided in the body, the vault's configured
/// `commit_message` is used; if that is also unset, a message is generated from
/// the changed-file list.
pub async fn git_commit(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    body: Option<Json<GitCommitRequest>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let vault_config = vault.vault_config.load();
    if !vault_config.git.enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "git integration is not enabled for this vault" })),
        ));
    }

    if !notesmith_git::ops::is_git_repo(&vault.root) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "vault is not a git repository" })),
        ));
    }

    // Body message takes precedence; else fall back to the configured template.
    let requested = body.and_then(|Json(req)| req.message);
    let message = requested.or_else(|| vault_config.git.commit_message.clone());
    let root = vault.root.clone();
    drop(state);

    let outcome =
        notesmith_git::ops::commit_all(&root, message.as_deref()).map_err(internal_error)?;

    match outcome {
        Some(commit) => Ok(Json(json!({
            "committed": true,
            "sha": commit.sha,
            "files": commit.files,
        }))),
        None => Ok(Json(json!({
            "committed": false,
            "sha": Value::Null,
            "files": [],
        }))),
    }
}

/// Initialize a git repository for the vault if one does not already exist
/// (idempotent). Scaffolds a minimal `.gitignore` and makes an initial commit
/// of existing content. Called automatically when git is enabled via config,
/// and available as an explicit action.
pub async fn git_init(
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
    let root = vault.root.clone();
    drop(state);

    let result = notesmith_git::ops::init_repo(&root).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(result).map_err(internal_error)?))
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
