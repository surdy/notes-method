//! Vector-store abstraction and a brute-force cosine implementation.
//!
//! ADR 0018 §1/§5: search is exposed behind the [`VectorStore`] trait so the
//! backing engine can evolve (brute-force → sqlite-vec `vec0` → LanceDB)
//! without touching callers. `search` returns **raw distances** so downstream
//! ranking (#199 RRF, weighted blends) has the real scores to work with.
//!
//! The default [`BruteForceStore`] scans every stored vector and computes
//! cosine distance. ADR 0018 §5 explicitly permits starting brute-force; the
//! benchmark harness (#250) + observability (#244) tell us when to swap in a
//! native index. Metadata filtering is expressed as an optional *allowed-path*
//! set: the daemon resolves vault/tag/type/date predicates against the note
//! index and passes the surviving paths here, keeping this store decoupled from
//! note metadata.

use std::collections::HashSet;
use std::sync::Arc;

use crate::store::EmbeddingStore;
use crate::{Chunk, Result};

/// A lightweight reference to a stored chunk, returned by search. Carries the
/// citation offsets so callers can quote the exact span.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkRef {
    pub vault_name: String,
    pub path: String,
    pub chunk_id: i64,
    pub char_start: i64,
    pub char_end: i64,
    pub media_ts_start: Option<f64>,
    pub media_ts_end: Option<f64>,
    pub content_hash: String,
}

impl ChunkRef {
    fn from_chunk(chunk: &Chunk) -> Self {
        Self {
            vault_name: chunk.vault_name.clone(),
            path: chunk.path.clone(),
            chunk_id: chunk.chunk_id,
            char_start: chunk.char_start,
            char_end: chunk.char_end,
            media_ts_start: chunk.media_ts_start,
            media_ts_end: chunk.media_ts_end,
            content_hash: chunk.content_hash.clone(),
        }
    }
}

/// Metadata scoping for a search. `vault_name` is required; `allowed_paths`,
/// when `Some`, restricts results to that set (resolved from the note index by
/// the daemon for tag/type/date filters).
#[derive(Debug, Clone)]
pub struct Filter {
    pub vault_name: String,
    pub allowed_paths: Option<HashSet<String>>,
}

impl Filter {
    /// A filter over a whole vault with no metadata restriction.
    pub fn vault(vault_name: impl Into<String>) -> Self {
        Self {
            vault_name: vault_name.into(),
            allowed_paths: None,
        }
    }

    fn permits(&self, path: &str) -> bool {
        match &self.allowed_paths {
            None => true,
            Some(set) => set.contains(path),
        }
    }
}

/// The vector-store abstraction. Implementations are the sole owners of how
/// vectors are indexed and scanned.
pub trait VectorStore {
    /// Insert or replace the given chunks (keyed by `(vault, path, chunk_id)`).
    fn upsert(&self, chunks: &[Chunk]) -> Result<()>;

    /// k-nearest chunks to `query` under `filter`, as `(ChunkRef, raw_distance)`
    /// sorted by ascending distance (nearest first).
    fn search(&self, query: &[f32], filter: &Filter, k: usize) -> Result<Vec<(ChunkRef, f32)>>;

    /// Delete every chunk for a note path.
    fn delete(&self, vault_name: &str, path: &str) -> Result<()>;
}

/// Brute-force cosine store over the SQLite-backed [`EmbeddingStore`].
pub struct BruteForceStore {
    store: Arc<EmbeddingStore>,
}

impl BruteForceStore {
    pub fn new(store: Arc<EmbeddingStore>) -> Self {
        Self { store }
    }

    /// The underlying store handle.
    pub fn store(&self) -> &Arc<EmbeddingStore> {
        &self.store
    }
}

impl VectorStore for BruteForceStore {
    fn upsert(&self, chunks: &[Chunk]) -> Result<()> {
        self.store.upsert_chunks(chunks)
    }

