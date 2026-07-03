//! Daemon-side embedding search primitive (ADR 0018 §5/§7).
//!
//! The daemon never writes `embeddings.db` — the embed worker owns it. Here we
//! open it **read-only** and do two things:
//!
//! 1. **Query-time embedding in the daemon** (§7): the daemon hosts the
//!    `Embedder` and vectorises the query string. The store records the
//!    `embedder_id`/`dim` it was built with; we enforce a match and fail loudly
//!    on mismatch so a model change can never silently return garbage.
//! 2. **Metadata JOIN via ATTACH** (§5): to scope a search by note metadata
//!    (e.g. a tag) we `ATTACH` the note index (`cache.sqlite`) and run a real
//!    JOIN against it to resolve the set of eligible note paths. k-NN ranking
//!    over the surviving vectors returns chunk refs with **raw distances**.
//!
//! Ranking is currently a brute-force cosine scan ([`BruteForceStore`]); ADR
//! 0018 §5 permits this until the #250 benchmark + #244 observability say it is
//! time to swap in sqlite-vec / LanceDB behind the same trait.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use crate::{BruteForceStore, ChunkRef, Embedder, EmbeddingStore, Filter, VectorStore};
use rusqlite::{Connection, OpenFlags};

/// Errors from constructing or running an [`EmbeddingSearch`].
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingSearchError {
    #[error("embeddings sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("embed store error: {0}")]
    Store(#[from] crate::EmbedError),
    #[error(
        "query embedder '{query}' does not match the store's '{store}'; \
         re-embed the vault or use the matching model"
    )]
    EmbedderMismatch { query: String, store: String },
    #[error("query embedder dim {query} does not match the store's dim {store}")]
    DimMismatch { query: usize, store: usize },
    #[error("embedding the query failed: {0}")]
    Embed(String),
}

/// Metadata scoping for a search, resolved against the note index via ATTACH.
#[derive(Debug, Clone, Default)]
pub struct MetaFilter {
    /// Restrict to notes carrying this tag.
    pub tag: Option<String>,
    /// Restrict to notes whose path starts with this prefix (folder scope).
    pub path_prefix: Option<String>,
}

impl MetaFilter {
    fn is_empty(&self) -> bool {
        self.tag.is_none() && self.path_prefix.is_none()
    }
}

/// A scored chunk returned by [`EmbeddingSearch::search`].
#[derive(Debug, Clone)]
pub struct ScoredChunk {
    pub chunk: ChunkRef,
    /// Raw cosine distance (lower is nearer).
    pub distance: f32,
}

/// Read-only semantic search over a vault's `embeddings.db`.
pub struct EmbeddingSearch {
    vault_name: String,
    ranker: BruteForceStore,
    embedder: Arc<dyn Embedder>,
    dim: usize,
    /// Separate read-only connection with the note index ATTACHed (if present),
    /// used only to resolve metadata filters to a path set. Wrapped in a mutex
    /// so the searcher is `Sync` and can be shared across daemon requests.
    filter_conn: std::sync::Mutex<Connection>,
    has_index: bool,
}

