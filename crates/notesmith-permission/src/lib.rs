//! Daemon-owned persistence for "Always Allow" agent write grants (issue #189).
//!
//! Issue #189 gives the per-write permission prompt (ADR 0012 Decision 5) three
//! tiers:
//!
//! 1. **Allow Once** — allow this single call, remember nothing.
//! 2. **Allow This Session** — allow + remember in the in-memory session
//!    permission state (suppresses re-prompts this session only).
//! 3. **Always Allow** — allow + remember in the session state *and* persist
//!    here, so a future session — even after a daemon/app restart — is
//!    pre-seeded and never re-prompts.
//!
//! Like the chat [`TranscriptStore`](notesmith_transcript) (ADR 0012 Decision
//! 13), persisted grants live in a **single durable SQLite database owned by the
//! daemon**, outside any vault and outside the rebuildable index cache. Every
//! method is **vault-scoped**: a grant made under vault A is invisible under
//! vault B. Grants are keyed by `(vault, tool)`.
//!
//! Per ADR 0009 (resilience to malformed content) nothing here panics on bad
//! stored data or a locked/corrupt database: reads degrade to "not granted" /
//! an empty list and mutations surface a typed error rather than crashing the
//! daemon.

use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::Connection;

/// Errors returned by [`PermissionGrantStore`].
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("permission store sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("permission store io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience alias for permission-store results.
pub type Result<T> = std::result::Result<T, PermissionError>;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS grants (
    vault      TEXT NOT NULL,
    tool       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (vault, tool)
);
CREATE INDEX IF NOT EXISTS idx_grants_vault ON grants(vault, tool);
";

/// Daemon-owned, per-vault store of persisted "Always Allow" tool grants backed
/// by a single SQLite database.
///
/// All methods are vault-scoped: passing a `vault` only ever observes or mutates
/// that vault's grants, so vault A's grants are invisible under vault B.
pub struct PermissionGrantStore {
    conn: Mutex<Connection>,
}

impl PermissionGrantStore {
    /// Open (creating if needed) the grant database at `path`. Parent
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
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock means another thread panicked while holding it; the
        // stored data is still valid, so recover the guard rather than crash.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Persist an "Always Allow" grant for `tool` in `vault`. Idempotent: a
    /// repeated grant is a no-op (the original `created_at` is preserved).
    pub fn grant(&self, vault: &str, tool: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock();
        conn.execute(
            "INSERT OR IGNORE INTO grants (vault, tool, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![vault, tool, now],
        )?;
        Ok(())
    }

    /// Remove a persisted grant for `tool` in `vault`. Returns `true` if a row
    /// was deleted. Revoking an absent grant is not an error.
    pub fn revoke(&self, vault: &str, tool: &str) -> Result<bool> {
        let conn = self.lock();
        let affected = conn.execute(
            "DELETE FROM grants WHERE vault = ?1 AND tool = ?2",
            rusqlite::params![vault, tool],
        )?;
        Ok(affected > 0)
    }

    /// Whether `tool` has a persisted grant in `vault`.
    pub fn is_granted(&self, vault: &str, tool: &str) -> bool {
        let conn = self.lock();
        conn.query_row(
            "SELECT 1 FROM grants WHERE vault = ?1 AND tool = ?2",
            rusqlite::params![vault, tool],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// List `vault`'s persisted grants, tool names sorted ascending. Corrupt
    /// rows are skipped (ADR 0009); an unknown vault yields an empty vector.
    pub fn list_granted(&self, vault: &str) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT tool FROM grants WHERE vault = ?1 ORDER BY tool ASC")?;
        let rows = stmt.query_map([vault], |row| Ok(row.get::<_, String>("tool").ok()))?;
        let mut out = Vec::new();
        for row in rows {
            match row? {
                Some(tool) => out.push(tool),
                None => tracing::warn!(vault, stage = "list_granted", "skipping corrupt grant row"),
            }
        }
        Ok(out)
    }
}

impl Default for PermissionGrantStore {
    /// An ephemeral in-memory store. Opening an in-memory SQLite connection does
    /// not perform I/O and never fails in practice.
    fn default() -> Self {
        Self::open_in_memory().expect("in-memory permission store should always open")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> PermissionGrantStore {
        PermissionGrantStore::open_in_memory().expect("open in-memory store")
    }

    #[test]
    fn grant_then_is_granted_and_list() {
        let s = store();
        assert!(!s.is_granted("notes", "create_note"));
        s.grant("notes", "create_note").unwrap();
        s.grant("notes", "append_note").unwrap();
        assert!(s.is_granted("notes", "create_note"));
        assert_eq!(
            s.list_granted("notes").unwrap(),
            vec!["append_note".to_string(), "create_note".to_string()]
        );
    }

    #[test]
    fn revoke_removes_a_grant() {
        let s = store();
        s.grant("notes", "create_note").unwrap();
        assert!(s.revoke("notes", "create_note").unwrap());
        assert!(!s.is_granted("notes", "create_note"));
        assert!(s.list_granted("notes").unwrap().is_empty());
        // Revoking an absent grant is a no-op, not an error.
        assert!(!s.revoke("notes", "create_note").unwrap());
    }

    #[test]
    fn grant_is_idempotent() {
        let s = store();
        s.grant("notes", "create_note").unwrap();
        s.grant("notes", "create_note").unwrap();
        assert_eq!(s.list_granted("notes").unwrap(), vec!["create_note"]);
    }

    #[test]
    fn grants_are_scoped_per_vault() {
        let s = store();
        s.grant("vault-a", "create_note").unwrap();
        assert!(s.is_granted("vault-a", "create_note"));
        // Vault B sees nothing of vault A's grant.
        assert!(!s.is_granted("vault-b", "create_note"));
        assert!(s.list_granted("vault-b").unwrap().is_empty());
        // Revoking from the wrong vault leaves vault A's grant intact.
        assert!(!s.revoke("vault-b", "create_note").unwrap());
        assert!(s.is_granted("vault-a", "create_note"));
    }

    #[test]
    fn unknown_vault_lists_empty() {
        let s = store();
        assert!(s.list_granted("never-seen").unwrap().is_empty());
    }

    #[test]
    fn grants_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-permissions.sqlite");
        {
            let s = PermissionGrantStore::open(&path).unwrap();
            s.grant("notes", "create_note").unwrap();
        }
        // Reopen the same file: the grant persists across the drop/reopen.
        let s = PermissionGrantStore::open(&path).unwrap();
        assert!(s.is_granted("notes", "create_note"));
        assert_eq!(s.list_granted("notes").unwrap(), vec!["create_note"]);
    }

    #[test]
    fn corrupt_grant_rows_are_skipped_not_fatal() {
        let s = store();
        s.grant("v", "good").unwrap();
        // Inject a malformed row directly (non-text tool): listing must skip it
        // rather than panic or error (ADR 0009).
        {
            let conn = s.lock();
            conn.execute(
                "INSERT INTO grants (vault, tool, created_at) VALUES ('v', X'00ff', 'now')",
                [],
            )
            .unwrap();
        }
        let listed = s.list_granted("v").unwrap();
        assert_eq!(listed, vec!["good"]);
    }
}
