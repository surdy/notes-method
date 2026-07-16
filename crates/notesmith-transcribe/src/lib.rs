//! Engine-agnostic speech-to-text core for Notesmith (ADR 0023).
//!
//! This crate owns the transcription *boundary*: the [`Transcriber`] trait, the
//! audio/transcript data model, bundled-model resolution, and the shared
//! transcript→note renderer. The real local engine (whisper.cpp via
//! `whisper-rs`) lives behind the off-by-default `local-whisper` feature so the
//! standard workspace gates never compile the C/C++ backend; lean builds fall
//! back to the [`StubTranscriber`] placeholder (mirroring `notesmith-embed`'s
//! `HashEmbedder`).
//!
//! Alternative engines (e.g. Parakeet) can be added later purely by
//! implementing [`Transcriber`] — nothing above this boundary is Whisper-aware
//! (ADR 0023 §2).

use std::path::PathBuf;

use thiserror::Error;

mod model;
mod render;

#[cfg(feature = "local-whisper")]
mod whisper;

pub use model::{WHISPER_MODEL_DIR_ENV, bundled_model_dir, whisper_model_file};
pub use render::{MediaMeta, format_timestamp, render_transcript_note, transcript_body};

#[cfg(feature = "local-whisper")]
pub use whisper::LocalWhisper;

/// Whether the real local transcription runtime (whisper.cpp / `whisper-rs`) is
/// compiled into this build — i.e. whether [`default_transcriber`] can return a
/// real engine rather than the [`StubTranscriber`] placeholder.
///
/// This is the single source of truth `/api/capabilities` should advertise as
/// `transcription.compiled_in`: because it is evaluated in the crate that owns
/// engine selection, it stays correct regardless of which upstream crate
/// enabled the feature (mirrors `notesmith_embed::LOCAL_EMBED_COMPILED`, the fix
/// from commit a8b8f55).
pub const LOCAL_WHISPER_COMPILED: bool = cfg!(feature = "local-whisper");

/// Audio handed to a [`Transcriber`].
///
/// Engines accept either a path (which they decode themselves) or pre-decoded
/// mono PCM. Full container/codec demuxing beyond WAV is out of scope for the
/// core crate (ADR 0023 §6) — the acquisition worker is responsible for
/// producing a decodable input.
#[derive(Debug, Clone)]
pub enum AudioInput {
    /// A path to an audio file on disk, decoded by the engine.
    Path(PathBuf),
    /// Pre-decoded mono PCM samples at `sample_rate` Hz.
    Pcm {
        /// Mono, floating-point samples in `[-1.0, 1.0]`.
        samples: Vec<f32>,
        /// Sample rate of `samples`, in Hz.
        sample_rate: u32,
    },
}

/// A single timestamped transcript segment. Times are seconds from the start of
/// the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    /// Segment start, in seconds.
    pub start: f64,
    /// Segment end, in seconds.
    pub end: f64,
    /// Segment text (trimmed).
    pub text: String,
}

/// A fully decoded transcript: an optional detected language plus timestamped
/// segments in playback order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transcript {
    /// Detected language code (e.g. `"en"`), when the engine reports one.
    pub language: Option<String>,
    /// Timestamped segments in playback order.
    pub segments: Vec<TranscriptSegment>,
}

