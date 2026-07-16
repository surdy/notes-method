//! Shared transcript→note rendering (ADR 0023 §7).
//!
//! Owns the timestamp formatter and timestamped-body builder that both this
//! crate's local-audio notes and `notesmith-clip`'s YouTube notes render from,
//! plus a generic media-provenance note renderer (ADR 0019 §3) used by the
//! `notesmith transcribe` CLI.

use chrono::{DateTime, Local};
use serde_yaml::{Mapping, Value};

use crate::{Transcript, TranscriptSegment};

/// Format `seconds` as `H:MM:SS` (or `M:SS` under an hour) for transcript lines.
pub fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The timestamped transcript body (`[M:SS] text` per non-empty segment).
pub fn transcript_body(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .filter(|seg| !seg.text.trim().is_empty())
        .map(|seg| format!("[{}] {}", format_timestamp(seg.start), seg.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Provenance for a transcribed media file, rendered into ADR 0019 §3
/// frontmatter.
#[derive(Debug, Clone)]
pub struct MediaMeta {
    /// Note title (defaults to the source when empty).
    pub title: String,
    /// Source identifier — a file path or URL (the dedup key).
    pub source: String,
    /// `source_type` frontmatter value (e.g. `"audio"`).
    pub source_type: String,
    /// Media duration in seconds, when known.
    pub duration: Option<u64>,
    /// Channel / author name, when known (e.g. a YouTube channel).
    pub channel: Option<String>,
    /// Publish date string as reported by the source, when known.
    pub published: Option<String>,
}

/// Render a [`Transcript`] into a Markdown note with media-provenance
/// frontmatter (ADR 0019 §3) and a timestamped transcript body.
///
/// `extra_tags` are appended after the mandatory `inbox` tag. `now` controls the
/// `ingested_at` timestamp. The transcript's detected `language`, when present,
/// is recorded as a `language` frontmatter key.
pub fn render_transcript_note(
    meta: &MediaMeta,
    transcript: &Transcript,
    extra_tags: &[String],
    now: DateTime<Local>,
) -> String {
    let ingested_at = now.to_rfc3339();
    let fm = media_frontmatter(meta, transcript, &ingested_at, extra_tags);
    let body = transcript_body(&transcript.segments);

    let yaml = serde_yaml::to_string(&Value::Mapping(fm))
        .unwrap_or_default()
        .trim_end()
        .to_string();

    format!("---\n{yaml}\n---\n\n{body}\n")
}

fn media_frontmatter(
    meta: &MediaMeta,
    transcript: &Transcript,
    ingested_at: &str,
    extra_tags: &[String],
) -> Mapping {
    let mut fm = Mapping::new();
    let title = if meta.title.trim().is_empty() {
        meta.source.clone()
    } else {
        meta.title.clone()
    };
    fm.insert(Value::from("title"), Value::from(title));
    fm.insert(Value::from("source_url"), Value::from(meta.source.clone()));
    fm.insert(
        Value::from("source_type"),
        Value::from(meta.source_type.clone()),
    );
    if let Some(channel) = &meta.channel {
        if !channel.trim().is_empty() {
            fm.insert(Value::from("channel"), Value::from(channel.clone()));
        }
    }
    if let Some(published) = &meta.published {
        if !published.trim().is_empty() {
            fm.insert(Value::from("published"), Value::from(published.clone()));
        }
    }
    if let Some(duration) = meta.duration {
        fm.insert(Value::from("duration"), Value::from(duration));
    }
    if let Some(language) = &transcript.language {
        fm.insert(Value::from("language"), Value::from(language.clone()));
    }
    fm.insert(Value::from("ingested_at"), Value::from(ingested_at));

    let mut tags: Vec<Value> = vec![Value::from("inbox")];
    for tag in extra_tags {
        let tag = tag.trim();
        if !tag.is_empty() && tag != "inbox" {
            tags.push(Value::from(tag));
        }
    }
    fm.insert(Value::from("tags"), Value::Sequence(tags));
    fm
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn timestamp_formatting() {
        assert_eq!(format_timestamp(0.0), "0:00");
        assert_eq!(format_timestamp(9.4), "0:09");
        assert_eq!(format_timestamp(65.0), "1:05");
        assert_eq!(format_timestamp(3661.0), "1:01:01");
        assert_eq!(format_timestamp(-5.0), "0:00");
    }

    #[test]
    fn body_skips_empty_segments_and_trims() {
        let segs = vec![
            TranscriptSegment {
                start: 0.0,
                end: 2.0,
                text: "  hello  ".into(),
            },
            TranscriptSegment {
                start: 2.0,
                end: 3.0,
                text: "   ".into(),
            },
            TranscriptSegment {
                start: 65.0,
                end: 70.0,
                text: "world".into(),
            },
        ];
        assert_eq!(transcript_body(&segs), "[0:00] hello\n[1:05] world");
    }

    #[test]
    fn renders_media_frontmatter_and_timestamped_body() {
        let meta = MediaMeta {
            title: "Interview".into(),
            source: "/drop/interview.wav".into(),
            source_type: "audio".into(),
            duration: Some(125),
            channel: None,
            published: None,
        };
        let transcript = Transcript {
            language: Some("en".into()),
            segments: vec![TranscriptSegment {
                start: 0.0,
                end: 2.0,
                text: "hello there".into(),
            }],
        };
        let now = Local.with_ymd_and_hms(2025, 1, 2, 3, 4, 5).unwrap();
        let note = render_transcript_note(&meta, &transcript, &["voice".into()], now);

        assert!(note.starts_with("---\n"));
        assert!(note.contains("title: Interview"));
        assert!(note.contains("source_url: /drop/interview.wav"));
        assert!(note.contains("source_type: audio"));
        assert!(note.contains("duration: 125"));
        assert!(note.contains("language: en"));
        assert!(note.contains("- inbox"));
        assert!(note.contains("- voice"));
        assert!(note.contains("[0:00] hello there"));
    }

    #[test]
    fn title_defaults_to_source_when_empty() {
        let meta = MediaMeta {
            title: "  ".into(),
            source: "/drop/clip.wav".into(),
            source_type: "audio".into(),
            duration: None,
            channel: None,
            published: None,
        };
        let now = Local.with_ymd_and_hms(2025, 1, 2, 3, 4, 5).unwrap();
        let note = render_transcript_note(&meta, &Transcript::default(), &[], now);
        assert!(note.contains("title: /drop/clip.wav"));
    }

    #[test]
    fn renders_channel_and_published_when_present() {
        let meta = MediaMeta {
            title: "Talk".into(),
            source: "https://www.youtube.com/watch?v=abc".into(),
            source_type: "youtube".into(),
            duration: Some(600),
            channel: Some("Some Channel".into()),
            published: Some("2025-01-01".into()),
        };
        let now = Local.with_ymd_and_hms(2025, 1, 2, 3, 4, 5).unwrap();
        let note = render_transcript_note(&meta, &Transcript::default(), &[], now);
        assert!(note.contains("source_type: youtube"));
        assert!(note.contains("channel: Some Channel"));
        assert!(note.contains("published:") && note.contains("2025-01-01"));
    }
}
