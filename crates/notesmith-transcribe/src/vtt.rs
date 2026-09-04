//! WebVTT → [`Transcript`] parsing (ADR 0025's 2026-09-04 amendment).
//!
//! Teams meeting transcripts arrive as WebVTT, one cue per utterance, each
//! carrying a `<v Speaker Name>` voice tag. This module turns that into the
//! same [`Transcript`] the Whisper and YouTube paths produce, so all three
//! render through [`crate::render_transcript_note`] — ADR 0025's "one
//! Transcript Note concept, regardless of origin".
//!
//! Deliberately lenient: a meeting transcript is worth having even if a few
//! cues are malformed. Anything unparseable is skipped, never fatal.

use crate::{Transcript, TranscriptSegment};

/// Parse a WebVTT document into a [`Transcript`].
///
/// **Source order is preserved.** Teams cue intervals can overlap (verified in
/// the 2026-09-04 spike against a real transcript), so sorting by start time
/// would interleave two speakers' turns into nonsense.
///
/// Tolerates: an absent `WEBVTT` header, cue identifier lines, `NOTE` and
/// `STYLE` blocks, `\r\n`, cue settings after the end timestamp, multi-line cue
/// payloads, and both `MM:SS.mmm` and `HH:MM:SS.mmm` timestamps. Cues whose
/// timing line will not parse, or whose payload is empty, are skipped.
pub fn parse_vtt(input: &str) -> Transcript {
    let mut segments = Vec::new();
    let mut pending: Vec<&str> = Vec::new();

    // Cues are separated by blank lines; a block is [identifier?] timing payload…
    for line in input.lines().chain(std::iter::once("")) {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            if let Some(segment) = parse_block(&pending) {
                segments.push(segment);
            }
            pending.clear();
        } else {
            pending.push(line);
        }
    }

    Transcript {
        language: None,
        segments,
    }
}

/// Turn one blank-line-delimited block into a segment, or `None`.
fn parse_block(lines: &[&str]) -> Option<TranscriptSegment> {
    if lines.is_empty() {
        return None;
    }
    // Header and metadata blocks carry no cue.
    let first = lines[0].trim();
    if first.starts_with("WEBVTT") || first.starts_with("NOTE") || first.starts_with("STYLE") {
        return None;
    }

    // The timing line is the first containing `-->`; anything before it is a
    // cue identifier, which Teams emits as a GUID and we have no use for.
    let timing_index = lines.iter().position(|line| line.contains("-->"))?;
    let (start, end) = parse_timing(lines[timing_index])?;

    let payload = lines[timing_index + 1..].join(" ");
    let (speaker, text) = split_voice_tag(&payload);
    let text = strip_tags(&text);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    Some(match speaker {
        Some(name) => TranscriptSegment::spoken(start, end, name, text),
        None => TranscriptSegment::new(start, end, text),
    })
}

/// `00:00:03.447 --> 00:00:06.567 align:start` → `(3.447, 6.567)`.
fn parse_timing(line: &str) -> Option<(f64, f64)> {
    let (left, right) = line.split_once("-->")?;
    let start = parse_timestamp(left.trim())?;
    // Cue settings (`align:start position:0%`) follow the end timestamp.
    let end_token = right.split_whitespace().next()?;
    let end = parse_timestamp(end_token)?;
    Some((start, end))
}

/// `HH:MM:SS.mmm` or `MM:SS.mmm` → seconds. Commas are accepted as the decimal
/// separator so an SRT-flavoured file does not silently lose every cue.
fn parse_timestamp(token: &str) -> Option<f64> {
    let token = token.trim().replace(',', ".");
    let mut seconds = 0.0_f64;
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    for part in &parts {
        let value: f64 = part.trim().parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        seconds = seconds * 60.0 + value;
    }
    Some(seconds)
}

/// Split a leading `<v Speaker Name>` voice tag off a cue payload.
///
/// Returns `(None, payload)` when there is no voice tag. A tag with an empty
/// name yields `None` rather than an empty speaker, so the line renders plain.
fn split_voice_tag(payload: &str) -> (Option<String>, String) {
    let trimmed = payload.trim_start();
    let Some(rest) = trimmed.strip_prefix("<v") else {
        return (None, payload.to_string());
    };
    let Some(close) = rest.find('>') else {
        return (None, payload.to_string());
    };
    // `<v.loud Name>` — any `.class` suffixes attach directly to the `v`, and
    // the speaker name is whatever follows the first whitespace.
    let name = rest[..close]
        .split_once(char::is_whitespace)
        .map(|(_classes, name)| name)
        .unwrap_or("")
        .trim()
        .to_string();
    let text = rest[close + 1..].to_string();
    let name = if name.is_empty() { None } else { Some(name) };
    (name, text)
}

