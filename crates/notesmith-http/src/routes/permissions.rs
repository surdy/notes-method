//! HTTP endpoints for per-vault persisted agent write grants (issue #189).
//!
//! These expose the daemon-owned
//! [`PermissionGrantStore`](notesmith_permission::PermissionGrantStore) so the
//! desktop chat UI can fetch a vault's "Always Allow" grants at session start
//! (to pre-seed the ACP session and never re-prompt) and persist a new grant
//! when the user picks "Always Allow". Persistence is frontend-orchestrated:
//! the Rust agent/ACP layer stays HTTP-free and only consumes the seed list.
//!
//! All routes are vault-scoped (`/api/v/{vault}/agent/permissions`); the store
//! keys every operation by vault, so one vault's grants are never visible under
//! another. Bodies are validated and malformed input yields a structured 4xx
//! (never a 500), matching `routes::transcripts`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_permission::PermissionError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;

type ApiError = (StatusCode, Json<Value>);

fn error(status: StatusCode, message: impl std::fmt::Display) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

fn store_error(err: PermissionError) -> ApiError {
    tracing::error!(error = %err, "permission store error");
    error(StatusCode::INTERNAL_SERVER_ERROR, "permission store error")
}

/// Clone the shared permission store out of `AppState`, releasing the app lock
/// before doing (synchronous) database work.
async fn permissions(
    state: &SharedAppState,
) -> std::sync::Arc<notesmith_permission::PermissionGrantStore> {
    state.read().await.permissions.clone()
}

#[derive(Debug, Deserialize)]
pub struct GrantRequest {
    pub tool: String,
}

/// `GET /api/v/{vault}/agent/permissions` — list a vault's persisted grants.
pub async fn list_grants(
    State(state): State<SharedAppState>,
    Path(vault): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let store = permissions(&state).await;
    let granted = store.list_granted(&vault).map_err(store_error)?;
    Ok(Json(granted))
}

/// `POST /api/v/{vault}/agent/permissions` — persist an "Always Allow" grant.
pub async fn grant_permission(
    State(state): State<SharedAppState>,
    Path(vault): Path<String>,
    Json(req): Json<GrantRequest>,
) -> Result<StatusCode, ApiError> {
    if req.tool.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "tool must not be empty"));
    }
    let store = permissions(&state).await;
    store.grant(&vault, &req.tool).map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v/{vault}/agent/permissions/{tool}` — revoke a persisted grant.
pub async fn revoke_permission(
    State(state): State<SharedAppState>,
    Path((vault, tool)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let store = permissions(&state).await;
    store.revoke(&vault, &tool).map_err(store_error)?;
    // Revoking an absent grant is idempotent; 204 either way.
    Ok(StatusCode::NO_CONTENT)
}
