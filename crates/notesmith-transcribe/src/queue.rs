//! Pending-transcription queue (`transcribe.db`) — ADR 0023 §5.
//!
//! A small SQLite table owned by the transcription worker's domain (never the
//! daemon's note index, preserving the ADR 0012 sole-index-owner invariant).
//! The daemon only appends intent rows (`enqueue`); the colocated worker drains
//! them (`claim`), transcribes, and records the outcome (`mark_done` /
//! `mark_failed`). Identity/dedup is the canonical `source_url`.

use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

use crate::TranscribeError;

type Result<T> = std::result::Result<T, TranscribeError>;

/// `source_type` for a local audio file (worker reads it directly, no network).
pub const SOURCE_TYPE_AUDIO: &str = "audio";
/// `source_type` for a YouTube video whose audio the worker acquires (P2c).
pub const SOURCE_TYPE_YOUTUBE: &str = "youtube";

/// Schema version for `transcribe.db`. Bump on any incompatible change; the
/// store drops and recreates on mismatch (queue state is transient work).
const SCHEMA_VERSION: i64 = 1;

fn map_sql<E: std::fmt::Display>(e: E) -> TranscribeError {
    TranscribeError::Queue(e.to_string())
}

/// Lifecycle status of a queue row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueStatus {
    /// Not yet processed.
    Pending,
    /// Successfully transcribed into a note.
    Done,
    /// Last attempt failed; retried next tick while under the attempt cap.
    Failed,
}

impl QueueStatus {
    fn as_str(self) -> &'static str {
        match self {
            QueueStatus::Pending => "pending",
            QueueStatus::Done => "done",
            QueueStatus::Failed => "failed",
        }
    }

    fn from_str(s: &str) -> QueueStatus {
        match s {
            "done" => QueueStatus::Done,
            "failed" => QueueStatus::Failed,
            _ => QueueStatus::Pending,
        }
    }
}

/// An intent row to append via [`TranscriptionQueue::enqueue`].
#[derive(Debug, Clone)]
pub struct NewQueueEntry {
    /// Canonical dedup key (a URL, or a `file://`-style path for local audio).
    pub source_url: String,
    /// `SOURCE_TYPE_AUDIO` or `SOURCE_TYPE_YOUTUBE`.
    pub source_type: String,
    /// Local audio path for `audio` items; `None` for items acquired later.
    pub audio_path: Option<String>,
    /// JSON provenance blob (title/channel/etc.); `"{}"` when unknown.
    pub meta_json: String,
}

impl NewQueueEntry {
    /// A local-audio entry keyed by its file path.
    pub fn local_audio(source_url: impl Into<String>, audio_path: impl Into<String>) -> Self {
        Self {
            source_url: source_url.into(),
            source_type: SOURCE_TYPE_AUDIO.to_string(),
            audio_path: Some(audio_path.into()),
            meta_json: "{}".to_string(),
        }
    }
}

/// A claimed queue row handed to the worker.
#[derive(Debug, Clone)]
pub struct QueueItem {
    /// Row id.
    pub id: i64,
    /// Canonical dedup key.
    pub source_url: String,
    /// Source type.
    pub source_type: String,
    /// Local audio path, when known.
    pub audio_path: Option<String>,
    /// JSON provenance blob.
    pub meta_json: String,
    /// Current status.
    pub status: QueueStatus,
    /// Number of failed attempts so far.
    pub attempts: i64,
    /// Written note path, once done.
    pub note_path: Option<String>,
    /// Last error message, when failed.
    pub last_error: Option<String>,
}

/// Whether [`TranscriptionQueue::enqueue`] created a new row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// A new intent row was appended.
    Inserted,
    /// An entry for this `source_url` already existed (idempotent no-op).
    Existed,
}

/// A handle to a per-vault transcription queue database.
pub struct TranscriptionQueue {
    conn: Mutex<Connection>,
}

