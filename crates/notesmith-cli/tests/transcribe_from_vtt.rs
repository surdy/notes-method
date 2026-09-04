//! `notesmith transcribe --from-vtt` — the entry point connectors use to render
//! an existing WebVTT transcript through the shared renderer (ADR 0025's
//! 2026-09-04 amendment).
//!
//! These drive the real binary because the mode's contract *is* CLI surface:
//! stdin piping (so transcript content never lands on disk), the merged
//! frontmatter a connector supplies, and the failure modes that must not
//! silently produce a note with no `event_id`.

use std::io::Write;
use std::process::{Command, Stdio};

/// Shaped after the real Teams sample captured in the 2026-09-04 spike: GUID
/// cue identifiers, `<v Speaker>` on every cue, and a deliberately overlapping
/// third cue.
const TEAMS_VTT: &str = "WEBVTT\n\n\
    7f3a-0001/1-0\n\
    00:00:03.447 --> 00:00:06.567\n\
    <v Alice Smith>Morning, shall we start?</v>\n\n\
    7f3a-0002/1-0\n\
    00:00:07.527 --> 00:00:26.727\n\
    <v Bob Jones>Yes. The renewal is the main thing.</v>\n\n\
    7f3a-0003/1-0\n\
    00:00:20.100 --> 00:00:31.000\n\
    <v Alice Smith>Agreed, and this cue overlaps the last.</v>\n";

fn notesmith_bin() -> String {
    std::env::var("CARGO_BIN_EXE_notesmith").unwrap()
}

