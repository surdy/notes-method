//! The transcription worker — drains the pending queue into notes (ADR 0023
//! §4/§7). Mirrors `notesmith-embed`'s `EmbedWorker`: a colocated CLI worker,
//! never the daemon.
//!
//! Each claimed item is processed in isolation (ADR 0009 / ADR 0023 §8): a
//! failure logs `WARN` and marks the item failed (retried next tick) without
//! aborting the batch or panicking. Local-audio items are transcribed and
//! written as timestamped notes; YouTube items are left pending because their
//! audio acquisition is P2c (not yet implemented).

use std::path::{Path, PathBuf};

use chrono::Local;

use crate::queue::{QueueItem, SOURCE_TYPE_AUDIO, SOURCE_TYPE_YOUTUBE, TranscriptionQueue};
use crate::{
    AudioAcquirer, AudioInput, MediaMeta, Transcriber, default_transcriber, render_transcript_note,
};

/// Default cap on failed-item retries before an item is abandoned.
pub const DEFAULT_MAX_ATTEMPTS: i64 = 5;
/// Default number of items processed per `run`.
pub const DEFAULT_BATCH_SIZE: usize = 16;

/// Outcome of one worker pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscribeReport {
    /// Items transcribed into notes this pass.
    pub transcribed: usize,
    /// Items whose processing failed (marked for retry).
    pub failed: usize,
    /// Items skipped because their source isn't handled yet (e.g. YouTube with
    /// no audio acquirer wired in).
    pub skipped: usize,
    /// Vault-relative note paths written this pass.
    pub notes: Vec<String>,
}

/// Drains a [`TranscriptionQueue`] into timestamped notes under `notes_dir`.
pub struct TranscribeWorker {
    queue: TranscriptionQueue,
    transcriber: Box<dyn Transcriber>,
    acquirer: Option<Box<dyn AudioAcquirer>>,
    notes_dir: PathBuf,
    max_attempts: i64,
    batch_size: usize,
}

