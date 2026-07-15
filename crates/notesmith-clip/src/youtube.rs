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
use serde_json::Value as Json;
use serde_yaml::{Mapping, Value as Yaml};
use url::Url;

use crate::error::ClipError;
use crate::fetch::{FetchLimits, fetch_html};
use crate::note::resolve_tags;

/// The `source_type` value used for YouTube clips.
pub const SOURCE_TYPE_YOUTUBE: &str = "youtube";

/// Hosts recognized as YouTube.
const YOUTUBE_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
];

/// A single timestamped caption segment.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    /// Segment start, in seconds from the beginning of the video.
    pub start: f64,
    /// Segment end, in seconds. Preserved for the ADR 0018 `media_ts_end`.
    pub end: f64,
    /// Caption text, HTML-entity-decoded and tag-stripped.
    pub text: String,
}

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

    let title = json_str(&json, &["videoDetails", "title"]);
    let channel = json_str(&json, &["videoDetails", "author"]);
    let duration =
        json_str(&json, &["videoDetails", "lengthSeconds"]).and_then(|s| s.parse::<u64>().ok());
    let published = json_str(
        &json,
        &["microformat", "playerMicroformatRenderer", "publishDate"],
    )
    .or_else(|| {
        json_str(
            &json,
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

    Some(PlayerResponse {
        title,
        channel,
        published,
        duration,
        caption_tracks,
    })
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
/// Resilient: malformed or partial entries are skipped; a body with no parseable
/// `<text>` entries yields an empty vec (treated as "no captions" by callers).
pub fn parse_timedtext(xml: &str) -> Vec<TranscriptSegment> {
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
        segments.push(TranscriptSegment {
            start,
            end: start + dur,
            text,
        });
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

/// Format `seconds` as `H:MM:SS` (or `M:SS` under an hour) for transcript lines.
fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
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
    let meta = &transcript.meta;
    let (tag_values, _tag_strings) = resolve_tags(extra_tags);

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
    fm.insert(Yaml::from("ingested_at"), Yaml::from(now.to_rfc3339()));
    fm.insert(Yaml::from("tags"), Yaml::Sequence(tag_values));

    let yaml = serde_yaml::to_string(&Yaml::Mapping(fm))
        .unwrap_or_default()
        .trim_end()
        .to_string();

    let body = transcript
        .segments
        .iter()
        .map(|seg| format!("[{}] {}", format_timestamp(seg.start), seg.text))
        .collect::<Vec<_>>()
        .join("\n");

    format!("---\n{yaml}\n---\n\n{body}\n")
}

/// Fetch and normalize a YouTube video's *published caption track*.
///
/// Reuses the SSRF-guarded, bounded [`fetch_html`] for both the watch page and
/// the timedtext track. Returns [`YoutubeOutcome::NoCaptions`] (non-fatal) when
/// no usable caption track exists — the caller hands off to the Whisper worker.
/// This function never transcribes audio (ADR 0019 §4 / ADR 0020 §8.3).
pub async fn fetch_youtube(url: &str, limits: &FetchLimits) -> Result<YoutubeOutcome, ClipError> {
    let video_id = youtube_video_id(url)
        .ok_or_else(|| ClipError::InvalidUrl(format!("not a YouTube video URL: {url}")))?;
    let source_url = format!("https://www.youtube.com/watch?v={video_id}");

    let page = fetch_html(&source_url, limits).await?;
    let player = parse_player_response(&page.html)
        .ok_or_else(|| ClipError::Extract("could not parse YouTube player response".to_string()))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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

    #[tokio::test]
    async fn fetch_rejects_non_youtube_url() {
        let err = fetch_youtube("https://example.com/post", &FetchLimits::default())
            .await
            .unwrap_err();
        assert!(matches!(err, ClipError::InvalidUrl(_)));
    }
}