/// Remove remaining inline VTT/HTML tags (`</v>`, `<b>`, `<c.colorE5E5E5>`).
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0_usize;
    for ch in text.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped after the real Teams sample captured in the 2026-09-04 spike:
    /// `WEBVTT`, GUID cue identifiers, `<v Speaker>` on every cue.
    const TEAMS: &str = "WEBVTT\n\n\
        b1e1-0001/1-0\n\
        00:00:03.447 --> 00:00:06.567\n\
        <v Alice Smith>Morning, shall we start?</v>\n\n\
        b1e1-0002/1-0\n\
        00:00:07.527 --> 00:00:26.727\n\
        <v Bob Jones>Yes. The renewal is the main thing.</v>\n";

    #[test]
    fn parses_teams_cues_with_speakers() {
        let transcript = parse_vtt(TEAMS);
        assert_eq!(transcript.segments.len(), 2);

        let first = &transcript.segments[0];
        assert_eq!(first.speaker.as_deref(), Some("Alice Smith"));
        assert_eq!(first.text, "Morning, shall we start?");
        assert!((first.start - 3.447).abs() < 1e-6, "{}", first.start);
        assert!((first.end - 6.567).abs() < 1e-6, "{}", first.end);

        assert_eq!(transcript.segments[1].speaker.as_deref(), Some("Bob Jones"));
        assert!((transcript.segments[1].start - 7.527).abs() < 1e-6);
    }

    #[test]
    fn renders_through_the_shared_body_builder() {
        let body = crate::transcript_body(&parse_vtt(TEAMS).segments);
        assert_eq!(
            body,
            "[0:03] Alice Smith: Morning, shall we start?\n\
             [0:07] Bob Jones: Yes. The renewal is the main thing."
        );
    }

    /// The property the spike specifically called out: cue intervals overlap,
    /// so parsing must not reorder them.
    #[test]
    fn preserves_source_order_across_overlapping_cues() {
        let vtt = "WEBVTT\n\n\
            00:00:10.000 --> 00:00:20.000\n<v A>first</v>\n\n\
            00:00:05.000 --> 00:00:15.000\n<v B>second</v>\n\n\
            00:00:12.000 --> 00:00:14.000\n<v A>third</v>\n";
        let transcript = parse_vtt(vtt);
        let texts: Vec<&str> = transcript
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(texts, vec!["first", "second", "third"]);
    }

    #[test]
    fn handles_hour_timestamps_and_cue_settings() {
        let vtt =
            "WEBVTT\n\n01:02:03.500 --> 01:02:09.000 align:start position:0%\n<v C>late</v>\n";
        let segments = parse_vtt(vtt).segments;
        assert_eq!(segments.len(), 1);
        assert!(
            (segments[0].start - 3723.5).abs() < 1e-6,
            "{}",
            segments[0].start
        );
        assert!((segments[0].end - 3729.0).abs() < 1e-6);
    }

    #[test]
    fn accepts_a_transcript_with_no_speakers() {
        let vtt = "WEBVTT\n\n00:00.000 --> 00:02.000\nplain caption\n";
        let segments = parse_vtt(vtt).segments;
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].speaker, None);
        assert_eq!(segments[0].text, "plain caption");
        assert_eq!(crate::transcript_body(&segments), "[0:00] plain caption");
    }

    #[test]
    fn joins_multi_line_payloads_and_strips_inline_tags() {
        let vtt = "WEBVTT\n\n00:00.000 --> 00:04.000\n<v Dana>one\ntwo <b>three</b></v>\n";
        let segments = parse_vtt(vtt).segments;
        assert_eq!(segments[0].text, "one two three");
        assert_eq!(segments[0].speaker.as_deref(), Some("Dana"));
    }

    #[test]
    fn voice_tag_classes_and_empty_names_degrade() {
        let vtt = "WEBVTT\n\n\
            00:00.000 --> 00:01.000\n<v.loud Eve>shouting</v>\n\n\
            00:01.000 --> 00:02.000\n<v >anonymous</v>\n";
        let segments = parse_vtt(vtt).segments;
        assert_eq!(segments[0].speaker.as_deref(), Some("Eve"));
        assert_eq!(segments[1].speaker, None, "an empty name is no attribution");
        assert_eq!(segments[1].text, "anonymous");
    }

    #[test]
    fn skips_metadata_blocks_and_malformed_cues() {
        let vtt = "WEBVTT - Some Title\n\n\
            NOTE this is a comment\nwith a second line\n\n\
            STYLE\n::cue { color: peachpuff; }\n\n\
            not-a-timing-line\nstray text\n\n\
            00:00.000 --> nonsense\n<v F>dropped</v>\n\n\
            00:05.000 --> 00:06.000\n<v G>kept</v>\n";
        let segments = parse_vtt(vtt).segments;
        assert_eq!(segments.len(), 1, "{segments:?}");
        assert_eq!(segments[0].text, "kept");
    }

    #[test]
    fn empty_and_whitespace_input_yield_no_segments() {
        assert!(parse_vtt("").segments.is_empty());
        assert!(parse_vtt("WEBVTT\n\n\n").segments.is_empty());
        assert!(
            parse_vtt("WEBVTT\n\n00:00.000 --> 00:01.000\n   \n")
                .segments
                .is_empty()
        );
    }

    #[test]
    fn accepts_crlf_and_srt_style_commas() {
        let vtt = "WEBVTT\r\n\r\n00:00:01,500 --> 00:00:02,500\r\n<v H>windows</v>\r\n";
        let segments = parse_vtt(vtt).segments;
        assert_eq!(segments.len(), 1);
        assert!((segments[0].start - 1.5).abs() < 1e-6);
        assert_eq!(segments[0].text, "windows");
    }
}
