//! YouTube transcript retrieval for the `youtube_transcript` MCP tool.
//!
//! This is a **thin wrapper** over the shared YouTube source module in
//! `notesmith-clip` ([ADR 0020](../../docs/adr/0020-web-clipper.md) §8.4): it
//! calls [`notesmith_clip::fetch_youtube`] and maps the outcome into a JSON
//! result. It never forks the fetch/parse logic and never transcribes audio —
//! videos without a published caption track return a clear, non-fatal
//! `no_captions` result (ADR 0019 §4 / ADR 0020 §8.3).
//!
//! The `Ops` trait is synchronous, but `fetch_youtube` is async and the MCP
//! `call_tool` handler is already async. Rather than block a runtime thread to
//! bridge async→sync inside a sync trait method, the retrieval is exposed as a
//! free async function that the async MCP dispatch awaits directly. The pure
//! [`youtube_outcome_to_value`] mapping is factored out so it can be unit-tested
//! without any network access.

use notesmith_clip::{
    FetchLimits, TranscriptSegment, YoutubeMeta, YoutubeOutcome, YoutubeTranscript, fetch_youtube,
};
use serde_json::{Value, json};

use crate::Result;

/// Fetch the published caption transcript for a YouTube `url`.
///
/// A thin wrapper over [`fetch_youtube`]: it performs the SSRF-guarded bounded
/// fetch and maps the [`YoutubeOutcome`] to a structured JSON value via
/// [`youtube_outcome_to_value`]. Invalid URLs, blocked targets, and fetch
/// failures surface as `Err` (which the MCP layer turns into a tool error);
/// videos without captions are a non-fatal `Ok` result.
pub async fn youtube_transcript(url: &str) -> Result<Value> {
    let outcome = fetch_youtube(url, &FetchLimits::default()).await?;
    Ok(youtube_outcome_to_value(&outcome))
}

/// Map a [`YoutubeOutcome`] to the `youtube_transcript` tool's JSON result.
///
/// Pure and network-free so it can be unit-tested with constructed values.
pub fn youtube_outcome_to_value(outcome: &YoutubeOutcome) -> Value {
    match outcome {
        YoutubeOutcome::Captions(transcript) => captions_value(transcript),
        YoutubeOutcome::NoCaptions(meta) => no_captions_value(meta),
    }
}

fn captions_value(transcript: &YoutubeTranscript) -> Value {
    let meta = &transcript.meta;
    let text = transcript
        .segments
        .iter()
        .map(|seg| format!("[{}] {}", format_timestamp(seg.start), seg.text))
        .collect::<Vec<_>>()
        .join("\n");
    let segments = transcript
        .segments
        .iter()
        .map(segment_value)
        .collect::<Vec<_>>();
    json!({
        "status": "captions",
        "source_url": meta.source_url,
        "video_id": meta.video_id,
        "title": meta.title,
        "channel": meta.channel,
        "published": meta.published,
        "duration": meta.duration,
        "text": text,
        "segments": segments,
    })
}

fn no_captions_value(meta: &YoutubeMeta) -> Value {
    json!({
        "status": "no_captions",
        "source_url": meta.source_url,
        "video_id": meta.video_id,
        "message": "no published captions available",
    })
}

fn segment_value(seg: &TranscriptSegment) -> Value {
    json!({
        "start": seg.start,
        "end": seg.end,
        "text": seg.text,
    })
}

/// Format a start offset in seconds as `m:ss` (or `h:mm:ss` past an hour).
fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> YoutubeMeta {
        YoutubeMeta {
            video_id: "dQw4w9WgXcQ".to_string(),
            source_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            title: Some("Test Title".to_string()),
            channel: Some("Test Channel".to_string()),
            published: Some("2009-10-25".to_string()),
            duration: Some(213),
        }
    }

    #[test]
    fn captions_outcome_maps_text_segments_and_metadata() {
        let transcript = YoutubeTranscript {
            meta: meta(),
            segments: vec![
                TranscriptSegment {
                    start: 0.0,
                    end: 2.5,
                    text: "Never gonna give you up".to_string(),
                },
                TranscriptSegment {
                    start: 65.0,
                    end: 67.0,
                    text: "Never gonna let you down".to_string(),
                },
            ],
        };
        let value = youtube_outcome_to_value(&YoutubeOutcome::Captions(transcript));

        assert_eq!(value["status"], "captions");
        assert_eq!(
            value["source_url"],
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(value["video_id"], "dQw4w9WgXcQ");
        assert_eq!(value["title"], "Test Title");
        assert_eq!(value["channel"], "Test Channel");
        assert_eq!(value["published"], "2009-10-25");
        assert_eq!(value["duration"], 213);

        let text = value["text"].as_str().unwrap();
        assert_eq!(
            text,
            "[0:00] Never gonna give you up\n[1:05] Never gonna let you down"
        );

        let segments = value["segments"].as_array().unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0]["start"], 0.0);
        assert_eq!(segments[0]["end"], 2.5);
        assert_eq!(segments[0]["text"], "Never gonna give you up");
        assert_eq!(segments[1]["start"], 65.0);
    }

    #[test]
    fn no_captions_outcome_is_non_fatal() {
        let value = youtube_outcome_to_value(&YoutubeOutcome::NoCaptions(meta()));

        assert_eq!(value["status"], "no_captions");
        assert_eq!(
            value["source_url"],
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(value["video_id"], "dQw4w9WgXcQ");
        assert_eq!(value["message"], "no published captions available");
        assert!(value.get("segments").is_none());
    }

    #[test]
    fn timestamps_include_hours_past_an_hour() {
        assert_eq!(format_timestamp(0.0), "0:00");
        assert_eq!(format_timestamp(9.0), "0:09");
        assert_eq!(format_timestamp(65.0), "1:05");
        assert_eq!(format_timestamp(3661.0), "1:01:01");
    }
}
