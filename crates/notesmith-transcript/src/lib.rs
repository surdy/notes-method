//! Per-vault chat transcript persistence for the Notesmith daemon.
//!
//! Transcripts are stored in a **single durable SQLite database owned by the
//! daemon** (see ADR 0012 Decision 13). They live outside the vault — so they
//! neither clutter the notes nor get synced — and outside the rebuildable index
//! cache (`cache.sqlite`), which is dropped on schema bumps and reindex. Each
//! vault has its own revisitable chat history that survives daemon restarts;
//! ACP child sessions are re-established lazily when a thread is reopened.
//!
//! Per ADR 0009 (resilience to malformed content), reads degrade rather than
//! fail: a corrupt or partial row is skipped with a `WARN` and the remaining
//! rows are returned. No method panics on bad stored data.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

/// Errors returned by [`TranscriptStore`].
#[derive(Debug, thiserror::Error)]
pub enum TranscriptError {
    #[error("transcript store sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("transcript store io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("thread {thread_id} not found in vault {vault}")]
    ThreadNotFound { vault: String, thread_id: String },
}

/// Convenience alias for transcript-store results.
pub type Result<T> = std::result::Result<T, TranscriptError>;

/// Who authored a transcript message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The human using Notesmith.
    User,
    /// The AI agent (Copilot / Claude / Codex).
    Agent,
    /// System / context preamble injected by Notesmith.
    System,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Agent => "agent",
            Role::System => "system",
        }
    }

    /// Parse a stored role string. Unknown values yield `None` so callers can
    /// skip the row rather than fail (ADR 0009).
    fn parse(raw: &str) -> Option<Role> {
        match raw {
            "user" => Some(Role::User),
            "agent" => Some(Role::Agent),
            "system" => Some(Role::System),
            _ => None,
        }
    }
}

