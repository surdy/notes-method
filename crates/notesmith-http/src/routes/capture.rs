use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_core::VaultPath;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{WriteNoteResponse, sanitize_slug, write_note};

#[derive(Debug, Deserialize)]
pub struct CaptureNoteRequest {
    pub text: String,
    pub title: Option<String>,
}

pub async fn capture_note(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<CaptureNoteRequest>,
) -> Result<(StatusCode, Json<WriteNoteResponse>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let config = vault.vault_config.load();
    let capture_folder = &config.capture.folder;
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string();

    let slug = match &request.title {
        Some(title) => sanitize_slug(title),
        None => sanitize_slug(&request.text.chars().take(40).collect::<String>()),
    };

    let filename = if slug.is_empty() {
        format!("{timestamp}.md")
    } else {
        format!("{timestamp} - {slug}.md")
    };

    let note_path = if capture_folder.is_empty() {
        VaultPath::new(filename)
    } else {
        VaultPath::new(format!("{capture_folder}/{filename}"))
    };
    let content = request.text.clone();
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteCaptured, note_path.as_str()),
    );

    Ok((StatusCode::CREATED, Json(response)))
}