impl TranscribeWorker {
    /// Build a worker with an explicit transcriber (used by tests to inject a
    /// stub) writing notes under `notes_dir`. No audio acquirer is wired in, so
    /// YouTube items are skipped; use [`Self::with_acquirer`] to add one.
    pub fn new(
        queue: TranscriptionQueue,
        transcriber: Box<dyn Transcriber>,
        notes_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            queue,
            transcriber,
            acquirer: None,
            notes_dir: notes_dir.into(),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    /// Build a worker using the build's [`default_transcriber`].
    pub fn with_default_transcriber(
        queue: TranscriptionQueue,
        notes_dir: impl Into<PathBuf>,
    ) -> Self {
        Self::new(queue, default_transcriber(), notes_dir)
    }

    /// Attach an [`AudioAcquirer`] so YouTube items are downloaded, decoded, and
    /// transcribed rather than skipped (ADR 0023 §6). Consuming builder.
    pub fn with_acquirer(mut self, acquirer: Box<dyn AudioAcquirer>) -> Self {
        self.acquirer = Some(acquirer);
        self
    }

    /// Run one incremental pass over the queue.
    pub fn run(&self) -> Result<TranscribeReport, crate::TranscribeError> {
        let items = self.queue.claim(self.batch_size, self.max_attempts)?;
        let mut report = TranscribeReport::default();

        for item in items {
            match self.process_item(&item) {
                Ok(Some(note_rel)) => {
                    self.queue.mark_done(item.id, &note_rel)?;
                    report.transcribed += 1;
                    report.notes.push(note_rel);
                }
                Ok(None) => {
                    // Source not handled in this build (e.g. a YouTube item with
                    // no audio acquirer wired in); leave it pending, don't count
                    // as failure.
                    report.skipped += 1;
                }
                Err(reason) => {
                    tracing::warn!(
                        item = item.source_url,
                        stage = %reason.stage,
                        reason = %reason.message,
                        "skipping transcription item",
                    );
                    // Best-effort failure record; a queue write error aborts the
                    // pass (the DB itself is unhealthy), but a per-item failure
                    // never does.
                    self.queue.mark_failed(item.id, &reason.message)?;
                    report.failed += 1;
                }
            }
        }

        Ok(report)
    }

    /// Process one item. `Ok(Some(rel))` = note written; `Ok(None)` = skipped
    /// (leave pending); `Err(reason)` = failed (retry next tick).
    fn process_item(&self, item: &QueueItem) -> Result<Option<String>, ItemError> {
        match item.source_type.as_str() {
            SOURCE_TYPE_AUDIO => self.process_local_audio(item),
            SOURCE_TYPE_YOUTUBE => self.process_youtube(item),
            // Unknown source types are left pending rather than failed, so a
            // future producer can be added without abandoning its rows.
            _ => Ok(None),
        }
    }

    /// Local-audio item: read the file directly and transcribe (no network).
    fn process_local_audio(&self, item: &QueueItem) -> Result<Option<String>, ItemError> {
        let audio_path = item
            .audio_path
            .as_ref()
            .ok_or_else(|| ItemError::new("decode", "audio item has no audio_path"))?;
        let audio_path = Path::new(audio_path);
        if !audio_path.is_file() {
            return Err(ItemError::new(
                "acquire",
                format!("audio file not found: {}", audio_path.display()),
            ));
        }

        let transcript = self
            .transcriber
            .transcribe(&AudioInput::Path(audio_path.to_path_buf()))
            .map_err(|e| ItemError::new("transcribe", e.to_string()))?;

        let title = audio_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let meta = MediaMeta {
            title,
            source: item.source_url.clone(),
            source_type: SOURCE_TYPE_AUDIO.to_string(),
            duration: transcript.segments.last().map(|s| s.end.max(0.0) as u64),
            channel: None,
            published: None,
        };
        let note = render_transcript_note(&meta, &transcript, &[], Local::now());

        let stem = audio_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let rel = self
            .write_note(stem, item.id, &note)
            .map_err(|e| ItemError::new("normalize", e))?;
        Ok(Some(rel))
    }

    /// YouTube item: acquire audio via the wired-in [`AudioAcquirer`]
    /// (download and decode, ADR 0023 §6), transcribe, and render a
    /// `source_type: youtube` note. With no acquirer compiled/wired in, the item
    /// is skipped (left pending) so a build without audio support does no harm.
    fn process_youtube(&self, item: &QueueItem) -> Result<Option<String>, ItemError> {
        let Some(acquirer) = &self.acquirer else {
            return Ok(None);
        };

        let acquired = acquirer
            .acquire_youtube(&item.source_url)
            .map_err(|e| ItemError::new("acquire", e.to_string()))?;

        let transcript = self
            .transcriber
            .transcribe(&acquired.audio)
            .map_err(|e| ItemError::new("transcribe", e.to_string()))?;

        // Prefer acquired provenance; fall back to the queued meta_json title.
        let queued = parse_meta_json(&item.meta_json);
        let title = acquired
            .title
            .or_else(|| queued.title.clone())
            .unwrap_or_else(|| item.source_url.clone());
        let duration = acquired
            .duration
            .or(queued.duration)
            .or_else(|| transcript.segments.last().map(|s| s.end.max(0.0) as u64));
        let meta = MediaMeta {
            title,
            source: item.source_url.clone(),
            source_type: SOURCE_TYPE_YOUTUBE.to_string(),
            duration,
            channel: acquired.channel.or(queued.channel),
            published: acquired.published.or(queued.published),
        };
        let note = render_transcript_note(&meta, &transcript, &[], Local::now());

        let stem = queued
            .video_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("youtube");
        let rel = self
            .write_note(stem, item.id, &note)
            .map_err(|e| ItemError::new("normalize", e))?;
        Ok(Some(rel))
    }

    /// Write `note` under `notes_dir`, returning its vault-relative path. The
    /// filename is a slug of `stem` plus the queue id for uniqueness (the queue
    /// itself dedups repeated work by canonical `source_url`).
    fn write_note(&self, stem: &str, id: i64, note: &str) -> Result<String, String> {
        std::fs::create_dir_all(&self.notes_dir).map_err(|e| format!("create notes dir: {e}"))?;
        let filename = format!("{}-{id}.md", slugify(stem));
        let full = self.notes_dir.join(&filename);
        std::fs::write(&full, note).map_err(|e| format!("write note: {e}"))?;
        Ok(format!(
            "{}/{}",
            self.notes_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("transcribed"),
            filename
        ))
    }
}

/// A per-item failure with the pipeline `stage` it occurred in (ADR 0023 §8:
/// `acquire | decode | transcribe | normalize`), for structured `WARN` logging.
struct ItemError {
    stage: &'static str,
    message: String,
}

impl ItemError {
    fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

/// Minimal provenance decoded from a queue row's `meta_json` (best-effort; a
/// malformed blob yields all-`None`, never an error — ADR 0009).
#[derive(Debug, Default)]
struct QueuedMeta {
    title: Option<String>,
    channel: Option<String>,
    published: Option<String>,
    duration: Option<u64>,
    video_id: Option<String>,
}

fn parse_meta_json(meta_json: &str) -> QueuedMeta {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(meta_json) else {
        return QueuedMeta::default();
    };
    let string = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    QueuedMeta {
        title: string("title"),
        channel: string("channel"),
        published: string("published"),
        duration: value.get("duration").and_then(|v| v.as_u64()),
        video_id: string("video_id"),
    }
}

/// Lowercase, collapse non-alphanumerics to single hyphens, trim hyphens.
fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "audio".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::{NewQueueEntry, QueueStatus};
    use crate::{AcquiredAudio, StubTranscriber, TranscribeError, TranscriptSegment};

