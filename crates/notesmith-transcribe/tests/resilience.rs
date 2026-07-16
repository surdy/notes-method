//! Resilience tests (ADR 0009 / ADR 0023 §8): pathological audio must never
//! panic and must complete in bounded time. These run on the lean build against
//! the [`StubTranscriber`] and the public model-resolution API; the whisper.cpp
//! decode path has its own feature-gated unit tests.

use std::time::Instant;

use notesmith_transcribe::{
    AudioInput, MediaMeta, StubTranscriber, Transcriber, Transcript, TranscriptSegment,
    render_transcript_note, transcript_body, whisper_model_file,
};

fn pathological_inputs() -> Vec<AudioInput> {
    vec![
        // Empty PCM.
        AudioInput::Pcm {
            samples: vec![],
            sample_rate: 16_000,
        },
        // Zero sample rate.
        AudioInput::Pcm {
            samples: vec![0.0; 8],
            sample_rate: 0,
        },
        // Non-finite samples.
        AudioInput::Pcm {
            samples: vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
            sample_rate: 44_100,
        },
        // Nonexistent path.
        AudioInput::Path("/does/not/exist.wav".into()),
        // A path that is a directory, not a file.
        AudioInput::Path("/".into()),
    ]
}

#[test]
fn stub_never_panics_on_pathological_audio() {
    let stub = StubTranscriber::new();
    let start = Instant::now();
    for input in pathological_inputs() {
        // Must not panic; stub always succeeds with an empty transcript.
        let out = stub.transcribe(&input).expect("stub never fails");
        assert!(out.segments.is_empty());
    }
    assert!(
        start.elapsed().as_secs() < 5,
        "must complete in bounded time"
    );
}

#[test]
fn model_resolution_on_nonexistent_dir_is_none_not_panic() {
    assert!(whisper_model_file(std::path::Path::new("/does/not/exist")).is_none());
}

#[test]
fn renderer_handles_degenerate_transcripts() {
    let meta = MediaMeta {
        title: String::new(),
        source: "x".into(),
        source_type: "audio".into(),
        duration: None,
    };
    // Empty transcript renders valid frontmatter + empty body, no panic.
    let note = render_transcript_note(&meta, &Transcript::default(), &[], chrono::Local::now());
    assert!(note.starts_with("---\n"));

    // Whitespace-only / non-finite-timestamp segments do not panic.
    let weird = vec![
        TranscriptSegment {
            start: f64::NAN,
            end: f64::INFINITY,
            text: "   ".into(),
        },
        TranscriptSegment {
            start: -1.0,
            end: 0.0,
            text: "kept".into(),
        },
    ];
    let body = transcript_body(&weird);
    assert!(body.contains("kept"));
    assert!(!body.contains("   "));
}
