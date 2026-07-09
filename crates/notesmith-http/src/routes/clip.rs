use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_clip::{
    ClipTemplate, FetchLimits, canonicalize_url, clip_url, download_and_rewrite_images, host_of,
    render_note_with_template, select_template,
};
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
    let (
        clip_folder,
        capture_folder,
        download_images,
        attachments_folder,
        templates,
        existing_for_input,
    ) = {
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
        let templates: Vec<ClipTemplate> =
            config.clip.templates.iter().map(to_clip_template).collect();
        (
            config.clip.folder.clone(),
            config.capture.folder.clone(),
            config.clip.download_images,
            config.clip.attachments_folder.clone(),
            templates,
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

    // Fetch + extract (no lock held across the network await).
    let mut doc = clip_url(&request.url, &FetchLimits::default())
        .await
        .map_err(map_clip_error)?;

    // Download and rewrite images before rendering, so a template body sees the
    // local links. Failures degrade to keeping the remote URL.
    let mut images = Vec::new();
    if download_images {
        let (rewritten, downloaded) = download_and_rewrite_images(
            &doc.markdown,
            &doc.source_url,
            &attachments_folder,
            &FetchLimits::default(),
        )
        .await;
        doc.markdown = rewritten;
        images = downloaded;
    }

    // Select the per-domain template (if any) by host and render.
    let host = host_of(&doc.source_url);
    let template = select_template(&templates, &host);
    let note_content =
        render_note_with_template(&doc, &request.tags, chrono::Local::now(), template);

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

    // Persist downloaded images into the attachments folder (best-effort: a
    // failed write leaves the note's local link dangling but never aborts).
    let saved_images = write_images(&vault.root, &attachments_folder, &images);

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
            "images": saved_images,
            "duplicate": false,
        })),
    ))
}

/// Map a config-layer [`notesmith_config::ClipTemplate`] to the clip crate's
/// template model.
fn to_clip_template(template: &notesmith_config::ClipTemplate) -> ClipTemplate {
    ClipTemplate {
        match_host: template.match_host.clone(),
        frontmatter: template.frontmatter.clone(),
        body: template.body.clone(),
    }
}

/// Write downloaded clip images into `<root>/<folder>/`. Returns the number of
/// images successfully written. Best-effort per image.
fn write_images(
    root: &std::path::Path,
    folder: &str,
    images: &[notesmith_clip::DownloadedImage],
) -> usize {
    if images.is_empty() {
        return 0;
    }
    let dir = root.join(folder);
    if let Err(reason) = std::fs::create_dir_all(&dir) {
        tracing::warn!(folder = %folder, reason = %reason, "clip image folder create failed");
        return 0;
    }
    let mut written = 0;
    for image in images {
        let path = dir.join(&image.filename);
        match std::fs::write(&path, &image.bytes) {
            Ok(()) => written += 1,
            Err(reason) => {
                tracing::warn!(file = %image.filename, reason = %reason, "clip image write failed");
            }
        }
    }
    written
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
