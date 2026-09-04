//! YouTube as a clip source ([ADR 0020](../../docs/adr/0020-web-clipper.md) §8).
//!
//! YouTube is a `source_type: youtube` module on the same shared extraction
//! library as article clipping. It is the single fetch/parse/normalize path
//! called by **both** the interactive clip endpoint and the `youtube_transcript`
//! MCP tool — no forked logic (ADR 0020 §2/§8.1).
//!
//! The pipeline mirrors the article path but for captions:
//! `fetch watch page (SSRF-guarded, bounded)` → `parse player response` →
//! `select caption track` → `fetch + parse timedtext` → `normalize to a
//! timestamped Markdown note with media provenance frontmatter`.
//!
//! Per [ADR 0019](../../docs/adr/0019-media-ingestion-pipeline.md) §4, this
//! module **only** consumes a *published caption track* — a single bounded
//! `GET`. When no usable caption track exists it returns a typed, non-fatal
//! [`YoutubeOutcome::NoCaptions`] so callers can hand the video to the
//! Whisper-capable worker. This module never transcribes audio.
//!
//! All fetched content is untrusted per
//! [ADR 0009](../../docs/adr/0009-resilience-to-malformed-content.md): parsing
//! degrades to `None`/empty and never panics.

use chrono::{DateTime, Local};
use minijinja::{Environment, context};
use serde_json::Value as Json;
use serde_yaml::{Mapping, Value as Yaml};
use url::Url;

use crate::error::ClipError;
use crate::fetch::{FetchLimits, fetch_html, fetch_json_post};
use crate::note::resolve_tags;
use crate::template::{ClipTemplate, apply_template_frontmatter};

pub use notesmith_transcribe::TranscriptSegment;
use notesmith_transcribe::transcript_body as render_transcript_body;

/// The `source_type` value used for YouTube clips.
pub const SOURCE_TYPE_YOUTUBE: &str = "youtube";

/// YouTube's unofficial InnerTube player endpoint. Fetched with the ANDROID
/// client context below, it returns caption `baseUrl`s that serve real
/// timedtext — unlike the watch page's `ytInitialPlayerResponse` baseUrls,
/// which are PoToken/session-locked and return empty bodies to plain scrapes
/// ([ADR 0020](../../docs/adr/0020-web-clipper.md) §8).
const INNERTUBE_PLAYER_URL: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
/// Pinned ANDROID client version for the InnerTube context.
const INNERTUBE_CLIENT_VERSION: &str = "20.10.38";

/// Hosts recognized as YouTube.
const YOUTUBE_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
];

/// Provenance metadata for a YouTube video, independent of whether captions
/// exist. Carries everything a Whisper-fallback handoff needs.
#[derive(Debug, Clone, PartialEq)]
pub struct YoutubeMeta {
    /// The 11-character video id.
    pub video_id: String,
    /// Canonical `https://www.youtube.com/watch?v=<id>` URL (the dedup key).
    pub source_url: String,
    /// Video title, when detected.
    pub title: Option<String>,
    /// Channel / author name, when detected.
    pub channel: Option<String>,
    /// Publish date string as reported by the source, when detected.
    pub published: Option<String>,
    /// Video duration in seconds, when detected.
    pub duration: Option<u64>,
}

/// A fully normalized YouTube transcript: provenance metadata plus timestamped
/// segments.
#[derive(Debug, Clone, PartialEq)]
pub struct YoutubeTranscript {
    /// Provenance metadata.
    pub meta: YoutubeMeta,
    /// Timestamped caption segments in playback order.
    pub segments: Vec<TranscriptSegment>,
}

/// Outcome of attempting to fetch a YouTube transcript from captions.
///
/// Both variants are non-fatal successes: [`YoutubeOutcome::NoCaptions`] tells
/// the caller to hand the video to the Whisper worker rather than treating the
/// absence of captions as an error (ADR 0020 §8.3).
#[derive(Debug, Clone, PartialEq)]
pub enum YoutubeOutcome {
    /// A usable caption track was found and parsed.
    Captions(YoutubeTranscript),
    /// No usable published caption track exists; hand off to the worker.
    NoCaptions(YoutubeMeta),
}

/// An audio-only adaptive stream advertised by the player response's
/// `streamingData.adaptiveFormats`. Only formats exposing a direct `url` are
/// captured — signature-ciphered formats require the player JS and are out of
/// scope (ADR 0023 §6, no `yt-dlp`).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFormat {
    /// YouTube format tag (e.g. 140 = AAC/m4a, 251 = Opus/webm).
    pub itag: u32,
    /// Direct progressive download URL.
    pub url: String,
    /// Full `mimeType` (e.g. `audio/mp4; codecs="mp4a.40.2"`).
    pub mime_type: String,
    /// Average bitrate in bits/sec, when reported (used to prefer smaller
    /// streams — Whisper resamples to 16 kHz mono regardless of quality).
    pub bitrate: Option<u64>,
    /// `contentLength` in bytes, when reported.
    pub content_length: Option<u64>,
}