/// A chat thread (conversation) scoped to a single vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Thread {
    pub id: String,
    pub vault: String,
    pub title: String,
    /// Agent the thread was started with (e.g. `copilot`), if known.
    pub agent: Option<String>,
    /// Model selected for the thread, if known.
    pub model: Option<String>,
    /// The agent's ACP `sessionId` for this thread, once a session has been
    /// established and persisted (issue #262). Present only for threads whose
    /// agent (e.g. Copilot, Claude Code, Codex) has a disk-backed session that
    /// can be resumed via ACP `session/load` on reopen. `None` for brand-new or
    /// never-sent threads (an empty ACP session is never persisted agent-side).
    pub acp_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single message within a [`Thread`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Message {
    pub id: i64,
    pub thread_id: String,
    /// Monotonic per-thread ordering index (1-based).
    pub seq: i64,
    pub role: Role,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS threads (
    id             TEXT PRIMARY KEY,
    vault          TEXT NOT NULL,
    title          TEXT NOT NULL,
    agent          TEXT,
    model          TEXT,
    acp_session_id TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_threads_vault ON threads(vault, updated_at DESC);
CREATE TABLE IF NOT EXISTS messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id  TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id, seq);
";

/// Daemon-owned, per-vault transcript store backed by a single SQLite database.
///
/// All methods are vault-scoped: passing a `vault` only ever observes or mutates
/// that vault's threads, so vault A's history is invisible under vault B.
pub struct TranscriptStore {
    conn: Mutex<Connection>,
}

impl TranscriptStore {
    /// Open (creating if needed) the transcript database at `path`. Parent
    /// directories are created. The database is durable and survives restarts.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Open an ephemeral in-memory store, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        // Best-effort migration for stores created before `acp_session_id`
        // existed: `CREATE TABLE IF NOT EXISTS` above never alters an existing
        // table, so add the column here. A "duplicate column name" error means
        // the column is already present, which is fine to ignore.
        let _ = conn.execute("ALTER TABLE threads ADD COLUMN acp_session_id TEXT", []);
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock means another thread panicked while holding it; the
        // stored data is still valid, so recover the guard rather than crash.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Create a new thread in `vault`. `agent`/`model` are optional metadata.
    pub fn create_thread(
        &self,
        vault: &str,
        title: &str,
        agent: Option<&str>,
        model: Option<&str>,
    ) -> Result<Thread> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let conn = self.lock();
        conn.execute(
            "INSERT INTO threads (id, vault, title, agent, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            rusqlite::params![id, vault, title, agent, model, now_str],
        )?;
        Ok(Thread {
            id,
            vault: vault.to_string(),
            title: title.to_string(),
            agent: agent.map(str::to_string),
            model: model.map(str::to_string),
            acp_session_id: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// List `vault`'s threads, most-recently-updated first. Corrupt rows are
    /// skipped (ADR 0009).
    pub fn list_threads(&self, vault: &str) -> Result<Vec<Thread>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, vault, title, agent, model, acp_session_id, created_at, updated_at
             FROM threads WHERE vault = ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([vault], |row| Ok(row_to_thread(row)))?;
        let mut out = Vec::new();
        for row in rows {
            match row? {
                Some(thread) => out.push(thread),
                None => {
                    tracing::warn!(vault, stage = "list_threads", "skipping corrupt thread row")
                }
            }
        }
        Ok(out)
    }

    /// Fetch a single thread scoped to `vault`, or `None` if it does not exist
    /// (or belongs to another vault).
    pub fn get_thread(&self, vault: &str, thread_id: &str) -> Result<Option<Thread>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, vault, title, agent, model, acp_session_id, created_at, updated_at
             FROM threads WHERE vault = ?1 AND id = ?2",
        )?;
        let mut rows = stmt.query(rusqlite::params![vault, thread_id])?;
        match rows.next()? {
            Some(row) => Ok(row_to_thread(row)),
            None => Ok(None),
        }
    }

    /// Rename a thread scoped to `vault`. Returns `true` if a row was updated.
    pub fn rename_thread(&self, vault: &str, thread_id: &str, title: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock();
        let affected = conn.execute(
            "UPDATE threads SET title = ?3, updated_at = ?4 WHERE vault = ?1 AND id = ?2",
            rusqlite::params![vault, thread_id, title, now],
        )?;
        Ok(affected > 0)
    }

    /// Persist the agent's ACP `sessionId` for a thread scoped to `vault`, so it
    /// can be resumed via ACP `session/load` on reopen (issue #262). Passing
    /// `None` clears it. Does **not** bump `updated_at` (recording the session
    /// binding is not conversation activity). Returns `true` if a row matched.
    pub fn set_acp_session_id(
        &self,
        vault: &str,
        thread_id: &str,
        acp_session_id: Option<&str>,
    ) -> Result<bool> {
        let conn = self.lock();
        let affected = conn.execute(
            "UPDATE threads SET acp_session_id = ?3 WHERE vault = ?1 AND id = ?2",
            rusqlite::params![vault, thread_id, acp_session_id],
        )?;
        Ok(affected > 0)
    }

    /// Delete a thread (and its messages, via cascade) scoped to `vault`.
    /// Returns `true` if a row was deleted.
    pub fn delete_thread(&self, vault: &str, thread_id: &str) -> Result<bool> {
        let conn = self.lock();
        let affected = conn.execute(
            "DELETE FROM threads WHERE vault = ?1 AND id = ?2",
            rusqlite::params![vault, thread_id],
        )?;
        Ok(affected > 0)
    }

    /// Append a message to a thread scoped to `vault`, bumping the thread's
    /// `updated_at`. Errors with [`TranscriptError::ThreadNotFound`] if the
    /// thread does not belong to `vault`.
    pub fn append_message(
        &self,
        vault: &str,
        thread_id: &str,
        role: Role,
        content: &str,
    ) -> Result<Message> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let mut conn = self.lock();
        let tx = conn.transaction()?;

        let belongs: bool = tx
            .query_row(
                "SELECT 1 FROM threads WHERE id = ?1 AND vault = ?2",
                rusqlite::params![thread_id, vault],
                |_| Ok(()),
            )
            .is_ok();
        if !belongs {
            return Err(TranscriptError::ThreadNotFound {
                vault: vault.to_string(),
                thread_id: thread_id.to_string(),
            });
        }

        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE thread_id = ?1",
            [thread_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO messages (thread_id, seq, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![thread_id, seq, role.as_str(), content, now_str],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE threads SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![thread_id, now_str],
        )?;
        tx.commit()?;

        Ok(Message {
            id,
            thread_id: thread_id.to_string(),
            seq,
            role,
            content: content.to_string(),
            created_at: now,
        })
    }

    /// Load a thread's messages in order, scoped to `vault`. Returns an empty
    /// vector if the thread does not belong to `vault`. Corrupt rows are skipped
    /// (ADR 0009).
    pub fn load_messages(&self, vault: &str, thread_id: &str) -> Result<Vec<Message>> {
        let conn = self.lock();
        let belongs: bool = conn
            .query_row(
                "SELECT 1 FROM threads WHERE id = ?1 AND vault = ?2",
                rusqlite::params![thread_id, vault],
                |_| Ok(()),
            )
            .is_ok();
        if !belongs {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT id, thread_id, seq, role, content, created_at
             FROM messages WHERE thread_id = ?1 ORDER BY seq ASC, id ASC",
        )?;
        let rows = stmt.query_map([thread_id], |row| Ok(row_to_message(row)))?;
        let mut out = Vec::new();
        for row in rows {
            match row? {
                Some(msg) => out.push(msg),
                None => tracing::warn!(
                    vault,
                    thread_id,
                    stage = "load_messages",
                    "skipping corrupt message row"
                ),
            }
        }
        Ok(out)
    }
}

impl Default for TranscriptStore {
    /// An ephemeral in-memory store. Opening an in-memory SQLite connection does
    /// not perform I/O and never fails in practice.
    fn default() -> Self {
        Self::open_in_memory().expect("in-memory transcript store should always open")
    }
}

fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Build a [`Thread`] from a row, returning `None` if any field is malformed
/// (e.g. an unparseable timestamp). Never panics.
fn row_to_thread(row: &rusqlite::Row<'_>) -> Option<Thread> {
    let id: String = row.get("id").ok()?;
    let vault: String = row.get("vault").ok()?;
    let title: String = row.get("title").ok()?;
    let agent: Option<String> = row.get("agent").ok()?;
    let model: Option<String> = row.get("model").ok()?;
    let acp_session_id: Option<String> = row.get("acp_session_id").ok()?;
    let created_at = parse_ts(&row.get::<_, String>("created_at").ok()?)?;
    let updated_at = parse_ts(&row.get::<_, String>("updated_at").ok()?)?;
    Some(Thread {
        id,
        vault,
        title,
        agent,
        model,
        acp_session_id,
        created_at,
        updated_at,
    })
}

/// Build a [`Message`] from a row, returning `None` if any field is malformed
/// (unknown role, unparseable timestamp, non-UTF-8 content). Never panics.
fn row_to_message(row: &rusqlite::Row<'_>) -> Option<Message> {
    let id: i64 = row.get("id").ok()?;
    let thread_id: String = row.get("thread_id").ok()?;
    let seq: i64 = row.get("seq").ok()?;
    let role = Role::parse(&row.get::<_, String>("role").ok()?)?;
    let content: String = row.get("content").ok()?;
    let created_at = parse_ts(&row.get::<_, String>("created_at").ok()?)?;
    Some(Message {
        id,
        thread_id,
        seq,
        role,
        content,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> TranscriptStore {
        TranscriptStore::open_in_memory().expect("open in-memory store")
    }

    #[test]
    fn create_append_and_read_back() {
        let s = store();
        let t = s
            .create_thread("notes", "First chat", Some("copilot"), None)
            .unwrap();
        assert_eq!(t.vault, "notes");
        assert_eq!(t.agent.as_deref(), Some("copilot"));

        s.append_message("notes", &t.id, Role::User, "hello")
            .unwrap();
        s.append_message("notes", &t.id, Role::Agent, "hi there")
            .unwrap();

        let msgs = s.load_messages("notes", &t.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].seq, 1);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content, "hello");
        assert_eq!(msgs[1].seq, 2);
        assert_eq!(msgs[1].role, Role::Agent);
    }

    #[test]
    fn list_threads_orders_by_recent_update() {
        let s = store();
        let a = s.create_thread("v", "A", None, None).unwrap();
        let b = s.create_thread("v", "B", None, None).unwrap();
        // Touch A so it becomes the most recently updated.
        s.append_message("v", &a.id, Role::User, "x").unwrap();

        let listed = s.list_threads("v").unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, a.id);
        assert_eq!(listed[1].id, b.id);
    }

    #[test]
    fn threads_are_scoped_per_vault() {
        let s = store();
        let a = s.create_thread("vault-a", "secret", None, None).unwrap();
        s.append_message("vault-a", &a.id, Role::User, "private")
            .unwrap();

        // Vault B cannot see, fetch, load, or mutate vault A's thread.
        assert!(s.list_threads("vault-b").unwrap().is_empty());
        assert!(s.get_thread("vault-b", &a.id).unwrap().is_none());
        assert!(s.load_messages("vault-b", &a.id).unwrap().is_empty());
        assert!(!s.rename_thread("vault-b", &a.id, "x").unwrap());
        assert!(!s.delete_thread("vault-b", &a.id).unwrap());
        assert!(matches!(
            s.append_message("vault-b", &a.id, Role::User, "x"),
            Err(TranscriptError::ThreadNotFound { .. })
        ));

        // Vault A still intact.
        assert_eq!(s.list_threads("vault-a").unwrap().len(), 1);
        assert_eq!(s.load_messages("vault-a", &a.id).unwrap().len(), 1);
    }

    #[test]
    fn append_to_missing_thread_errors() {
        let s = store();
        let err = s.append_message("v", "does-not-exist", Role::User, "x");
        assert!(matches!(err, Err(TranscriptError::ThreadNotFound { .. })));
    }

    #[test]
    fn rename_and_delete_thread() {
        let s = store();
        let t = s.create_thread("v", "old", None, None).unwrap();
        assert!(s.rename_thread("v", &t.id, "new").unwrap());
        assert_eq!(s.get_thread("v", &t.id).unwrap().unwrap().title, "new");

        s.append_message("v", &t.id, Role::User, "x").unwrap();
        assert!(s.delete_thread("v", &t.id).unwrap());
        assert!(s.get_thread("v", &t.id).unwrap().is_none());
        // Cascade removed the messages.
        assert!(s.load_messages("v", &t.id).unwrap().is_empty());
    }

    #[test]
    fn corrupt_message_rows_are_skipped_not_fatal() {
        let s = store();
        let t = s.create_thread("v", "t", None, None).unwrap();
        s.append_message("v", &t.id, Role::User, "good").unwrap();

        // Inject malformed rows directly, bypassing the API: an unknown role and
        // an unparseable timestamp. These must be skipped, not panic or error.
        {
            let conn = s.lock();
            conn.execute(
                "INSERT INTO messages (thread_id, seq, role, content, created_at)
                 VALUES (?1, 2, 'wizard', 'bad role', ?2)",
                rusqlite::params![t.id, Utc::now().to_rfc3339()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO messages (thread_id, seq, role, content, created_at)
                 VALUES (?1, 3, 'user', 'bad ts', 'not-a-timestamp')",
                rusqlite::params![t.id],
            )
            .unwrap();
        }

        let msgs = s.load_messages("v", &t.id).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "good");
    }

    #[test]
    fn corrupt_thread_rows_are_skipped_not_fatal() {
        let s = store();
        s.create_thread("v", "ok", None, None).unwrap();
        {
            let conn = s.lock();
            conn.execute(
                "INSERT INTO threads (id, vault, title, created_at, updated_at)
                 VALUES ('bad', 'v', 'broken', 'nope', 'nope')",
                [],
            )
            .unwrap();
        }
        let listed = s.list_threads("v").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "ok");
        // Direct fetch of the corrupt row degrades to None rather than panicking.
        assert!(s.get_thread("v", "bad").unwrap().is_none());
    }

    #[test]
    fn acp_session_id_defaults_none_then_persists() {
        let s = store();
        let t = s.create_thread("v", "chat", Some("copilot"), None).unwrap();
        assert_eq!(t.acp_session_id, None);

        // Set it, and read it back via both get_thread and list_threads.
        assert!(s.set_acp_session_id("v", &t.id, Some("sess-abc")).unwrap());
        let fetched = s.get_thread("v", &t.id).unwrap().unwrap();
        assert_eq!(fetched.acp_session_id.as_deref(), Some("sess-abc"));
        let listed = s.list_threads("v").unwrap();
        assert_eq!(listed[0].acp_session_id.as_deref(), Some("sess-abc"));

        // Clearing it removes the binding.
        assert!(s.set_acp_session_id("v", &t.id, None).unwrap());
        assert_eq!(
            s.get_thread("v", &t.id).unwrap().unwrap().acp_session_id,
            None
        );
    }

    #[test]
    fn set_acp_session_id_is_vault_scoped_and_reports_miss() {
        let s = store();
        let t = s.create_thread("vault-a", "chat", None, None).unwrap();
        // Wrong vault does not match the row.
        assert!(!s.set_acp_session_id("vault-b", &t.id, Some("x")).unwrap());
        assert_eq!(
            s.get_thread("vault-a", &t.id)
                .unwrap()
                .unwrap()
                .acp_session_id,
            None
        );
        // Unknown thread id reports a miss.
        assert!(!s.set_acp_session_id("vault-a", "nope", Some("x")).unwrap());
    }

    #[test]
    fn migrates_pre_existing_db_without_acp_session_id_column() {
        // A store created before the column existed: build the legacy schema by
        // hand, then reopen through `from_connection` and confirm the migration
        // added the column (set/read round-trips).
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY, vault TEXT NOT NULL, title TEXT NOT NULL,
                agent TEXT, model TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, vault, title, created_at, updated_at)
             VALUES ('t1', 'v', 'legacy', ?1, ?1)",
            [Utc::now().to_rfc3339()],
        )
        .unwrap();

        let s = TranscriptStore::from_connection(conn).unwrap();
        let t = s.get_thread("v", "t1").unwrap().unwrap();
        assert_eq!(t.acp_session_id, None);
        assert!(
            s.set_acp_session_id("v", "t1", Some("sess-legacy"))
                .unwrap()
        );
        assert_eq!(
            s.get_thread("v", "t1")
                .unwrap()
                .unwrap()
                .acp_session_id
                .as_deref(),
            Some("sess-legacy")
        );
    }
}
