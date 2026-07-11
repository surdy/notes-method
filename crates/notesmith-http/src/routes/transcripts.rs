//! HTTP endpoints for per-vault agent chat transcripts.
//!
//! These expose the daemon-owned [`TranscriptStore`](notesmith_transcript::TranscriptStore)
//! (ADR 0012 Decision 13) so the desktop chat UI can list, reopen, and persist
//! conversations over the same HTTP surface used for everything else — which is
//! what makes remote vaults work (the store lives wherever the daemon runs).
//!
//! All routes are vault-scoped (`/api/v/{vault}/agent/...`); the store keys
//! every operation by vault, so one vault's history is never visible under
//! another. Bodies are validated and malformed input yields a structured 4xx
//! (never a 500), matching `routes::routing::preview`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_transcript::{Message, Role, Thread, TranscriptError};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;

type ApiError = (StatusCode, Json<Value>);

fn error(status: StatusCode, message: impl std::fmt::Display) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

fn store_error(err: TranscriptError) -> ApiError {
    match err {
        TranscriptError::ThreadNotFound { .. } => error(StatusCode::NOT_FOUND, err),
        other => {
            tracing::error!(error = %other, "transcript store error");
            error(StatusCode::INTERNAL_SERVER_ERROR, "transcript store error")
        }
    }
}

/// Clone the shared transcript store out of `AppState`, releasing the app lock
/// before doing (synchronous) database work.
async fn transcripts(
    state: &SharedAppState,
) -> std::sync::Arc<notesmith_transcript::TranscriptStore> {
    state.read().await.transcripts.clone()
}

#[derive(Debug, Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RenameThreadRequest {
    pub title: String,
}

/// Body for binding (or clearing) a thread's ACP `sessionId` (issue #262).
#[derive(Debug, Deserialize)]
pub struct SetThreadSessionRequest {
    /// The agent's ACP `sessionId` to persist, or `null` to clear the binding.
    #[serde(default)]
    pub acp_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppendMessageRequest {
    pub role: Role,
    pub content: String,
}

/// `GET /api/v/{vault}/agent/threads` — list a vault's threads, most recent first.
pub async fn list_threads(
    State(state): State<SharedAppState>,
    Path(vault): Path<String>,
) -> Result<Json<Vec<Thread>>, ApiError> {
    let store = transcripts(&state).await;
    let threads = store.list_threads(&vault).map_err(store_error)?;
    Ok(Json(threads))
}

/// `POST /api/v/{vault}/agent/threads` — create a new thread.
pub async fn create_thread(
    State(state): State<SharedAppState>,
    Path(vault): Path<String>,
    Json(req): Json<CreateThreadRequest>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    if req.title.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "title must not be empty"));
    }
    let store = transcripts(&state).await;
    let thread = store
        .create_thread(
            &vault,
            &req.title,
            req.agent.as_deref(),
            req.model.as_deref(),
        )
        .map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(thread)))
}

/// `GET /api/v/{vault}/agent/threads/{thread_id}` — fetch one thread.
pub async fn get_thread(
    State(state): State<SharedAppState>,
    Path((vault, thread_id)): Path<(String, String)>,
) -> Result<Json<Thread>, ApiError> {
    let store = transcripts(&state).await;
    match store.get_thread(&vault, &thread_id).map_err(store_error)? {
        Some(thread) => Ok(Json(thread)),
        None => Err(error(StatusCode::NOT_FOUND, "thread not found")),
    }
}

/// `POST /api/v/{vault}/agent/threads/{thread_id}/rename` — rename a thread.
pub async fn rename_thread(
    State(state): State<SharedAppState>,
    Path((vault, thread_id)): Path<(String, String)>,
    Json(req): Json<RenameThreadRequest>,
) -> Result<Json<Thread>, ApiError> {
    if req.title.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "title must not be empty"));
    }
    let store = transcripts(&state).await;
    let renamed = store
        .rename_thread(&vault, &thread_id, &req.title)
        .map_err(store_error)?;
    if !renamed {
        return Err(error(StatusCode::NOT_FOUND, "thread not found"));
    }
    match store.get_thread(&vault, &thread_id).map_err(store_error)? {
        Some(thread) => Ok(Json(thread)),
        None => Err(error(StatusCode::NOT_FOUND, "thread not found")),
    }
}

/// `POST /api/v/{vault}/agent/threads/{thread_id}/session` — bind (or clear,
/// with a `null` body value) the thread's ACP `sessionId` for resume (#262).
pub async fn set_thread_session(
    State(state): State<SharedAppState>,
    Path((vault, thread_id)): Path<(String, String)>,
    Json(req): Json<SetThreadSessionRequest>,
) -> Result<Json<Thread>, ApiError> {
    let store = transcripts(&state).await;
    let updated = store
        .set_acp_session_id(&vault, &thread_id, req.acp_session_id.as_deref())
        .map_err(store_error)?;
    if !updated {
        return Err(error(StatusCode::NOT_FOUND, "thread not found"));
    }
    match store.get_thread(&vault, &thread_id).map_err(store_error)? {
        Some(thread) => Ok(Json(thread)),
        None => Err(error(StatusCode::NOT_FOUND, "thread not found")),
    }
}

/// `DELETE /api/v/{vault}/agent/threads/{thread_id}` — delete a thread.
pub async fn delete_thread(
    State(state): State<SharedAppState>,
    Path((vault, thread_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let store = transcripts(&state).await;
    let deleted = store
        .delete_thread(&vault, &thread_id)
        .map_err(store_error)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(StatusCode::NOT_FOUND, "thread not found"))
    }
}

/// `GET /api/v/{vault}/agent/threads/{thread_id}/messages` — load a thread's messages.
pub async fn list_messages(
    State(state): State<SharedAppState>,
    Path((vault, thread_id)): Path<(String, String)>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let store = transcripts(&state).await;
    // Scoped to the vault; a thread that does not belong to it yields `[]`. We
    // still surface a 404 when the thread does not exist at all in the vault so
    // the UI can distinguish "empty conversation" from "gone".
    if store
        .get_thread(&vault, &thread_id)
        .map_err(store_error)?
        .is_none()
    {
        return Err(error(StatusCode::NOT_FOUND, "thread not found"));
    }
    let messages = store
        .load_messages(&vault, &thread_id)
        .map_err(store_error)?;
    Ok(Json(messages))
}

/// `POST /api/v/{vault}/agent/threads/{thread_id}/messages` — append a message.
pub async fn append_message(
    State(state): State<SharedAppState>,
    Path((vault, thread_id)): Path<(String, String)>,
    Json(req): Json<AppendMessageRequest>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    let store = transcripts(&state).await;
    let message = store
        .append_message(&vault, &thread_id, req.role, &req.content)
        .map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(message)))
}