/// Run the binary with `args`, piping `stdin_data` in. Returns (ok, stdout, stderr).
fn run(args: &[&str], stdin_data: Option<&str>) -> (bool, String, String) {
    let mut child = Command::new(notesmith_bin())
        .args(args)
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn notesmith");

    if let Some(data) = stdin_data {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(data.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn renders_a_teams_transcript_from_stdin_with_connector_frontmatter() {
    let (ok, stdout, stderr) = run(
        &[
            "transcribe",
            "--from-vtt",
            "-",
            "--title",
            "Acme Q3 sync",
            "--source",
            "teams:AAMkAGI2-0001",
            "--frontmatter",
            r#"{"event_id":"AAMkAGI2-0001","event":"[[2026-08-04 0930 Acme Q3 sync]]","customers":["[[Acme Corp]]"]}"#,
        ],
        Some(TEAMS_VTT),
    );
    assert!(ok, "stderr={stderr}");

    // ADR 0025's Transcript Note identity.
    assert!(stdout.contains("kind: transcript"), "{stdout}");
    assert!(stdout.contains("source_type: teams"), "{stdout}");
    assert!(stdout.contains("title: Acme Q3 sync"), "{stdout}");
    assert!(
        stdout.contains("source_url: teams:AAMkAGI2-0001"),
        "{stdout}"
    );

    // The join back to the meeting, supplied by the connector.
    assert!(stdout.contains("event_id: AAMkAGI2-0001"), "{stdout}");
    assert!(
        stdout.contains("event: '[[2026-08-04 0930 Acme Q3 sync]]'"),
        "{stdout}"
    );
    assert!(stdout.contains("- '[[Acme Corp]]'"), "{stdout}");

    // Speaker attribution through the shared body builder.
    assert!(
        stdout.contains("[0:03] Alice Smith: Morning, shall we start?"),
        "{stdout}"
    );
    assert!(
        stdout.contains("[0:07] Bob Jones: Yes. The renewal is the main thing."),
        "{stdout}"
    );

    // Duration is the maximum cue end, not the last cue's — cues overlap.
    assert!(stdout.contains("duration: 31"), "{stdout}");
}

/// The spike's specific warning: cue intervals overlap, so rendering must not
/// reorder them into a different conversation.
#[test]
fn preserves_cue_order_rather_than_sorting_by_time() {
    let (ok, stdout, stderr) = run(&["transcribe", "--from-vtt", "-"], Some(TEAMS_VTT));
    assert!(ok, "stderr={stderr}");

    let body: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with('['))
        .collect();
    assert_eq!(
        body,
        vec![
            "[0:03] Alice Smith: Morning, shall we start?",
            "[0:07] Bob Jones: Yes. The renewal is the main thing.",
            "[0:20] Alice Smith: Agreed, and this cue overlaps the last.",
        ]
    );
}

#[test]
fn json_output_reports_segment_and_speaker_counts() {
    let (ok, stdout, stderr) = run(
        &["--format", "json", "transcribe", "--from-vtt", "-"],
        Some(TEAMS_VTT),
    );
    assert!(ok, "stderr={stderr}");

    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(value["segments"], 3);
    assert_eq!(value["speakers"], 2, "Alice and Bob, deduplicated");
    assert_eq!(value["source"], "stdin");
    assert!(value["note"].as_str().unwrap().contains("kind: transcript"));

    // `body` is the transcript without the frontmatter fence — what a connector
    // POSTs alongside its own vault-model frontmatter.
    let body = value["body"].as_str().unwrap();
    assert!(
        !body.contains("kind: transcript"),
        "body must exclude frontmatter: {body}"
    );
    assert!(body.starts_with("[0:03] Alice Smith:"), "{body}");
    assert!(body.contains("[0:20] Alice Smith:"), "{body}");
}

#[test]
fn reads_a_file_and_defaults_the_title_to_its_stem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Acme Q3 sync.vtt");
    std::fs::write(&path, TEAMS_VTT).unwrap();
    let out_path = dir.path().join("note.md");

    let (ok, _stdout, stderr) = run(
        &[
            "transcribe",
            "--from-vtt",
            path.to_str().unwrap(),
            "--output",
            out_path.to_str().unwrap(),
        ],
        None,
    );
    assert!(ok, "stderr={stderr}");

    let note = std::fs::read_to_string(&out_path).unwrap();
    assert!(note.contains("title: Acme Q3 sync"), "{note}");
    assert!(note.contains("[0:03] Alice Smith:"), "{note}");
}

#[test]
fn source_type_is_overridable() {
    let (ok, stdout, stderr) = run(
        &["transcribe", "--from-vtt", "-", "--source-type", "webinar"],
        Some(TEAMS_VTT),
    );
    assert!(ok, "stderr={stderr}");
    assert!(stdout.contains("source_type: webinar"), "{stdout}");
}

/// A transcript with no cues must fail loudly. Rendering an empty note would
/// look like success and leave the meeting silently untranscribed.
#[test]
fn empty_vtt_is_an_error() {
    let (ok, _stdout, stderr) = run(&["transcribe", "--from-vtt", "-"], Some("WEBVTT\n\n"));
    assert!(!ok, "an empty transcript must not render a note");
    assert!(stderr.contains("no cues parsed"), "{stderr}");
}

/// A malformed `--frontmatter` must fail rather than drop `event_id` — a note
/// that renders without it can never be joined to its meeting.
#[test]
fn malformed_frontmatter_is_rejected() {
    for bad in [r#"["not","an","object"]"#, "just a string", "{unclosed:"] {
        let (ok, _stdout, stderr) = run(
            &["transcribe", "--from-vtt", "-", "--frontmatter", bad],
            Some(TEAMS_VTT),
        );
        assert!(!ok, "should have rejected --frontmatter {bad}");
        assert!(
            stderr.contains("--frontmatter"),
            "error should name the flag, got: {stderr}"
        );
    }
}

#[test]
fn a_missing_vtt_file_is_an_error() {
    let (ok, _stdout, stderr) = run(&["transcribe", "--from-vtt", "/nonexistent/none.vtt"], None);
    assert!(!ok);
    assert!(stderr.contains("VTT file not found"), "{stderr}");
}

/// A caption file with no speakers (YouTube, Whisper output re-rendered) must
/// still work — the speaker is optional, not required.
#[test]
fn a_transcript_without_speakers_renders_plain_lines() {
    let vtt = "WEBVTT\n\n00:00.000 --> 00:02.000\nplain caption\n";
    let (ok, stdout, stderr) = run(&["transcribe", "--from-vtt", "-"], Some(vtt));
    assert!(ok, "stderr={stderr}");
    assert!(stdout.contains("[0:00] plain caption"), "{stdout}");
    assert!(!stdout.contains(": plain caption"), "no stray separator");
}