impl AudioFormat {
    /// Whether this is an MP4/AAC (`audio/mp4`) stream — the container/codec the
    /// worker can decode without an external demuxer (ADR 0023 §6).
    pub fn is_mp4_aac(&self) -> bool {
        self.mime_type.starts_with("audio/mp4")
    }
}

/// A caption track advertised by the player response.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionTrack {
    /// URL of the timedtext track.
    pub base_url: String,
    /// BCP-47 language code (e.g. `en`).
    pub language_code: String,
    /// `true` when the track is auto-generated (`kind == "asr"`).
    pub auto_generated: bool,
}

/// Metadata extracted from `ytInitialPlayerResponse`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerResponse {
    pub title: Option<String>,
    pub channel: Option<String>,
    pub published: Option<String>,
    pub duration: Option<u64>,
    pub caption_tracks: Vec<CaptionTrack>,
    /// Audio-only adaptive formats with a direct URL (for the no-captions
    /// Whisper fallback, ADR 0023 §6).
    pub audio_formats: Vec<AudioFormat>,
}

/// Returns `true` when `url` points at a recognized YouTube host.
pub fn is_youtube_url(url: &str) -> bool {
    Url::parse(url.trim())
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|h| YOUTUBE_HOSTS.contains(&h.as_str()))
        .unwrap_or(false)
}

fn is_valid_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Extract the video id from any recognized YouTube URL form
/// (`watch?v=`, `youtu.be/<id>`, `/embed/<id>`, `/shorts/<id>`, `/live/<id>`).
///
/// Returns `None` for non-YouTube URLs or when no valid id is present.
pub fn youtube_video_id(url: &str) -> Option<String> {
    let parsed = Url::parse(url.trim()).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if !YOUTUBE_HOSTS.contains(&host.as_str()) {
        return None;
    }

    if host == "youtu.be" {
        let id = parsed.path_segments()?.next()?.to_string();
        return is_valid_video_id(&id).then_some(id);
    }

    // watch?v=<id>
    if let Some((_, v)) = parsed.query_pairs().find(|(k, _)| k == "v") {
        let id = v.into_owned();
        if is_valid_video_id(&id) {
            return Some(id);
        }
    }

    // /embed/<id>, /shorts/<id>, /live/<id>, /v/<id>
    let mut segs = parsed.path_segments()?;
    if let Some(first) = segs.next() {
        if matches!(first, "embed" | "shorts" | "live" | "v") {
            if let Some(id) = segs.next() {
                let id = id.to_string();
                return is_valid_video_id(&id).then_some(id);
            }
        }
    }

    None
}

/// Canonicalize a YouTube URL to `https://www.youtube.com/watch?v=<id>`.
///
/// This drops playback-position (`t`), playlist (`list`, `index`), and tracking
/// params so re-clipping the same video maps to one dedup key
/// (ADR 0019 §6 / ADR 0020 §8.2). Returns `None` when the URL is not a YouTube
/// video URL.
pub fn canonicalize_youtube_url(url: &str) -> Option<String> {
    let id = youtube_video_id(url)?;
    Some(format!("https://www.youtube.com/watch?v={id}"))
}

/// Decode a minimal set of HTML entities found in caption text.
fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';').filter(|&s| s <= 12) {
            let entity = &after[1..semi];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                other => other
                    .strip_prefix('#')
                    .and_then(|num| {
                        if let Some(hex) = num.strip_prefix(['x', 'X']) {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            num.parse::<u32>().ok()
                        }
                    })
                    .and_then(char::from_u32),
            };
            if let Some(ch) = decoded {
                out.push(ch);
                rest = &after[semi + 1..];
                continue;
            }
        }
        // Not a recognized entity: keep the literal '&' and continue past it.
        out.push('&');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