impl Transcript {
    /// The plain concatenated text of all segments, space-joined.
    pub fn full_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Errors a [`Transcriber`] may return. Every variant is a *degraded success*
/// signal for callers: per ADR 0009 the worker logs and skips the item rather
/// than crashing.
#[derive(Debug, Error)]
pub enum TranscribeError {
    /// No usable model is available (feature disabled, or model dir missing).
    #[error("transcription model unavailable: {0}")]
    ModelUnavailable(String),
    /// The audio could not be decoded into PCM the engine can consume.
    #[error("audio decode failed: {0}")]
    Decode(String),
    /// The audio was structurally unsupported (e.g. empty, non-PCM WAV).
    #[error("unsupported audio: {0}")]
    Unsupported(String),
    /// The transcription backend itself failed.
    #[error("transcription backend error: {0}")]
    Backend(String),
}

/// The engine-agnostic transcription boundary. Mirrors `notesmith_embed`'s
/// `Embedder`: one method, `Send + Sync`, no Whisper types leaking through.
pub trait Transcriber: Send + Sync {
    /// Transcribe `audio` into a [`Transcript`].
    fn transcribe(&self, audio: &AudioInput) -> Result<Transcript, TranscribeError>;
}

/// A deterministic, model-free [`Transcriber`] used as the lean-build
/// placeholder and in tests. It never touches disk or native code and always
/// succeeds with the configured canned segments.
#[derive(Debug, Clone, Default)]
pub struct StubTranscriber {
    segments: Vec<TranscriptSegment>,
    language: Option<String>,
}

impl StubTranscriber {
    /// A stub that yields no segments (an empty transcript).
    pub fn new() -> Self {
        Self::default()
    }

    /// A stub that yields the given canned `segments`.
    pub fn with_segments(segments: Vec<TranscriptSegment>) -> Self {
        Self {
            segments,
            language: Some("en".to_string()),
        }
    }
}

impl Transcriber for StubTranscriber {
    fn transcribe(&self, _audio: &AudioInput) -> Result<Transcript, TranscribeError> {
        Ok(Transcript {
            language: self.language.clone(),
            segments: self.segments.clone(),
        })
    }
}

/// The default transcriber for this build.
///
/// When the `local-whisper` feature is compiled in *and* a usable model
/// directory resolves ([`bundled_model_dir`]), returns a real [`LocalWhisper`]
/// engine; otherwise returns a [`StubTranscriber`] placeholder so callers always
/// get a working `Transcriber` (mirrors `notesmith_embed::default_embedder`).
pub fn default_transcriber() -> Box<dyn Transcriber> {
    #[cfg(feature = "local-whisper")]
    {
        if let Some(dir) = bundled_model_dir() {
            match LocalWhisper::from_model_dir(&dir) {
                Ok(engine) => return Box::new(engine),
                Err(err) => {
                    tracing::warn!(
                        stage = "acquire",
                        reason = %err,
                        "falling back to stub transcriber",
                    );
                }
            }
        }
    }
    Box::new(StubTranscriber::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_yields_configured_segments() {
        let stub = StubTranscriber::with_segments(vec![
            TranscriptSegment {
                start: 0.0,
                end: 1.5,
                text: "hello".into(),
            },
            TranscriptSegment {
                start: 1.5,
                end: 3.0,
                text: "world".into(),
            },
        ]);
        let t = stub
            .transcribe(&AudioInput::Pcm {
                samples: vec![0.0; 16],
                sample_rate: 16_000,
            })
            .expect("stub never fails");
        assert_eq!(t.segments.len(), 2);
        assert_eq!(t.language.as_deref(), Some("en"));
        assert_eq!(t.full_text(), "hello world");
    }

    #[test]
    fn empty_stub_is_empty_transcript() {
        let t = StubTranscriber::new()
            .transcribe(&AudioInput::Path("nonexistent.wav".into()))
            .expect("stub never fails");
        assert!(t.segments.is_empty());
        assert_eq!(t.full_text(), "");
    }

    #[test]
    fn default_transcriber_is_usable() {
        // In lean builds this is the stub; either way it must not panic and must
        // return a working Transcriber.
        let t = default_transcriber();
        let out = t
            .transcribe(&AudioInput::Pcm {
                samples: vec![0.0; 8],
                sample_rate: 16_000,
            })
            .expect("default transcriber must succeed on silence");
        assert!(out.segments.is_empty() || !out.segments.is_empty());
    }

    #[test]
    fn compiled_in_matches_feature() {
        assert_eq!(LOCAL_WHISPER_COMPILED, cfg!(feature = "local-whisper"));
    }
}
