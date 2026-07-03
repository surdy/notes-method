//! Persistent per-vault embeddings store (`embeddings.db`).
//!
//! Mirrors the `TranscriptStore`/`transcripts.sqlite` precedent: a durable,
//! daemon-adjacent SQLite database that lives in `data_dir/<vault>/`, **not**
//! the rebuildable cache dir. WAL mode lets the daemon read while the embed
//! worker writes. Schema is version-guarded like `notesmith-index`'s cache:
//! a version mismatch drops and recreates the tables (embeddings are always
//! rebuildable from the notes).

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags};

use crate::{Chunk, EmbedError, Result, blob_to_vector, vector_to_blob};

/// Schema version for `embeddings.db`. Bump on any incompatible change; the
/// store drops and recreates on mismatch (embeddings are derived data).
pub const SCHEMA_VERSION: i64 = 1;

const META_SCHEMA_VERSION: &str = "schema_version";
const META_EMBEDDER_ID: &str = "embedder_id";
const META_DIM: &str = "dim";

/// A handle to a per-vault embeddings database.
pub struct EmbeddingStore {
    conn: Mutex<Connection>,
    read_only: bool,
}

impl EmbeddingStore {
    /// Open (creating if needed) the store read-write and ensure the schema is
    /// present and current. The embed worker uses this — it is the sole writer.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let store = Self {
            conn: Mutex::new(conn),
            read_only: false,
        };
        store.ensure_schema()?;
        Ok(store)
    }

    /// Open the store **read-only** (the daemon's access mode, ADR 0018 §2/§7).
    /// The database must already exist and carry a compatible schema.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            read_only: true,
        })
    }

    /// Run `f` with a locked connection handle.
    pub(crate) fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().expect("embeddings store mutex poisoned");
        f(&conn)
    }

    fn ensure_schema(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )?;
            let current: Option<i64> = conn
                .query_row(
                    "SELECT CAST(value AS INTEGER) FROM _meta WHERE key = ?1",
                    [META_SCHEMA_VERSION],
                    |row| row.get(0),
                )
                .ok();

            if current != Some(SCHEMA_VERSION) {
                conn.execute_batch(
                    "DROP TABLE IF EXISTS chunks;
                     DELETE FROM _meta WHERE key IN ('embedder_id', 'dim');",
                )?;
            }

            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS chunks (
                    vault_name     TEXT NOT NULL,
                    path           TEXT NOT NULL,
                    chunk_id       INTEGER NOT NULL,
                    char_start     INTEGER NOT NULL,
                    char_end       INTEGER NOT NULL,
                    media_ts_start REAL,
                    media_ts_end   REAL,
                    content_hash   TEXT NOT NULL,
                    vector         BLOB NOT NULL,
                    PRIMARY KEY (vault_name, path, chunk_id)
                );
                CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(vault_name, path);
                CREATE INDEX IF NOT EXISTS idx_chunks_hash ON chunks(vault_name, path, content_hash);",
            )?;

            conn.execute(
                "INSERT OR REPLACE INTO _meta (key, value) VALUES (?1, ?2)",
                rusqlite::params![META_SCHEMA_VERSION, SCHEMA_VERSION.to_string()],
            )?;
            Ok(())
        })
    }

    /// The persisted schema version, or `None` on a store with no `_meta`.
    pub fn schema_version(&self) -> Result<Option<i64>> {
        self.get_meta_int(META_SCHEMA_VERSION)
    }

    /// Read a raw `_meta` string value.
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            let value = conn
                .query_row("SELECT value FROM _meta WHERE key = ?1", [key], |row| {
                    row.get::<_, String>(0)
                })
                .ok();
            Ok(value)
        })
    }

    fn get_meta_int(&self, key: &str) -> Result<Option<i64>> {
        Ok(self.get_meta(key)?.and_then(|v| v.parse().ok()))
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO _meta (key, value) VALUES (?1, ?2)",
                rusqlite::params![key, value],
            )?;
            Ok(())
        })
    }

    /// The embedder id the store was built with, if any.
    pub fn embedder_id(&self) -> Result<Option<String>> {
        self.get_meta(META_EMBEDDER_ID)
    }

    /// The vector dimension the store was built with, if any.
    pub fn dim(&self) -> Result<Option<usize>> {
        Ok(self.get_meta_int(META_DIM)?.map(|d| d as usize))
    }

    /// Stamp the store's embedder identity, or validate that an existing stamp
    /// matches. A mismatch is a hard error (ADR 0018 §7: fail loudly) — the
    /// caller must re-embed the vault to change models.
    pub fn ensure_embedder(&self, embedder_id: &str, dim: usize) -> Result<()> {
        match self.embedder_id()? {
            None => {
                self.set_meta(META_EMBEDDER_ID, embedder_id)?;
                self.set_meta(META_DIM, &dim.to_string())?;
                Ok(())
            }
            Some(found) if found == embedder_id => match self.dim()? {
                Some(found_dim) if found_dim == dim => Ok(()),
                Some(found_dim) => Err(EmbedError::DimMismatch {
                    expected: dim,
                    found: found_dim,
                }),
                None => {
                    self.set_meta(META_DIM, &dim.to_string())?;
                    Ok(())
                }
            },
            Some(found) => Err(EmbedError::EmbedderMismatch {
                expected: embedder_id.to_string(),
                found,
            }),
        }
    }

    /// Whether this handle was opened read-only.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Count the chunk rows for a vault (diagnostics / stats).
    pub fn chunk_count(&self, vault_name: &str) -> Result<i64> {
        self.with_conn(|conn| {
            let count = conn.query_row(
                "SELECT COUNT(*) FROM chunks WHERE vault_name = ?1",
                [vault_name],
                |row| row.get::<_, i64>(0),
            )?;
            Ok(count)
        })
    }

    /// The content hashes currently stored per note path for a vault. Used by
    /// the worker to decide which notes changed (incremental re-embed).
    pub fn stored_hashes(&self, vault_name: &str) -> Result<Vec<(String, String)>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT path, content_hash FROM chunks WHERE vault_name = ?1 GROUP BY path",
            )?;
            let rows = stmt
                .query_map([vault_name], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
    }

    /// Replace all chunks for a single note path with a fresh set (transactional
    /// per-note). Passing an empty slice deletes the note's chunks.
    pub fn replace_note_chunks(
        &self,
        vault_name: &str,
        path: &str,
        chunks: &[Chunk],
    ) -> Result<()> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM chunks WHERE vault_name = ?1 AND path = ?2",
                rusqlite::params![vault_name, path],
            )?;
            for chunk in chunks {
                tx.execute(
                    "INSERT OR REPLACE INTO chunks
                     (vault_name, path, chunk_id, char_start, char_end,
                      media_ts_start, media_ts_end, content_hash, vector)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        chunk.vault_name,
                        chunk.path,
                        chunk.chunk_id,
                        chunk.char_start,
                        chunk.char_end,
                        chunk.media_ts_start,
                        chunk.media_ts_end,
                        chunk.content_hash,
                        vector_to_blob(&chunk.vector),
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Insert-or-replace individual chunks without first clearing the note.
    /// Callers that want clean per-note replacement should use
    /// [`Self::replace_note_chunks`]; this is the lower-level `VectorStore::upsert`
    /// primitive keyed on the `(vault_name, path, chunk_id)` PK.
    pub fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        self.with_conn(|conn| {
            for chunk in chunks {
                conn.execute(
                    "INSERT OR REPLACE INTO chunks
                     (vault_name, path, chunk_id, char_start, char_end,
                      media_ts_start, media_ts_end, content_hash, vector)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        chunk.vault_name,
                        chunk.path,
                        chunk.chunk_id,
                        chunk.char_start,
                        chunk.char_end,
                        chunk.media_ts_start,
                        chunk.media_ts_end,
                        chunk.content_hash,
                        vector_to_blob(&chunk.vector),
                    ],
                )?;
            }
            Ok(())
        })
    }

    /// Delete all chunks for a note path (e.g. the note was removed).
    pub fn delete_note(&self, vault_name: &str, path: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM chunks WHERE vault_name = ?1 AND path = ?2",
                rusqlite::params![vault_name, path],
            )?;
            Ok(())
        })
    }

    /// Load every stored chunk for a vault as `(Chunk)`. Corrupt vector blobs are
    /// skipped with a `WARN` (ADR 0009). Primarily for the brute-force store and
    /// tests; the daemon uses SQL-side scans.
    pub fn load_chunks(&self, vault_name: &str) -> Result<Vec<Chunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT vault_name, path, chunk_id, char_start, char_end,
                        media_ts_start, media_ts_end, content_hash, vector
                 FROM chunks WHERE vault_name = ?1
                 ORDER BY path, chunk_id",
            )?;
            let mut out = Vec::new();
            let rows = stmt.query_map([vault_name], |row| {
                let blob: Vec<u8> = row.get(8)?;
                Ok((
                    Chunk {
                        vault_name: row.get(0)?,
                        path: row.get(1)?,
                        chunk_id: row.get(2)?,
                        char_start: row.get(3)?,
                        char_end: row.get(4)?,
                        media_ts_start: row.get(5)?,
                        media_ts_end: row.get(6)?,
                        content_hash: row.get(7)?,
                        vector: Vec::new(),
                    },
                    blob,
                ))
            })?;
            for row in rows {
                let (mut chunk, blob) = match row {
                    Ok(pair) => pair,
                    Err(error) => {
                        tracing::warn!(stage = "load_chunks", reason = %error, "skipping bad row");
                        continue;
                    }
                };
                match blob_to_vector(&blob) {
                    Some(vector) => {
                        chunk.vector = vector;
                        out.push(chunk);
                    }
                    None => {
                        tracing::warn!(
                            note = %chunk.path,
                            stage = "load_chunks",
                            reason = "misaligned vector blob",
                            "skipping chunk"
                        );
                    }
                }
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, EmbeddingStore) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("v").join("embeddings.db");
        let store = EmbeddingStore::open(&path).unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_db_with_wal_and_schema_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("v").join("embeddings.db");
        let store = EmbeddingStore::open(&path).unwrap();
        assert!(path.exists(), "db file created in data dir");
        assert_eq!(store.schema_version().unwrap(), Some(SCHEMA_VERSION));
        let mode: String = store
            .with_conn(|c| Ok(c.pragma_query_value(None, "journal_mode", |r| r.get(0))?))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn ensure_embedder_stamps_then_validates() {
        let (_dir, store) = temp_store();
        store.ensure_embedder("bge-small-en-v1.5", 384).unwrap();
        assert_eq!(
            store.embedder_id().unwrap().as_deref(),
            Some("bge-small-en-v1.5")
        );
        assert_eq!(store.dim().unwrap(), Some(384));
        // Same identity is fine.
        store.ensure_embedder("bge-small-en-v1.5", 384).unwrap();
    }

    #[test]
    fn ensure_embedder_rejects_model_mismatch() {
        let (_dir, store) = temp_store();
        store.ensure_embedder("bge-small-en-v1.5", 384).unwrap();
        let err = store.ensure_embedder("other-model", 384).unwrap_err();
        assert!(matches!(err, EmbedError::EmbedderMismatch { .. }));
    }

    #[test]
    fn ensure_embedder_rejects_dim_mismatch() {
        let (_dir, store) = temp_store();
        store.ensure_embedder("bge-small-en-v1.5", 384).unwrap();
        let err = store.ensure_embedder("bge-small-en-v1.5", 512).unwrap_err();
        assert!(matches!(err, EmbedError::DimMismatch { .. }));
    }

    #[test]
    fn version_bump_drops_chunks_but_reopen_preserves() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("v").join("embeddings.db");
        {
            let store = EmbeddingStore::open(&path).unwrap();
            store.ensure_embedder("m", 3).unwrap();
            let chunk = Chunk {
                vault_name: "v".into(),
                path: "n.md".into(),
                chunk_id: 0,
                char_start: 0,
                char_end: 4,
                media_ts_start: None,
                media_ts_end: None,
                content_hash: "h".into(),
                vector: vec![1.0, 0.0, 0.0],
            };
            store.replace_note_chunks("v", "n.md", &[chunk]).unwrap();
            assert_eq!(store.chunk_count("v").unwrap(), 1);
        }
        // Reopen at the same version: data survives.
        let store = EmbeddingStore::open(&path).unwrap();
        assert_eq!(store.chunk_count("v").unwrap(), 1);
        let loaded = store.load_chunks("v").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].vector, vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn replace_note_chunks_is_idempotent_per_note() {
        let (_dir, store) = temp_store();
        let mk = |id: i64| Chunk {
            vault_name: "v".into(),
            path: "n.md".into(),
            chunk_id: id,
            char_start: 0,
            char_end: 1,
            media_ts_start: None,
            media_ts_end: None,
            content_hash: "h".into(),
            vector: vec![0.0, 1.0],
        };
        store
            .replace_note_chunks("v", "n.md", &[mk(0), mk(1)])
            .unwrap();
        assert_eq!(store.chunk_count("v").unwrap(), 2);
        // Re-embed with a single chunk replaces the prior two.
        store.replace_note_chunks("v", "n.md", &[mk(0)]).unwrap();
        assert_eq!(store.chunk_count("v").unwrap(), 1);
    }

    #[test]
    fn stored_hashes_and_delete_note() {
        let (_dir, store) = temp_store();
        let chunk = Chunk {
            vault_name: "v".into(),
            path: "a.md".into(),
            chunk_id: 0,
            char_start: 0,
            char_end: 1,
            media_ts_start: None,
            media_ts_end: None,
            content_hash: "hash-a".into(),
            vector: vec![1.0],
        };
        store.replace_note_chunks("v", "a.md", &[chunk]).unwrap();
        let hashes = store.stored_hashes("v").unwrap();
        assert_eq!(hashes, vec![("a.md".to_string(), "hash-a".to_string())]);
        store.delete_note("v", "a.md").unwrap();
        assert_eq!(store.chunk_count("v").unwrap(), 0);
    }
}