/// Strip XML/HTML tags from a caption fragment.
fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Balanced-brace extraction of the JSON object that follows `marker` in `html`.
///
/// Honors string literals and escapes so braces inside strings do not unbalance
/// the scan. Returns `None` if the marker is absent or the object is truncated.
fn extract_json_object_after<'a>(html: &'a str, marker: &str) -> Option<&'a str> {
    let start = html.find(marker)? + marker.len();
    let bytes = html.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i] != b'{' {
        // Only skip whitespace and the assignment operator between marker and object.
        if !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | b'=') {
            return None;
        }
        i += 1;
    }
    let obj_start = i;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&html[obj_start..=i]);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn json_str(v: &Json, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

/// Parse `ytInitialPlayerResponse` out of a watch-page HTML body.
///
/// Resilient (ADR 0009): a missing/broken player response yields `None` and
/// individual missing fields degrade to `None` rather than failing the parse.
pub fn parse_player_response(html: &str) -> Option<PlayerResponse> {
    let raw = extract_json_object_after(html, "ytInitialPlayerResponse")?;
    let json: Json = serde_json::from_str(raw).ok()?;
    Some(parse_player_json(&json))
}

/// Parse a player-response JSON value (as returned by the InnerTube
/// `/youtubei/v1/player` endpoint, or embedded as `ytInitialPlayerResponse`)
/// into a [`PlayerResponse`].
///
/// Resilient (ADR 0009): every field degrades to `None`/empty independently.
pub fn parse_player_json(json: &Json) -> PlayerResponse {
    let title = json_str(json, &["videoDetails", "title"]);
    let channel = json_str(json, &["videoDetails", "author"]);
    let duration =
        json_str(json, &["videoDetails", "lengthSeconds"]).and_then(|s| s.parse::<u64>().ok());
    let published = json_str(
        json,
        &["microformat", "playerMicroformatRenderer", "publishDate"],
    )
    .or_else(|| {
        json_str(
            json,
            &["microformat", "playerMicroformatRenderer", "uploadDate"],
        )
    });

    let mut caption_tracks = Vec::new();
    if let Some(tracks) = json
        .get("captions")
        .and_then(|c| c.get("playerCaptionsTracklistRenderer"))
        .and_then(|r| r.get("captionTracks"))
        .and_then(Json::as_array)
    {
        for t in tracks {
            let Some(base_url) = t.get("baseUrl").and_then(Json::as_str) else {
                continue;
            };
            caption_tracks.push(CaptionTrack {
                base_url: base_url.to_string(),
                language_code: t
                    .get("languageCode")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                auto_generated: t.get("kind").and_then(Json::as_str) == Some("asr"),
            });
        }
    }

    PlayerResponse {
        title,
        channel,
        published,
        duration,
        caption_tracks,
        audio_formats: parse_audio_formats(json),
    }
}

/// Parse audio-only entries from `streamingData.adaptiveFormats`. Only formats
/// with a direct `url` are kept (signature-ciphered formats need the player JS
/// and are out of scope, ADR 0023 §6). Resilient (ADR 0009): a missing/broken
/// `streamingData` yields an empty vec.
fn parse_audio_formats(json: &Json) -> Vec<AudioFormat> {
    let Some(formats) = json
        .get("streamingData")
        .and_then(|s| s.get("adaptiveFormats"))
        .and_then(Json::as_array)
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for f in formats {
        let mime_type = f
            .get("mimeType")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        if !mime_type.starts_with("audio/") {
            continue;
        }
        // Skip ciphered formats: no direct URL means we'd need the player JS.
        let Some(url) = f.get("url").and_then(Json::as_str) else {
            continue;
        };
        let itag = f.get("itag").and_then(Json::as_u64).unwrap_or(0) as u32;
        let bitrate = f
            .get("averageBitrate")
            .and_then(Json::as_u64)
            .or_else(|| f.get("bitrate").and_then(Json::as_u64));
        // `contentLength` is a stringified integer in the InnerTube response.
        let content_length = f
            .get("contentLength")
            .and_then(Json::as_str)
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| f.get("contentLength").and_then(Json::as_u64));
        out.push(AudioFormat {
            itag,
            url: url.to_string(),
            mime_type,
            bitrate,
            content_length,
        });
    }
    out
}

/// Select the best audio-only stream to transcribe: prefer an MP4/AAC stream
/// (decodable without an external demuxer, ADR 0023 §6), choosing the smallest
/// bitrate among them (Whisper resamples to 16 kHz mono, so higher bitrate is
/// wasted bandwidth); fall back to the smallest-bitrate audio stream of any
/// codec if no MP4/AAC exists.
pub fn select_audio_format(formats: &[AudioFormat]) -> Option<&AudioFormat> {
    let by_bitrate = |a: &&AudioFormat, b: &&AudioFormat| {
        a.bitrate
            .unwrap_or(u64::MAX)
            .cmp(&b.bitrate.unwrap_or(u64::MAX))
    };
    formats
        .iter()
        .filter(|f| f.is_mp4_aac())
        .min_by(by_bitrate)
        .or_else(|| formats.iter().min_by(by_bitrate))
}

/// Select the best caption track: prefer a manual (non-ASR) track in
/// `preferred_lang`, then any manual track, then ASR in `preferred_lang`, then
/// any track at all.
pub fn select_caption_track<'a>(
    tracks: &'a [CaptionTrack],
    preferred_lang: &str,
) -> Option<&'a CaptionTrack> {
    let pref = |t: &CaptionTrack| t.language_code.eq_ignore_ascii_case(preferred_lang);
    tracks
        .iter()
        .find(|t| !t.auto_generated && pref(t))
        .or_else(|| tracks.iter().find(|t| !t.auto_generated))
        .or_else(|| tracks.iter().find(|t| pref(t)))
        .or_else(|| tracks.first())
}

