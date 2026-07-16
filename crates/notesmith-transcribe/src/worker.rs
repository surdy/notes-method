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

use crate::queue::{QueueItem, SOURCE_TYPE_AUDIO, TranscriptionQueue};
use crate::{AudioInput, MediaMeta, Transcriber, default_transcriber, render_transcript_note};

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
    /// Items skipped because their source isn't handled yet (e.g. YouTube).
    pub skipped: usize,
    /// Vault-relative note paths written this pass.
    pub notes: Vec<String>,
}

/// Drains a [`TranscriptionQueue`] into timestamped notes under `notes_dir`.
pub struct TranscribeWorker {
    queue: TranscriptionQueue,
    transcriber: Box<dyn Transcriber>,
    notes_dir: PathBuf,
    max_attempts: i64,
    batch_size: usize,
}

impl TranscribeWorker {
    /// Build a worker with an explicit transcriber (used by tests to inject a
    /// stub) writing notes under `notes_dir`.
    pub fn new(
        queue: TranscriptionQueue,
        transcriber: Box<dyn Transcriber>,
        notes_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            queue,
            transcriber,
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
                    // Source not handled yet (e.g. YouTube audio acquisition is
                    // P2c); leave it pending, don't count as failure.
                    report.skipped += 1;
                }
                Err(reason) => {
                    tracing::warn!(
                        item = item.source_url,
                        stage = "transcribe",
                        reason = %reason,
                        "skipping transcription item",
                    );
                    // Best-effort failure record; a queue write error aborts the
                    // pass (the DB itself is unhealthy), but a per-item transcribe
                    // failure never does.
                    self.queue.mark_failed(item.id, &reason)?;
                    report.failed += 1;
                }
            }
        }

        Ok(report)
    }

    /// Process one item. `Ok(Some(rel))` = note written; `Ok(None)` = skipped
    /// (leave pending); `Err(reason)` = failed (retry next tick).
    fn process_item(&self, item: &QueueItem) -> Result<Option<String>, String> {
        if item.source_type != SOURCE_TYPE_AUDIO {
            return Ok(None);
        }
        let audio_path = item
            .audio_path
            .as_ref()
            .ok_or_else(|| "audio item has no audio_path".to_string())?;
        let audio_path = Path::new(audio_path);
        if !audio_path.is_file() {
            return Err(format!("audio file not found: {}", audio_path.display()));
        }

        let transcript = self
            .transcriber
            .transcribe(&AudioInput::Path(audio_path.to_path_buf()))
            .map_err(|e| e.to_string())?;

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
        };
        let note = render_transcript_note(&meta, &transcript, &[], Local::now());

        let rel = self.write_note(audio_path, item.id, &note)?;
        Ok(Some(rel))
    }

    /// Write `note` under `notes_dir`, returning its vault-relative path. The
    /// filename is a slug of the audio stem plus the queue id for uniqueness.
    fn write_note(&self, audio_path: &Path, id: i64, note: &str) -> Result<String, String> {
        std::fs::create_dir_all(&self.notes_dir).map_err(|e| format!("create notes dir: {e}"))?;
        let stem = audio_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
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
    use crate::{StubTranscriber, TranscriptSegment};

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
    fn youtube_items_are_skipped_not_failed() {
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
