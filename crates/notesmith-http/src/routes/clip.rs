use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Local};
use notesmith_clip::{
    ClipTemplate, FetchLimits, YoutubeOutcome, canonicalize_url, canonicalize_youtube_url,
    clip_url, download_and_rewrite_images, fetch_youtube, host_of, is_youtube_url,
    render_note_with_template, render_youtube_note_with_template, select_template,
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
    // YouTube is a `source_type: youtube` module on the same shared clip flow
    // (ADR 0020 §8): detect it up front and branch, leaving the article path
    // below unchanged for every other URL.
    if is_youtube_url(&request.url) {
        return clip_youtube(state, vault_name, request).await;
    }

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

/// Post-fetch plan for a YouTube clip. Pure, so the outcome → note/response
/// mapping is unit-testable without hitting the network.
#[derive(Debug, PartialEq)]
enum YoutubeClipPlan {
    /// A caption track was found: write this rendered note.
    Note {
        content: String,
        title: String,
        source_url: String,
    },
    /// No usable captions: hand off to the transcription worker (non-fatal).
    NoCaptions {
        source_url: String,
        video_id: String,
        /// JSON provenance blob (title/channel/published/duration) recorded on
        /// the queue row so the worker can render a faithful note later.
        meta_json: String,
    },
}

/// Serialize a [`YoutubeMeta`] into the compact provenance JSON stored on a
/// pending-transcription queue row. Best-effort: falls back to `"{}"` if
/// serialization somehow fails so a clip never errors on provenance.
fn youtube_meta_json(meta: &notesmith_clip::YoutubeMeta) -> String {
    serde_json::to_string(&json!({
        "title": meta.title,
        "channel": meta.channel,
        "published": meta.published,
        "duration": meta.duration,
        "video_id": meta.video_id,
        "source_url": meta.source_url,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

/// Turn a [`YoutubeOutcome`] into a [`YoutubeClipPlan`], selecting the per-domain
/// template (ADR 0020 §8.5) for the captions case.
fn plan_youtube_clip(
    outcome: YoutubeOutcome,
    tags: &[String],
    templates: &[ClipTemplate],
    now: DateTime<Local>,
) -> YoutubeClipPlan {
    match outcome {
        YoutubeOutcome::Captions(transcript) => {
            let host = host_of(&transcript.meta.source_url);
            let template = select_template(templates, &host);
            let content = render_youtube_note_with_template(&transcript, tags, now, template);
            let title = transcript
                .meta
                .title
                .clone()
                .unwrap_or_else(|| transcript.meta.source_url.clone());
            YoutubeClipPlan::Note {
                content,
                title,
                source_url: transcript.meta.source_url,
            }
        }
        YoutubeOutcome::NoCaptions(meta) => YoutubeClipPlan::NoCaptions {
            meta_json: youtube_meta_json(&meta),
            source_url: meta.source_url,
            video_id: meta.video_id,
        },
    }
}

/// Append a YouTube-transcription intent to the vault's pending queue (ADR 0023
/// §5: the daemon enqueues only). Keyed by canonical `source_url` so repeated
/// clips of a caption-less video are idempotent. Returns whether a new row was
/// inserted; any queue error is logged and swallowed so a clip never fails on
/// the transcription handoff (resilience policy, ADR 0009).
fn enqueue_youtube_transcription(vault_name: &str, source_url: &str, meta_json: &str) -> bool {
    let entry = notesmith_transcribe::NewQueueEntry {
        source_url: source_url.to_string(),
        source_type: notesmith_transcribe::SOURCE_TYPE_YOUTUBE.to_string(),
        audio_path: None,
        meta_json: meta_json.to_string(),
    };
    let result = notesmith_transcribe::queue_db_path(vault_name)
        .and_then(|path| notesmith_transcribe::TranscriptionQueue::open(&path))
        .and_then(|queue| queue.enqueue(&entry));
    match result {
        Ok(notesmith_transcribe::EnqueueOutcome::Inserted) => true,
        Ok(notesmith_transcribe::EnqueueOutcome::Existed) => false,
        Err(error) => {
            tracing::warn!(
                vault = %vault_name,
                source_url = %source_url,
                reason = %error,
                "could not enqueue youtube transcription; clip still succeeded"
            );
            false
        }
    }
}
/// the article path: same `clip.enabled` gate, canonical-URL dedup, per-domain
/// templates, inbox write, and `NoteClipped` event. Captions are fetched with a
/// single bounded, SSRF-guarded `GET`; the daemon never runs Whisper.
async fn clip_youtube(
    state: SharedAppState,
    vault_name: String,
    request: ClipRequest,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Snapshot config + dedup under the read lock, release before the fetch.
    let (clip_folder, capture_folder, templates, existing_for_input, canonical) = {
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

        // Canonical identity is the watch URL; fall back to the generic
        // canonicalizer if the id can't be parsed (dedup still best-effort).
        let canonical = canonicalize_youtube_url(&request.url)
            .unwrap_or_else(|| canonicalize_url(&request.url));
        let existing = find_existing(&vault.cache, &canonical);
        let templates: Vec<ClipTemplate> =
            config.clip.templates.iter().map(to_clip_template).collect();
        (
            config.clip.folder.clone(),
            config.capture.folder.clone(),
            templates,
            existing,
            canonical,
        )
    };

    // Fast path: the video is already clipped.
    if let Some(path) = existing_for_input {
        return Ok((
            StatusCode::OK,
            Json(json!({ "path": path, "duplicate": true })),
        ));
    }

    // Fetch the published caption track (bounded, SSRF-guarded, no lock held).
    let outcome = fetch_youtube(&request.url, &FetchLimits::default())
        .await
        .map_err(map_clip_error)?;

    match plan_youtube_clip(outcome, &request.tags, &templates, chrono::Local::now()) {
        YoutubeClipPlan::Note {
            content,
            title,
            source_url,
        } => {
            let state = state.read().await;
            let vault = state.vaults.get(&vault_name).ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": format!("vault not found: {vault_name}") })),
                )
            })?;

            // Re-check dedup against the resolved canonical URL.
            if source_url != canonical {
                if let Some(path) = find_existing(&vault.cache, &source_url) {
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
            let slug = sanitize_slug(&title);
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

            let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

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
                    "source_url": source_url,
                    "title": title,
                    "source_type": notesmith_clip::SOURCE_TYPE_YOUTUBE,
                    "duplicate": false,
                })),
            ))
        }
        YoutubeClipPlan::NoCaptions {
            source_url,
            video_id,
            meta_json,
        } => {
            // Non-fatal: no published captions. The daemon must not run Whisper
            // (ADR 0019 §4 / ADR 0020 §8.3 / ADR 0023 §5); it only records the
            // intent by appending a row to the per-vault pending-transcription
            // queue. The colocated `notesmith transcribe --drain` worker (P2c)
            // acquires the audio and renders the note out of process. A queue
            // failure must never fail the clip, so it is logged, not returned.
            let queued = enqueue_youtube_transcription(&vault_name, &source_url, &meta_json);
            Ok((
                StatusCode::OK,
                Json(json!({
                    "status": "no_captions",
                    "source_url": source_url,
                    "video_id": video_id,
                    "source_type": notesmith_clip::SOURCE_TYPE_YOUTUBE,
                    "queued": queued,
                    "message": "no published captions; queued for transcription",
                })),
            ))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_clip::{TranscriptSegment, YoutubeMeta, YoutubeTranscript};
    use std::collections::BTreeMap;

    fn meta() -> YoutubeMeta {
        YoutubeMeta {
            video_id: "dQw4w9WgXcQ".into(),
            source_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            title: Some("Never Gonna Give You Up".into()),
            channel: Some("Rick Astley".into()),
            published: Some("2009-10-25".into()),
            duration: Some(212),
        }
    }

    fn now() -> DateTime<Local> {
        use chrono::TimeZone;
        Local.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap()
    }

    #[test]
    fn captions_outcome_plans_a_note() {
        let transcript = YoutubeTranscript {
            meta: meta(),
            segments: vec![TranscriptSegment {
                start: 0.0,
                end: 2.0,
                text: "We're no strangers to love".into(),
            }],
        };
        let plan = plan_youtube_clip(
            YoutubeOutcome::Captions(transcript),
            &["watch-later".into()],
            &[],
            now(),
        );
        match plan {
            YoutubeClipPlan::Note {
                content,
                title,
                source_url,
            } => {
                assert_eq!(title, "Never Gonna Give You Up");
                assert_eq!(source_url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
                assert!(content.contains("source_type: youtube"));
                assert!(content.contains("[0:00] We're no strangers to love"));
                assert!(content.contains("- watch-later"));
            }
            other => panic!("expected Note, got {other:?}"),
        }
    }

    #[test]
    fn captions_outcome_applies_matching_template() {
        let transcript = YoutubeTranscript {
            meta: meta(),
            segments: vec![TranscriptSegment {
                start: 0.0,
                end: 2.0,
                text: "line".into(),
            }],
        };
        let mut fm = BTreeMap::new();
        fm.insert("channel_note".to_string(), "{{ channel }}".to_string());
        let template = ClipTemplate {
            match_host: "youtube.com".to_string(),
            frontmatter: fm,
            body: Some("# {{ title }}\n\n{{ content }}".to_string()),
        };
        let plan = plan_youtube_clip(
            YoutubeOutcome::Captions(transcript),
            &[],
            std::slice::from_ref(&template),
            now(),
        );
        match plan {
            YoutubeClipPlan::Note { content, .. } => {
                assert!(content.contains("channel_note: Rick Astley"));
                assert!(content.contains("# Never Gonna Give You Up"));
            }
            other => panic!("expected Note, got {other:?}"),
        }
    }

    #[test]
    fn no_captions_outcome_plans_worker_handoff() {
        let plan = plan_youtube_clip(YoutubeOutcome::NoCaptions(meta()), &[], &[], now());
        match plan {
            YoutubeClipPlan::NoCaptions {
                source_url,
                video_id,
                meta_json,
            } => {
                assert_eq!(source_url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
                assert_eq!(video_id, "dQw4w9WgXcQ");
                // Provenance is captured for the worker to render later.
                let parsed: serde_json::Value = serde_json::from_str(&meta_json).unwrap();
                assert_eq!(parsed["video_id"], "dQw4w9WgXcQ");
            }
            other => panic!("expected NoCaptions, got {other:?}"),
        }
    }
}