/// Parse a YouTube `timedtext` XML body into ordered [`TranscriptSegment`]s.
///
/// Handles both timedtext formats returned by YouTube:
/// - **srv3** (`fmt=srv3`, what the InnerTube ANDROID track serves):
///   `<p t="ms" d="ms">...<s>word</s>...</p>` — times in **milliseconds**,
///   text optionally split across nested `<s>` word tags.
/// - **legacy** (`<text start="s" dur="s">...</text>`): times in **seconds**.
///
/// Resilient: malformed or partial entries are skipped; a body with no parseable
/// entries yields an empty vec (treated as "no captions" by callers).
pub fn parse_timedtext(xml: &str) -> Vec<TranscriptSegment> {
    let srv3 = parse_timedtext_srv3(xml);
    if !srv3.is_empty() {
        return srv3;
    }
    parse_timedtext_legacy(xml)
}

/// Parse the srv3 `<p t="ms" d="ms">` format (milliseconds).
fn parse_timedtext_srv3(xml: &str) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<p ") {
        let after_open = &rest[open..];
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let attrs = &after_open[..gt];
        let body_and_rest = &after_open[gt + 1..];
        let (inner, next) = match body_and_rest.find("</p>") {
            Some(close) => (&body_and_rest[..close], &body_and_rest[close + 4..]),
            None => (body_and_rest, ""),
        };
        rest = next;

        let Some(start_ms) = attr_value(attrs, "t").and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        let dur_ms = attr_value(attrs, "d")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let text = decode_entities(&strip_tags(inner)).trim().to_string();
        if text.is_empty() {
            continue;
        }
        segments.push(TranscriptSegment::new(
            start_ms / 1000.0,
            (start_ms + dur_ms) / 1000.0,
            text,
        ));
    }
    segments
}

/// Parse the legacy `<text start="s" dur="s">` format (seconds).
fn parse_timedtext_legacy(xml: &str) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<text") {
        let after_open = &rest[open..];
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let attrs = &after_open[..gt];
        let body_and_rest = &after_open[gt + 1..];
        let (inner, next) = match body_and_rest.find("</text>") {
            Some(close) => (&body_and_rest[..close], &body_and_rest[close + 7..]),
            None => (body_and_rest, ""),
        };
        rest = next;

        let Some(start) = attr_value(attrs, "start").and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        let dur = attr_value(attrs, "dur")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let text = decode_entities(&strip_tags(inner)).trim().to_string();
        if text.is_empty() {
            continue;
        }
        segments.push(TranscriptSegment::new(start, start + dur, text));
    }
    segments
}