impl EmbeddingSearch {
    /// Open the vault's store read-only, enforce the embedder match, and ATTACH
    /// the note index for metadata filtering.
    pub fn open(
        vault_name: impl Into<String>,
        embeddings_db_path: &Path,
        index_db_path: &Path,
        embedder: Arc<dyn Embedder>,
    ) -> Result<Self, EmbeddingSearchError> {
        let vault_name = vault_name.into();
        let store = Arc::new(EmbeddingStore::open_read_only(embeddings_db_path)?);

        // Fail loudly if the query embedder disagrees with the stored model.
        if let Some(store_id) = store.embedder_id()? {
            if store_id != embedder.id() {
                return Err(EmbeddingSearchError::EmbedderMismatch {
                    query: embedder.id().to_string(),
                    store: store_id,
                });
            }
        }
        if let Some(store_dim) = store.dim()? {
            if store_dim != embedder.dim() {
                return Err(EmbeddingSearchError::DimMismatch {
                    query: embedder.dim(),
                    store: store_dim,
                });
            }
        }

        let filter_conn = Connection::open_with_flags(
            embeddings_db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let has_index = index_db_path.exists();
        if has_index {
            // Read-only ATTACH of the note index for metadata JOINs.
            filter_conn.execute(
                "ATTACH DATABASE ?1 AS idx",
                [index_db_path.to_string_lossy().as_ref()],
            )?;
        }

        Ok(Self {
            vault_name,
            ranker: BruteForceStore::new(store),
            dim: embedder.dim(),
            embedder,
            filter_conn: std::sync::Mutex::new(filter_conn),
            has_index,
        })
    }

    /// Semantic search: embed `query` in-daemon, apply the metadata filter, and
    /// return the top-`k` nearest chunks with raw distances.
    pub fn search(
        &self,
        query: &str,
        k: usize,
        filter: &MetaFilter,
    ) -> Result<Vec<ScoredChunk>, EmbeddingSearchError> {
        if query.trim().is_empty() || k == 0 {
            return Ok(Vec::new());
        }

        let mut vectors = self
            .embedder
            .embed(&[query.to_string()])
            .map_err(|e| EmbeddingSearchError::Embed(e.to_string()))?;
        let qvec = vectors
            .pop()
            .ok_or_else(|| EmbeddingSearchError::Embed("embedder returned no vector".into()))?;
        if qvec.len() != self.dim {
            return Err(EmbeddingSearchError::DimMismatch {
                query: qvec.len(),
                store: self.dim,
            });
        }

        let allowed_paths = self.resolve_allowed_paths(filter)?;
        let hits = self.ranker.search(
            &qvec,
            &Filter {
                vault_name: self.vault_name.clone(),
                allowed_paths,
            },
            k,
        )?;
        Ok(hits
            .into_iter()
            .map(|(chunk, distance)| ScoredChunk { chunk, distance })
            .collect())
    }

    /// Resolve a metadata filter to the set of eligible note paths using a real
    /// SQL query. Tag filters JOIN the ATTACHed note index; a `None` result
    /// means "no restriction".
    fn resolve_allowed_paths(
        &self,
        filter: &MetaFilter,
    ) -> Result<Option<HashSet<String>>, EmbeddingSearchError> {
        if filter.is_empty() {
            return Ok(None);
        }

        let mut paths = HashSet::new();
        let conn = self
            .filter_conn
            .lock()
            .expect("embedding search filter connection poisoned");

        match (&filter.tag, self.has_index) {
            (Some(tag), true) => {
                // Real ATTACH JOIN: chunks ⋈ idx.tags on (vault_name, note_path).
                let mut sql = String::from(
                    "SELECT DISTINCT c.path FROM chunks c \
                     JOIN idx.tags t \
                       ON t.vault_name = c.vault_name AND t.note_path = c.path \
                     WHERE c.vault_name = ?1 AND t.tag = ?2",
                );
                if filter.path_prefix.is_some() {
                    sql.push_str(" AND c.path LIKE ?3 || '%'");
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows = if let Some(prefix) = &filter.path_prefix {
                    stmt.query_map(rusqlite::params![self.vault_name, tag, prefix], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<String>>>()?
                } else {
                    stmt.query_map(rusqlite::params![self.vault_name, tag], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<String>>>()?
                };
                paths.extend(rows);
            }
            (Some(_), false) => {
                // Tag filter requested but no index attached → nothing matches.
                return Ok(Some(HashSet::new()));
            }
            (None, _) => {
                // Path-prefix only: query embeddings.db directly.
                let prefix = filter.path_prefix.as_deref().unwrap_or("");
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT path FROM chunks \
                     WHERE vault_name = ?1 AND path LIKE ?2 || '%'",
                )?;
                let rows = stmt.query_map(rusqlite::params![self.vault_name, prefix], |r| {
                    r.get::<_, String>(0)
                })?;
                for path in rows {
                    paths.insert(path?);
                }
            }
        }

        Ok(Some(paths))
    }
}
