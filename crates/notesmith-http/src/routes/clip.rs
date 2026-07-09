use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_clip::{FetchLimits, canonicalize_url, clip_url_to_note};
use notesmith_core::VaultPath;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{sanitize_slug, write_note};

#[derive(Debug, Deserialize)]
pub struct ClipRequest {
    /// URL of the page to clip.
    pub url: String,
    /// Extra tags to add alongside the mandatory `inbox` tag.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Look up an existing note whose `source_url` field equals `canonical`.
fn find_existing(cache: &notesmith_index::VaultCache, canonical: &str) -> Option<String> {
    let escaped = canonical.replace('\'', "''");
    let sql = format!(
        "SELECT note_path FROM v_fields WHERE key = 'source_url' AND value = '{escaped}' LIMIT 1"
    );
    let result = notesmith_query::execute_sql_with_options(cache, &sql, Some(1)).ok()?;
    result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

pub async fn clip_note(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<ClipRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Snapshot the config and dedup-check under the read lock, then release it
    // before the (potentially slow) network fetch so we never hold the lock
    // across `.await` on the network.
    let (clip_folder, capture_folder, existing_for_input) = {
        let state = state.read().await;
        let vault = state.vaults.get(&vault_name).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("vault not found: {vault_name}") })),
            )
        })?;

        let config = vault.vault_config.load();
        if !config.clip.enabled {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "clipping is disabled for this vault" })),
            ));
        }

        let canonical_input = canonicalize_url(&request.url);
        let existing = find_existing(&vault.cache, &canonical_input);
        (
            config.clip.folder.clone(),
            config.capture.folder.clone(),
            existing,
        )
    };

    // Fast path: the input URL is already clipped.
    if let Some(path) = existing_for_input {
        return Ok((
            StatusCode::OK,
            Json(json!({ "path": path, "duplicate": true })),
        ));
    }

    // Fetch + extract + render (no lock held across the network await).
    let (note_content, doc) =
        clip_url_to_note(&request.url, &request.tags, &FetchLimits::default())
            .await
            .map_err(map_clip_error)?;

    // Re-acquire the lock to dedup against the post-redirect canonical URL and
    // to write the note.
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    if doc.source_url != canonicalize_url(&request.url) {
        if let Some(path) = find_existing(&vault.cache, &doc.source_url) {
            return Ok((
                StatusCode::OK,
                Json(json!({ "path": path, "duplicate": true })),
            ));
        }
    }

    let folder = if !clip_folder.is_empty() {
        clip_folder
    } else {
        capture_folder
    };
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string();
    let slug = sanitize_slug(&doc.title);
    let filename = if slug.is_empty() {
        format!("{timestamp}.md")
    } else {
        format!("{timestamp} - {slug}.md")
    };
    let note_path = if folder.is_empty() {
        VaultPath::new(filename)
    } else {
        VaultPath::new(format!("{folder}/{filename}"))
    };

    let response = write_note(&vault.engine, &vault.root, &note_path, None, &note_content)?;

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteClipped, note_path.as_str()),
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "path": response.path,
            "hash": response.hash,
            "source_url": doc.source_url,
            "title": doc.title,
            "duplicate": false,
        })),
    ))
}

fn map_clip_error(error: notesmith_clip::ClipError) -> (StatusCode, Json<Value>) {
    use notesmith_clip::ClipError;
    let status = match error {
        ClipError::InvalidUrl(_) | ClipError::Blocked(_) => StatusCode::BAD_REQUEST,
        ClipError::TooLarge(_, _) => StatusCode::PAYLOAD_TOO_LARGE,
        ClipError::Fetch(_) => StatusCode::BAD_GATEWAY,
        ClipError::Extract(_) => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (status, Json(json!({ "error": error.to_string() })))
}