    fn stub_with_text() -> Box<dyn Transcriber> {
        Box::new(StubTranscriber::with_segments(vec![
            TranscriptSegment {
                start: 0.0,
                end: 2.0,
                text: "hello world".into(),
            },
            TranscriptSegment {
                start: 65.0,
                end: 70.0,
                text: "second line".into(),
            },
        ]))
    }

    /// A mock acquirer that returns canned audio + provenance, so the worker's
    /// YouTube path is exercised without any network (ADR 0023 acceptance:
    /// "(mocked) audio → note").
    struct MockAcquirer {
        title: Option<String>,
        channel: Option<String>,
    }

    impl AudioAcquirer for MockAcquirer {
        fn acquire_youtube(&self, _source_url: &str) -> Result<AcquiredAudio, TranscribeError> {
            Ok(AcquiredAudio {
                audio: AudioInput::Pcm {
                    samples: vec![0.0; 16],
                    sample_rate: 16_000,
                },
                title: self.title.clone(),
                channel: self.channel.clone(),
                published: Some("2025-01-01".into()),
                duration: Some(90),
            })
        }
    }

    /// An acquirer that always fails, to exercise the acquire-stage failure path.
    struct FailingAcquirer;

    impl AudioAcquirer for FailingAcquirer {
        fn acquire_youtube(&self, _source_url: &str) -> Result<AcquiredAudio, TranscribeError> {
            Err(TranscribeError::Decode("stream fetch failed".into()))
        }
    }

    #[test]
    fn slugify_normalizes() {
        assert_eq!(slugify("My Meeting 2026!!"), "my-meeting-2026");
        assert_eq!(slugify("   "), "audio");
    }

    #[test]
    fn drains_local_audio_into_note() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("meeting.wav");
        std::fs::write(&audio, b"not really audio, stub ignores it").unwrap();

        let queue = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        queue
            .enqueue(&NewQueueEntry::local_audio(
                "file:///meeting.wav",
                audio.to_str().unwrap(),
            ))
            .unwrap();

        let notes_dir = dir.path().join("transcribed");
        let worker = TranscribeWorker::new(queue, stub_with_text(), &notes_dir);
        let report = worker.run().unwrap();

        assert_eq!(report.transcribed, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(report.notes.len(), 1);

        let written = std::fs::read_dir(&notes_dir).unwrap().count();
        assert_eq!(written, 1);
        let note_path = notes_dir.join("meeting-1.md");
        let body = std::fs::read_to_string(&note_path).unwrap();
        assert!(body.contains("source_type: audio"));
        assert!(body.contains("[0:00] hello world"));
        assert!(body.contains("[1:05] second line"));
    }

