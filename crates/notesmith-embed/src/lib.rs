//! notesmith-embed: local embeddings + vector search for Notesmith.
//!
//! This crate owns the per-vault embeddings store (`embeddings.db`), the
//! [`Embedder`] and [`VectorStore`] abstractions, the note chunker, and the
//! `notesmith embed` worker that keeps the store fresh. See
//! `docs/adr/0018-embedding-and-vector-search.md` for the full design.
//!
//! Placement **B** (ADR 0018 §2): the embed worker is the *sole writer* of
//! `embeddings.db`; the daemon opens it read-only for search. The store lives
//! in the durable data dir (`data_dir/<vault>/embeddings.db`), not the
//! rebuildable cache, mirroring the `transcripts.sqlite` precedent.
//!
//! Per ADR 0009 (resilience to malformed content), per-note operations degrade
//! rather than fail: a bad note is skipped with a `WARN` and the batch
//! continues. No path panics on untrusted `.md` content.

mod bench;
mod chunker;
mod embedder;
mod metrics;
mod paths;
mod search;
mod store;
mod vector;
mod worker;

pub use bench::{
    BaselineResult, SWITCH_P95_MS, ScaleResult, WARN_P95_MS, crossover, golden_baseline,
    synthetic_knn_bench,
};
pub use chunker::{ChunkSpan, ChunkerOptions, chunk_note};
pub use embedder::{CANONICAL_DIM, CANONICAL_MODEL_ID, LOCAL_EMBED_COMPILED};
pub use embedder::{Embedder, HashEmbedder, default_embedder};
pub use metrics::{SearchMetrics, SearchSample, metrics_for};
pub use paths::{data_dir, embeddings_db_path, sanitize_vault_name};
pub use search::{EmbeddingSearch, EmbeddingSearchError, MetaFilter, ScoredChunk};
pub use store::{EmbeddingStore, SCHEMA_VERSION};
pub use vector::{BruteForceStore, ChunkRef, Filter, VectorStore};
pub use worker::{EmbedWorker, WorkerReport};

#[cfg(feature = "local-embed")]
pub use embedder::LocalFastEmbed;

/// Errors returned across the embed crate.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("embeddings sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("embeddings io error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "embedder mismatch: store was built with '{found}' but caller uses '{expected}' \
         (re-embed the vault to change models)"
    )]
    EmbedderMismatch { expected: String, found: String },
    #[error("dimension mismatch: store dim is {found} but caller uses {expected}")]
    DimMismatch { expected: usize, found: usize },
    #[error("embedding failed: {0}")]
    Embed(String),
}

/// Convenience alias for embed-crate results.
pub type Result<T> = std::result::Result<T, EmbedError>;

/// A single embedded chunk of a note (or, later, a media transcript).
///
/// `chunk_id` is a monotonic per-note index (0-based). `char_start`/`char_end`
/// are byte offsets into the note body for citation. `media_ts_*` carry the
/// timestamp span for transcript chunks (ADR 0019) and are `None` for notes.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub vault_name: String,
    pub path: String,
    pub chunk_id: i64,
    pub char_start: i64,
    pub char_end: i64,
    pub media_ts_start: Option<f64>,
    pub media_ts_end: Option<f64>,
    pub content_hash: String,
    pub vector: Vec<f32>,
}

/// Encode an `f32` vector as a little-endian byte BLOB for SQLite storage.
pub(crate) fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Decode a little-endian byte BLOB back into an `f32` vector. Returns `None`
/// if the blob length is not a multiple of 4 (corrupt row → skip, ADR 0009).
pub(crate) fn blob_to_vector(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

#[cfg(test)]
mod blob_tests {
    use super::*;

    #[test]
    fn blob_roundtrip_preserves_vector() {
        let v = vec![0.0_f32, 1.5, -2.25, 3.125];
        let blob = vector_to_blob(&v);
        assert_eq!(blob.len(), 16);
        assert_eq!(blob_to_vector(&blob), Some(v));
    }

    #[test]
    fn blob_to_vector_rejects_misaligned_length() {
        assert_eq!(blob_to_vector(&[0, 1, 2]), None);
    }
}