    fn search(&self, query: &[f32], filter: &Filter, k: usize) -> Result<Vec<(ChunkRef, f32)>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let query_norm = norm(query);
        let candidates = self.store.load_chunks(&filter.vault_name)?;
        let mut scored: Vec<(ChunkRef, f32)> = candidates
            .iter()
            .filter(|c| filter.permits(&c.path))
            .map(|c| {
                (
                    ChunkRef::from_chunk(c),
                    cosine_distance(query, query_norm, &c.vector),
                )
            })
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }

    fn delete(&self, vault_name: &str, path: &str) -> Result<()> {
        self.store.delete_note(vault_name, path)
    }
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Cosine distance in `[0, 2]` (`1 - cosine_similarity`). Mismatched dims or a
/// zero-norm vector yield the maximum distance (1.0) so the chunk sorts last
/// without panicking (ADR 0009).
fn cosine_distance(query: &[f32], query_norm: f32, candidate: &[f32]) -> f32 {
    if query.len() != candidate.len() {
        return 1.0;
    }
    let cand_norm = norm(candidate);
    if query_norm == 0.0 || cand_norm == 0.0 {
        return 1.0;
    }
    let dot: f32 = query.iter().zip(candidate).map(|(a, b)| a * b).sum();
    1.0 - (dot / (query_norm * cand_norm))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store() -> (TempDir, BruteForceStore) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("v").join("embeddings.db");
        let store = Arc::new(EmbeddingStore::open(&path).unwrap());
        (dir, BruteForceStore::new(store))
    }

    fn chunk(path: &str, id: i64, vector: Vec<f32>) -> Chunk {
        Chunk {
            vault_name: "v".into(),
            path: path.into(),
            chunk_id: id,
            char_start: 0,
            char_end: 1,
            media_ts_start: None,
            media_ts_end: None,
            content_hash: "h".into(),
            vector,
        }
    }

    #[test]
    fn search_ranks_nearest_first_with_raw_distances() {
        let (_dir, vs) = store();
        vs.upsert(&[
            chunk("a.md", 0, vec![1.0, 0.0]),
            chunk("b.md", 0, vec![0.0, 1.0]),
            chunk("c.md", 0, vec![0.9, 0.1]),
        ])
        .unwrap();
        let results = vs.search(&[1.0, 0.0], &Filter::vault("v"), 3).unwrap();
        assert_eq!(results.len(), 3);
        // a.md is identical → distance ~0, then c.md, then b.md (orthogonal → ~1).
        assert_eq!(results[0].0.path, "a.md");
        assert!(results[0].1 < 1e-6);
        assert_eq!(results[2].0.path, "b.md");
        assert!((results[2].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn search_honours_allowed_paths_filter() {
        let (_dir, vs) = store();
        vs.upsert(&[
            chunk("a.md", 0, vec![1.0, 0.0]),
            chunk("b.md", 0, vec![0.9, 0.1]),
        ])
        .unwrap();
        let mut allowed = HashSet::new();
        allowed.insert("b.md".to_string());
        let filter = Filter {
            vault_name: "v".into(),
            allowed_paths: Some(allowed),
        };
        let results = vs.search(&[1.0, 0.0], &filter, 5).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.path, "b.md");
    }

    #[test]
    fn search_returns_citation_offsets() {
        let (_dir, vs) = store();
        let mut c = chunk("a.md", 2, vec![1.0, 0.0]);
        c.char_start = 10;
        c.char_end = 42;
        vs.upsert(&[c]).unwrap();
        let results = vs.search(&[1.0, 0.0], &Filter::vault("v"), 1).unwrap();
        assert_eq!(results[0].0.char_start, 10);
        assert_eq!(results[0].0.char_end, 42);
        assert_eq!(results[0].0.chunk_id, 2);
    }

    #[test]
    fn delete_removes_note_chunks() {
        let (_dir, vs) = store();
        vs.upsert(&[chunk("a.md", 0, vec![1.0, 0.0])]).unwrap();
        vs.delete("v", "a.md").unwrap();
        assert!(
            vs.search(&[1.0, 0.0], &Filter::vault("v"), 5)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn mismatched_dim_does_not_panic() {
        let (_dir, vs) = store();
        vs.upsert(&[chunk("a.md", 0, vec![1.0, 0.0, 0.0])]).unwrap();
        let results = vs.search(&[1.0, 0.0], &Filter::vault("v"), 5).unwrap();
        assert_eq!(results.len(), 1);
        assert!((results[0].1 - 1.0).abs() < 1e-6);
    }
}