    #[test]
    fn missing_audio_file_fails_and_retries() {
        let dir = tempfile::tempdir().unwrap();
        let queue = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        queue
            .enqueue(&NewQueueEntry::local_audio(
                "file:///gone.wav",
                "/does/not/exist.wav",
            ))
            .unwrap();

        let worker = TranscribeWorker::new(queue, stub_with_text(), dir.path().join("transcribed"));
        let report = worker.run().unwrap();
        assert_eq!(report.transcribed, 0);
        assert_eq!(report.failed, 1);

        // Re-run: the failed item is retried (still under the attempt cap).
        let report2 = worker.run().unwrap();
        assert_eq!(report2.failed, 1);
    }

    #[test]
    fn youtube_items_are_skipped_when_no_acquirer() {
        let dir = tempfile::tempdir().unwrap();
        let queue = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        queue
            .enqueue(&NewQueueEntry {
                source_url: "https://youtube.com/watch?v=abc".into(),
                source_type: crate::queue::SOURCE_TYPE_YOUTUBE.into(),
                audio_path: None,
                meta_json: "{}".into(),
            })
            .unwrap();

        let worker = TranscribeWorker::new(queue, stub_with_text(), dir.path().join("transcribed"));
        let report = worker.run().unwrap();
        assert_eq!(report.transcribed, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.skipped, 1);
    }

    #[test]
    fn youtube_item_with_acquirer_becomes_note() {
        let dir = tempfile::tempdir().unwrap();
        let queue = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        queue
            .enqueue(&NewQueueEntry {
                source_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
                source_type: crate::queue::SOURCE_TYPE_YOUTUBE.into(),
                audio_path: None,
                meta_json: r#"{"video_id":"dQw4w9WgXcQ","title":"queued title"}"#.into(),
            })
            .unwrap();

        let notes_dir = dir.path().join("transcribed");
        let worker = TranscribeWorker::new(queue, stub_with_text(), &notes_dir).with_acquirer(
            Box::new(MockAcquirer {
                title: Some("Acquired Title".into()),
                channel: Some("Rick Astley".into()),
            }),
        );
        let report = worker.run().unwrap();

        assert_eq!(report.transcribed, 1);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.notes.len(), 1);

        // Filename uses the video id from meta_json + queue id.
        let note_path = notes_dir.join("dqw4w9wgxcq-1.md");
        let body = std::fs::read_to_string(&note_path).unwrap();
        assert!(body.contains("source_type: youtube"));
        assert!(body.contains("source_url: https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        // Acquired provenance wins over the queued title.
        assert!(body.contains("title: Acquired Title"));
        assert!(body.contains("channel: Rick Astley"));
        assert!(body.contains("[0:00] hello world"));
        assert!(body.contains("[1:05] second line"));
    }

    #[test]
    fn youtube_acquire_failure_fails_and_retries() {
        let dir = tempfile::tempdir().unwrap();
        let queue = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        queue
            .enqueue(&NewQueueEntry {
                source_url: "https://www.youtube.com/watch?v=abc".into(),
                source_type: crate::queue::SOURCE_TYPE_YOUTUBE.into(),
                audio_path: None,
                meta_json: "{}".into(),
            })
            .unwrap();

        let worker = TranscribeWorker::new(queue, stub_with_text(), dir.path().join("transcribed"))
            .with_acquirer(Box::new(FailingAcquirer));
        let report = worker.run().unwrap();
        assert_eq!(report.transcribed, 0);
        assert_eq!(report.failed, 1);
        assert_eq!(report.skipped, 0);

        // The failed item is retried next tick (still under the attempt cap).
        let report2 = worker.run().unwrap();
        assert_eq!(report2.failed, 1);
    }

    #[test]
    fn done_items_are_not_reprocessed() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("a.wav");
        std::fs::write(&audio, b"x").unwrap();
        let queue = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        queue
            .enqueue(&NewQueueEntry::local_audio(
                "file:///a.wav",
                audio.to_str().unwrap(),
            ))
            .unwrap();

        let worker = TranscribeWorker::new(queue, stub_with_text(), dir.path().join("transcribed"));
        assert_eq!(worker.run().unwrap().transcribed, 1);
        // Second pass: nothing left pending.
        let report = worker.run().unwrap();
        assert_eq!(report.transcribed, 0);
        assert_eq!(report.skipped, 0);
        assert_eq!(worker.queue.count(QueueStatus::Done).unwrap(), 1);
    }
}