/// Extract the value of `name="..."` from a raw tag-attribute string.
fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let idx = attrs.find(&key)? + key.len();
    let rest = &attrs[idx..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Media provenance frontmatter (ADR 0019 §3) for a YouTube video.
fn youtube_frontmatter(meta: &YoutubeMeta, ingested_at: &str, tag_values: &[Yaml]) -> Mapping {
    let mut fm = Mapping::new();
    let title = meta
        .title
        .clone()
        .unwrap_or_else(|| meta.source_url.clone());
    fm.insert(Yaml::from("title"), Yaml::from(title));
    fm.insert(
        Yaml::from("source_url"),
        Yaml::from(meta.source_url.clone()),
    );
    fm.insert(Yaml::from("source_type"), Yaml::from(SOURCE_TYPE_YOUTUBE));
    if let Some(channel) = &meta.channel {
        fm.insert(Yaml::from("channel"), Yaml::from(channel.clone()));
    }
    if let Some(published) = &meta.published {
        fm.insert(Yaml::from("published"), Yaml::from(published.clone()));
    }
    if let Some(duration) = meta.duration {
        fm.insert(Yaml::from("duration"), Yaml::from(duration));
    }
    fm.insert(Yaml::from("ingested_at"), Yaml::from(ingested_at));
    fm.insert(Yaml::from("tags"), Yaml::Sequence(tag_values.to_vec()));
    fm
}

/// The timestamped transcript body (`[M:SS] text` per line).
fn transcript_body(transcript: &YoutubeTranscript) -> String {
    render_transcript_body(&transcript.segments)
}

/// Render a [`YoutubeTranscript`] into a Markdown note with media provenance
/// frontmatter (ADR 0019 §3) and a timestamped transcript body.
///
/// `extra_tags` are appended after the mandatory `inbox` tag. `now` controls the
/// `ingested_at` timestamp.
pub fn render_youtube_note(
    transcript: &YoutubeTranscript,
    extra_tags: &[String],
    now: DateTime<Local>,
) -> String {
    render_youtube_note_with_template(transcript, extra_tags, now, None)
}

/// Render a [`YoutubeTranscript`] into a Markdown note, optionally applying a
/// per-domain `template` ([`ClipTemplate`]) — the media equivalent of
/// [`crate::render_note_with_template`] (ADR 0020 §8.5).
///
/// The minijinja context exposes `title`, `source_url`, `source_type`,
/// `channel`, `published`, `duration`, `content` (the timestamped transcript),
/// `host`, `ingested_at`, and `tags`. Rendering is resilient: a broken template
/// body degrades to the default transcript body (ADR 0009).
pub fn render_youtube_note_with_template(
    transcript: &YoutubeTranscript,
    extra_tags: &[String],
    now: DateTime<Local>,
    template: Option<&ClipTemplate>,
) -> String {
    let meta = &transcript.meta;
    let (tag_values, tag_strings) = resolve_tags(extra_tags);
    let ingested_at = now.to_rfc3339();

    let mut fm = youtube_frontmatter(meta, &ingested_at, &tag_values);
    let default_body = transcript_body(transcript);

    let body = if let Some(template) = template {
        let env = Environment::new();
        let ctx = context! {
            title => meta.title.clone().unwrap_or_else(|| meta.source_url.clone()),
            source_url => meta.source_url.clone(),
            source_type => SOURCE_TYPE_YOUTUBE,
            channel => meta.channel.clone(),
            published => meta.published.clone(),
            duration => meta.duration,
            content => default_body.clone(),
            host => crate::url::host_of(&meta.source_url),
            ingested_at => ingested_at.clone(),
            tags => tag_strings.clone(),
        };
        apply_template_frontmatter(&mut fm, template, &env, &ctx);
        match &template.body {
            Some(body_tpl) => env
                .render_str(body_tpl, &ctx)
                .unwrap_or_else(|_| default_body.clone())
                .trim()
                .to_string(),
            None => default_body.trim().to_string(),
        }
    } else {
        default_body
    };

    let yaml = serde_yaml::to_string(&Yaml::Mapping(fm))
        .unwrap_or_default()
        .trim_end()
        .to_string();

    format!("---\n{yaml}\n---\n\n{body}\n")
}

/// Fetch and normalize a YouTube video's *published caption track*.
///
/// Uses YouTube's InnerTube `/youtubei/v1/player` endpoint with the ANDROID
/// client context to obtain caption `baseUrl`s that actually serve timedtext
/// (the watch-page baseUrls are PoToken-locked). Both the player POST and the
/// timedtext GET go through the SSRF-guarded, bounded fetch path. Returns
/// [`YoutubeOutcome::NoCaptions`] (non-fatal) when no usable caption track
/// exists — the caller hands off to the Whisper worker. This function never
/// transcribes audio (ADR 0019 §4 / ADR 0020 §8.3).
pub async fn fetch_youtube(url: &str, limits: &FetchLimits) -> Result<YoutubeOutcome, ClipError> {
    let video_id = youtube_video_id(url)
        .ok_or_else(|| ClipError::InvalidUrl(format!("not a YouTube video URL: {url}")))?;
    let source_url = format!("https://www.youtube.com/watch?v={video_id}");

    let player = fetch_innertube_player(&video_id, limits).await?;

    let meta = YoutubeMeta {
        video_id,
        source_url,
        title: player.title,
        channel: player.channel,
        published: player.published,
        duration: player.duration,
    };

    let Some(track) = select_caption_track(&player.caption_tracks, "en") else {
        return Ok(YoutubeOutcome::NoCaptions(meta));
    };

    let timedtext = fetch_html(&track.base_url, limits).await?;
    let segments = parse_timedtext(&timedtext.html);
    if segments.is_empty() {
        return Ok(YoutubeOutcome::NoCaptions(meta));
    }

    Ok(YoutubeOutcome::Captions(YoutubeTranscript {
        meta,
        segments,
    }))
}

/// Fetch and parse the InnerTube player response for `video_id` (title,
/// channel, caption tracks, and audio-only adaptive formats). Public entry point
/// for the transcription worker's YouTube audio fallback (ADR 0023 §6); the
/// captions path uses this internally too.
pub async fn fetch_youtube_player(
    video_id: &str,
    limits: &FetchLimits,
) -> Result<PlayerResponse, ClipError> {
    fetch_innertube_player(video_id, limits).await
}

/// POST the ANDROID InnerTube player request for `video_id` and parse the
/// response into a [`PlayerResponse`].
async fn fetch_innertube_player(
    video_id: &str,
    limits: &FetchLimits,
) -> Result<PlayerResponse, ClipError> {
    let body = serde_json::json!({
        "context": {
            "client": {
                "clientName": "ANDROID",
                "clientVersion": INNERTUBE_CLIENT_VERSION,
                "androidSdkVersion": 30,
                "hl": "en",
                "gl": "US",
            }
        },
        "videoId": video_id,
    });
    let body = serde_json::to_vec(&body).map_err(|e| ClipError::Fetch(e.to_string()))?;

    let user_agent =
        format!("com.google.android.youtube/{INNERTUBE_CLIENT_VERSION} (Linux; U; Android 11)");
    let headers = vec![
        ("Origin".to_string(), "https://www.youtube.com".to_string()),
        (
            "Referer".to_string(),
            "https://www.youtube.com/".to_string(),
        ),
    ];

    let resp = fetch_json_post(
        INNERTUBE_PLAYER_URL,
        body,
        "application/json",
        headers,
        Some(user_agent),
        limits,
    )
    .await?;

    let json: Json = serde_json::from_slice(&resp.bytes).map_err(|_| {
        ClipError::Extract("could not parse YouTube InnerTube player response".to_string())
    })?;
    Ok(parse_player_json(&json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use notesmith_transcribe::format_timestamp;

    #[test]
    fn recognizes_youtube_hosts() {
        assert!(is_youtube_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(is_youtube_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_youtube_url(
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(!is_youtube_url("https://example.com/watch?v=dQw4w9WgXcQ"));
        assert!(!is_youtube_url("not a url"));
    }

    #[test]
    fn extracts_video_id_from_all_forms() {
        let id = "dQw4w9WgXcQ";
        assert_eq!(
            youtube_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=30s").as_deref(),
            Some(id)
        );
        assert_eq!(
            youtube_video_id("https://youtu.be/dQw4w9WgXcQ?t=30").as_deref(),
            Some(id)
        );
        assert_eq!(
            youtube_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ").as_deref(),
            Some(id)
        );
        assert_eq!(
            youtube_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").as_deref(),
            Some(id)
        );
        assert_eq!(youtube_video_id("https://example.com/watch?v=x"), None);
        assert_eq!(
            youtube_video_id("https://www.youtube.com/watch?v=short"),
            None
        );
    }

    #[test]
    fn canonicalizes_dropping_time_and_playlist() {
        let want = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        assert_eq!(
            canonicalize_youtube_url(
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s&list=PL123&index=4"
            )
            .as_deref(),
            Some(want)
        );
        assert_eq!(
            canonicalize_youtube_url("https://youtu.be/dQw4w9WgXcQ?t=42").as_deref(),
            Some(want)
        );
        assert_eq!(
            canonicalize_youtube_url("https://m.youtube.com/watch?v=dQw4w9WgXcQ&feature=share")
                .as_deref(),
            Some(want)
        );
        assert_eq!(canonicalize_youtube_url("https://example.com/x"), None);
    }

    const PLAYER_HTML: &str = r#"<!DOCTYPE html><html><body>
<script>var ytInitialPlayerResponse = {"captions":{"playerCaptionsTracklistRenderer":{"captionTracks":[
  {"baseUrl":"https://www.youtube.com/api/timedtext?lang=en-asr","languageCode":"en","kind":"asr"},
  {"baseUrl":"https://www.youtube.com/api/timedtext?lang=en","languageCode":"en"}
]}},"videoDetails":{"title":"Never Gonna Give You Up","author":"Rick Astley","lengthSeconds":"212"},
"microformat":{"playerMicroformatRenderer":{"publishDate":"2009-10-25"}}};var x=1;</script>
</body></html>"#;

    #[test]
    fn parses_player_response_metadata_and_tracks() {
        let pr = parse_player_response(PLAYER_HTML).unwrap();
        assert_eq!(pr.title.as_deref(), Some("Never Gonna Give You Up"));
        assert_eq!(pr.channel.as_deref(), Some("Rick Astley"));
        assert_eq!(pr.duration, Some(212));
        assert_eq!(pr.published.as_deref(), Some("2009-10-25"));
        assert_eq!(pr.caption_tracks.len(), 2);
        assert!(pr.caption_tracks[0].auto_generated);
        assert!(!pr.caption_tracks[1].auto_generated);
    }

    #[test]
    fn missing_player_response_returns_none() {
        assert!(parse_player_response("<html><body>no data</body></html>").is_none());
    }

    #[test]
    fn broken_player_json_degrades_to_none() {
        // Truncated object: balanced-brace scan finds no closing brace.
        let html = r#"<script>var ytInitialPlayerResponse = {"videoDetails":{"title":"x""#;
        assert!(parse_player_response(html).is_none());
    }

    #[test]
    fn selects_manual_english_over_asr() {
        let pr = parse_player_response(PLAYER_HTML).unwrap();
        let track = select_caption_track(&pr.caption_tracks, "en").unwrap();
        assert!(!track.auto_generated);
    }

    #[test]
    fn selection_falls_back_to_asr_then_any() {
        let asr = CaptionTrack {
            base_url: "u".into(),
            language_code: "en".into(),
            auto_generated: true,
        };
        assert!(select_caption_track(std::slice::from_ref(&asr), "en").is_some());
        let other = CaptionTrack {
            base_url: "u".into(),
            language_code: "fr".into(),
            auto_generated: false,
        };
        // No English at all: falls back to the only track.
        let sel = select_caption_track(std::slice::from_ref(&other), "en").unwrap();
        assert_eq!(sel.language_code, "fr");
        assert!(select_caption_track(&[], "en").is_none());
    }

    #[test]
    fn parses_audio_only_adaptive_formats_skipping_ciphered_and_video() {
        let json = serde_json::json!({
            "streamingData": {
                "adaptiveFormats": [
                    {
                        "itag": 137,
                        "url": "https://r1.googlevideo.com/videoplayback?itag=137",
                        "mimeType": "video/mp4; codecs=\"avc1.640028\"",
                        "bitrate": 4_000_000
                    },
                    {
                        "itag": 140,
                        "url": "https://r1.googlevideo.com/videoplayback?itag=140",
                        "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                        "averageBitrate": 128_000,
                        "contentLength": "3456789"
                    },
                    {
                        "itag": 251,
                        "url": "https://r1.googlevideo.com/videoplayback?itag=251",
                        "mimeType": "audio/webm; codecs=\"opus\"",
                        "averageBitrate": 96_000
                    },
                    {
                        "itag": 141,
                        "signatureCipher": "s=abc&url=https%3A%2F%2Fexample.com",
                        "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                        "averageBitrate": 256_000
                    }
                ]
            }
        });
        let pr = parse_player_json(&json);
        // Video and ciphered (no url) formats are excluded.
        assert_eq!(pr.audio_formats.len(), 2);
        let m4a = pr.audio_formats.iter().find(|f| f.itag == 140).unwrap();
        assert!(m4a.is_mp4_aac());
        assert_eq!(m4a.bitrate, Some(128_000));
        assert_eq!(m4a.content_length, Some(3_456_789));
    }

    #[test]
    fn selects_smallest_mp4_aac_audio_format() {
        let formats = vec![
            AudioFormat {
                itag: 251,
                url: "opus".into(),
                mime_type: "audio/webm; codecs=\"opus\"".into(),
                bitrate: Some(96_000),
                content_length: None,
            },
            AudioFormat {
                itag: 140,
                url: "aac-hi".into(),
                mime_type: "audio/mp4; codecs=\"mp4a.40.2\"".into(),
                bitrate: Some(128_000),
                content_length: None,
            },
            AudioFormat {
                itag: 139,
                url: "aac-lo".into(),
                mime_type: "audio/mp4; codecs=\"mp4a.40.5\"".into(),
                bitrate: Some(48_000),
                content_length: None,
            },
        ];
        // Prefers MP4/AAC (decodable without a demuxer) and the smallest bitrate.
        let sel = select_audio_format(&formats).unwrap();
        assert_eq!(sel.itag, 139);
    }

    #[test]
    fn selects_any_audio_when_no_mp4_aac() {
        let formats = vec![AudioFormat {
            itag: 251,
            url: "opus".into(),
            mime_type: "audio/webm; codecs=\"opus\"".into(),
            bitrate: Some(96_000),
            content_length: None,
        }];
        assert_eq!(select_audio_format(&formats).unwrap().itag, 251);
        assert!(select_audio_format(&[]).is_none());
    }

    #[test]
    fn missing_streaming_data_yields_no_audio_formats() {
        let json = serde_json::json!({ "videoDetails": { "title": "x" } });
        assert!(parse_player_json(&json).audio_formats.is_empty());
    }

    const TIMEDTEXT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<transcript>
<text start="0.0" dur="3.36">We&#39;re no strangers to love</text>
<text start="3.36" dur="2.64">You know the &lt;rules&gt; &amp; so do I</text>
<text start="6.0" dur="2.0"> </text>
<text start="8.0" dur="4.0"><b>A full</b> commitment</text>
</transcript>"#;

    #[test]
    fn parses_timedtext_with_entities_and_tags() {
        let segs = parse_timedtext(TIMEDTEXT);
        assert_eq!(segs.len(), 3); // blank segment skipped
        assert_eq!(segs[0].start, 0.0);
        assert!((segs[0].end - 3.36).abs() < 1e-9);
        assert_eq!(segs[0].text, "We're no strangers to love");
        assert_eq!(segs[1].text, "You know the <rules> & so do I");
        assert_eq!(segs[2].text, "A full commitment");
    }

    #[test]
    fn empty_timedtext_yields_no_segments() {
        assert!(parse_timedtext("<transcript></transcript>").is_empty());
        assert!(parse_timedtext("garbage not xml").is_empty());
    }

    const TIMEDTEXT_SRV3: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<timedtext format="3">
<body>
<p t="0" d="3360"><s>We&#39;re</s><s> no</s><s> strangers</s></p>
<p t="3360" d="2640">You know the &lt;rules&gt; &amp; so do I</p>
<p t="6000" d="2000"> </p>
<p t="8000" d="4000">A full commitment</p>
</body>
</timedtext>"#;

    #[test]
    fn parses_srv3_timedtext_milliseconds() {
        let segs = parse_timedtext(TIMEDTEXT_SRV3);
        assert_eq!(segs.len(), 3); // blank segment skipped
        assert_eq!(segs[0].start, 0.0);
        assert!((segs[0].end - 3.36).abs() < 1e-9);
        assert_eq!(segs[0].text, "We're no strangers");
        assert_eq!(segs[1].start, 3.36);
        assert_eq!(segs[1].text, "You know the <rules> & so do I");
        assert_eq!(segs[2].start, 8.0);
        assert!((segs[2].end - 12.0).abs() < 1e-9);
        assert_eq!(segs[2].text, "A full commitment");
    }

    #[test]
    fn srv3_preferred_over_legacy_when_both_present() {
        // A body containing srv3 <p> tags must parse as srv3 (ms), not legacy.
        let segs = parse_timedtext(r#"<p t="1500" d="500">hi</p>"#);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 1.5);
    }

    #[test]
    fn timestamp_formatting() {
        assert_eq!(format_timestamp(0.0), "0:00");
        assert_eq!(format_timestamp(72.4), "1:12");
        assert_eq!(format_timestamp(3661.0), "1:01:01");
    }

    fn sample_transcript() -> YoutubeTranscript {
        YoutubeTranscript {
            meta: YoutubeMeta {
                video_id: "dQw4w9WgXcQ".into(),
                source_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
                title: Some("Never Gonna Give You Up".into()),
                channel: Some("Rick Astley".into()),
                published: Some("2009-10-25".into()),
                duration: Some(212),
            },
            segments: parse_timedtext(TIMEDTEXT),
        }
    }

    fn fixed_now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap()
    }

    #[test]
    fn renders_media_frontmatter_and_timestamped_body() {
        let note = render_youtube_note(&sample_transcript(), &["watch-later".into()], fixed_now());
        assert!(note.starts_with("---\n"));
        assert!(note.contains("source_type: youtube"));
        assert!(note.contains("source_url: https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(note.contains("channel: Rick Astley"));
        assert!(note.contains("published: 2009-10-25"));
        assert!(note.contains("duration: 212"));
        assert!(note.contains("- inbox"));
        assert!(note.contains("- watch-later"));
        assert!(note.contains("[0:00] We're no strangers to love"));
    }

    #[test]
    fn frontmatter_round_trips_as_valid_yaml() {
        let note = render_youtube_note(&sample_transcript(), &[], fixed_now());
        let fm = note
            .strip_prefix("---\n")
            .and_then(|s| s.split("\n---\n").next())
            .unwrap();
        let parsed: Yaml = serde_yaml::from_str(fm).unwrap();
        assert_eq!(parsed["source_type"].as_str().unwrap(), "youtube");
        assert_eq!(parsed["duration"].as_u64().unwrap(), 212);
    }

    #[test]
    fn note_omits_absent_optional_fields() {
        let mut t = sample_transcript();
        t.meta.channel = None;
        t.meta.published = None;
        t.meta.duration = None;
        let note = render_youtube_note(&t, &[], fixed_now());
        assert!(!note.contains("channel:"));
        assert!(!note.contains("published:"));
        assert!(!note.contains("duration:"));
    }

    #[test]
    fn template_adds_media_frontmatter_and_body() {
        use std::collections::BTreeMap;
        let mut fm = BTreeMap::new();
        fm.insert("category".to_string(), "{{ host }}".to_string());
        fm.insert("secs".to_string(), "{{ duration }}".to_string());
        let template = ClipTemplate {
            match_host: "youtube.com".to_string(),
            frontmatter: fm,
            body: Some(
                "# {{ title }}\n\n> {{ channel }} on {{ published }}\n\n{{ content }}".to_string(),
            ),
        };
        let note = render_youtube_note_with_template(
            &sample_transcript(),
            &[],
            fixed_now(),
            Some(&template),
        );
        assert!(note.contains("category: www.youtube.com"));
        // Numeric YAML scalar, not a quoted string.
        assert!(note.contains("secs: 212"));
        assert!(note.contains("# Never Gonna Give You Up"));
        assert!(note.contains("> Rick Astley on 2009-10-25"));
        assert!(note.contains("[0:00] We're no strangers to love"));
        assert!(note.contains("source_type: youtube"));
    }

    #[test]
    fn broken_template_body_falls_back_to_transcript() {
        let template = ClipTemplate {
            match_host: "youtube.com".to_string(),
            frontmatter: std::collections::BTreeMap::new(),
            body: Some("{{ unclosed".to_string()),
        };
        let note = render_youtube_note_with_template(
            &sample_transcript(),
            &[],
            fixed_now(),
            Some(&template),
        );
        assert!(note.contains("[0:00] We're no strangers to love"));
        assert!(note.contains("source_type: youtube"));
    }

    #[test]
    fn none_template_matches_render_youtube_note() {
        let with_none =
            render_youtube_note_with_template(&sample_transcript(), &[], fixed_now(), None);
        let plain = render_youtube_note(&sample_transcript(), &[], fixed_now());
        assert_eq!(with_none, plain);
    }

    #[tokio::test]
    async fn fetch_rejects_non_youtube_url() {
        let err = fetch_youtube("https://example.com/post", &FetchLimits::default())
            .await
            .unwrap_err();
        assert!(matches!(err, ClipError::InvalidUrl(_)));
    }
}