impl TranscriptionQueue {
    /// Open (creating if needed) the queue and ensure the schema is current.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| TranscribeError::Io(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(map_sql)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(map_sql)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(map_sql)?;
        let queue = Self {
            conn: Mutex::new(conn),
        };
        queue.ensure_schema()?;
        Ok(queue)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self.conn.lock().expect("queue mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(map_sql)?;
        let current: Option<i64> = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM _meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sql)?;
        if current != Some(SCHEMA_VERSION) {
            conn.execute_batch("DROP TABLE IF EXISTS pending_transcription;")
                .map_err(map_sql)?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_transcription (
                id           INTEGER PRIMARY KEY,
                source_url   TEXT NOT NULL UNIQUE,
                source_type  TEXT NOT NULL,
                audio_path   TEXT,
                meta_json    TEXT NOT NULL DEFAULT '{}',
                status       TEXT NOT NULL DEFAULT 'pending',
                attempts     INTEGER NOT NULL DEFAULT 0,
                last_error   TEXT,
                note_path    TEXT,
                created_at   TEXT NOT NULL,
                updated_at   TEXT NOT NULL
            );",
        )
        .map_err(map_sql)?;
        conn.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION],
        )
        .map_err(map_sql)?;
        Ok(())
    }

    /// Append an intent row, keyed by `source_url`. Idempotent: a second enqueue
    /// of the same `source_url` is a no-op ([`EnqueueOutcome::Existed`]).
    pub fn enqueue(&self, entry: &NewQueueEntry) -> Result<EnqueueOutcome> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().expect("queue mutex poisoned");
        let changed = conn
            .execute(
                "INSERT INTO pending_transcription
                    (source_url, source_type, audio_path, meta_json, status,
                     attempts, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?5)
                 ON CONFLICT(source_url) DO NOTHING",
                params![
                    entry.source_url,
                    entry.source_type,
                    entry.audio_path,
                    entry.meta_json,
                    now,
                ],
            )
            .map_err(map_sql)?;
        Ok(if changed > 0 {
            EnqueueOutcome::Inserted
        } else {
            EnqueueOutcome::Existed
        })
    }

    /// Claim up to `limit` items to process: everything `pending`, plus `failed`
    /// items still under `max_attempts` (retried next tick, ADR 0023 §4).
    pub fn claim(&self, limit: usize, max_attempts: i64) -> Result<Vec<QueueItem>> {
        let conn = self.conn.lock().expect("queue mutex poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT id, source_url, source_type, audio_path, meta_json,
                        status, attempts, note_path, last_error
                 FROM pending_transcription
                 WHERE status = 'pending'
                    OR (status = 'failed' AND attempts < ?1)
                 ORDER BY id ASC
                 LIMIT ?2",
            )
            .map_err(map_sql)?;
        let rows = stmt
            .query_map(params![max_attempts, limit as i64], |row| {
                Ok(QueueItem {
                    id: row.get(0)?,
                    source_url: row.get(1)?,
                    source_type: row.get(2)?,
                    audio_path: row.get(3)?,
                    meta_json: row.get(4)?,
                    status: QueueStatus::from_str(&row.get::<_, String>(5)?),
                    attempts: row.get(6)?,
                    note_path: row.get(7)?,
                    last_error: row.get(8)?,
                })
            })
            .map_err(map_sql)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql)
    }

    /// Mark an item transcribed, recording the written `note_path`.
    pub fn mark_done(&self, id: i64, note_path: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().expect("queue mutex poisoned");
        conn.execute(
            "UPDATE pending_transcription
             SET status = 'done', note_path = ?2, last_error = NULL, updated_at = ?3
             WHERE id = ?1",
            params![id, note_path, now],
        )
        .map_err(map_sql)?;
        Ok(())
    }

    /// Mark an item failed, incrementing its attempt count and recording `error`.
    pub fn mark_failed(&self, id: i64, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().expect("queue mutex poisoned");
        conn.execute(
            "UPDATE pending_transcription
             SET status = 'failed', attempts = attempts + 1,
                 last_error = ?2, updated_at = ?3
             WHERE id = ?1",
            params![id, error, now],
        )
        .map_err(map_sql)?;
        Ok(())
    }

    /// Count rows in a given status (observability / tests).
    pub fn count(&self, status: QueueStatus) -> Result<i64> {
        let conn = self.conn.lock().expect("queue mutex poisoned");
        conn.query_row(
            "SELECT COUNT(*) FROM pending_transcription WHERE status = ?1",
            params![status.as_str()],
            |row| row.get(0),
        )
        .map_err(map_sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_queue() -> (tempfile::TempDir, TranscriptionQueue) {
        let dir = tempfile::tempdir().unwrap();
        let q = TranscriptionQueue::open(&dir.path().join("transcribe.db")).unwrap();
        (dir, q)
    }

    #[test]
    fn enqueue_is_idempotent_by_source_url() {
        let (_d, q) = temp_queue();
        let entry = NewQueueEntry::local_audio("file:///a.wav", "/a.wav");
        assert_eq!(q.enqueue(&entry).unwrap(), EnqueueOutcome::Inserted);
        assert_eq!(q.enqueue(&entry).unwrap(), EnqueueOutcome::Existed);
        assert_eq!(q.count(QueueStatus::Pending).unwrap(), 1);
    }

    #[test]
    fn claim_returns_pending_items() {
        let (_d, q) = temp_queue();
        q.enqueue(&NewQueueEntry::local_audio("file:///a.wav", "/a.wav"))
            .unwrap();
        q.enqueue(&NewQueueEntry::local_audio("file:///b.wav", "/b.wav"))
            .unwrap();
        let items = q.claim(10, 3).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].source_url, "file:///a.wav");
    }

    #[test]
    fn mark_done_removes_from_pending_and_records_note() {
        let (_d, q) = temp_queue();
        q.enqueue(&NewQueueEntry::local_audio("file:///a.wav", "/a.wav"))
            .unwrap();
        let item = q.claim(1, 3).unwrap().pop().unwrap();
        q.mark_done(item.id, "transcribed/a.md").unwrap();
        assert_eq!(q.count(QueueStatus::Pending).unwrap(), 0);
        assert_eq!(q.count(QueueStatus::Done).unwrap(), 1);
        // A done item is not re-claimed.
        assert!(q.claim(10, 3).unwrap().is_empty());
    }

    #[test]
    fn failed_items_are_retried_until_attempt_cap() {
        let (_d, q) = temp_queue();
        q.enqueue(&NewQueueEntry::local_audio("file:///a.wav", "/a.wav"))
            .unwrap();
        // Fail it twice; with max_attempts=3 it is still claimable.
        let item = q.claim(1, 3).unwrap().pop().unwrap();
        q.mark_failed(item.id, "boom").unwrap();
        q.mark_failed(item.id, "boom").unwrap();
        let retried = q.claim(10, 3).unwrap();
        assert_eq!(retried.len(), 1);
        assert_eq!(retried[0].attempts, 2);
        // Third failure hits the cap → no longer claimed.
        q.mark_failed(item.id, "boom").unwrap();
        assert!(q.claim(10, 3).unwrap().is_empty());
        assert_eq!(q.count(QueueStatus::Failed).unwrap(), 1);
    }
}
