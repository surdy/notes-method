//! Embedding observability endpoint (#244).
//!
//! `GET /api/v/{vault}/embeddings/stats` exposes the current state of a vault's
//! embedding index and its rolling search latency so operators can watch the
//! scaling signals documented in `docs/embeddings/05-scaling-and-monitoring.md`
//! (vector count, on-disk size, p50/p95 query latency) and decide when to move
//! from the brute-force SQLite store to LanceDB.
//!
//! The endpoint only ever reads `embeddings.db` (opened read-only) plus the
//! in-process metrics registry. If a vault has never been embedded the database
//! is absent; that is reported as an empty-but-valid index (zero vectors), not
//! an error.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;

#[derive(Debug, Serialize)]
pub struct EmbeddingStats {
    /// Number of stored chunk vectors for this vault.
    pub vector_count: i64,
    /// Size of `embeddings.db` on disk, in bytes.
    pub db_bytes: u64,
    /// Vector dimensionality (from the store's `_meta`), if any vectors exist.
    pub dim: Option<usize>,
    /// Identifier of the embedder that produced the stored vectors, if known.
    pub embedder_id: Option<String>,
    /// Rolling p50 search latency (ms) over the recent query window.
    pub p50_ms: f64,
    /// Rolling p95 search latency (ms) over the recent query window.
    pub p95_ms: f64,
    /// Number of latency samples backing the percentiles above.
    pub sample_count: usize,
    /// Unix seconds of the last embeddings.db write, if the file exists.
    pub last_ingest_at: Option<u64>,
    /// Whether an embed pass is currently running for this vault (#260).
    pub running: bool,
    /// Total notes the current (or most recent) pass will visit.
    pub notes_total: u64,
    /// Notes visited so far in the current (or most recent) pass.
    pub notes_done: u64,
    /// Unix seconds when the current (or most recent) pass began, if any.
    pub started_at: Option<u64>,
}

pub async fn get_embedding_stats(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<EmbeddingStats>, (StatusCode, Json<Value>)> {
    // Confirm the vault exists (consistent 404 with the rest of the API).
    {
        let state = state.read().await;
        if !state.vaults.contains_key(&vault_name) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("vault not found: {vault_name}") })),
            ));
        }
    }

    let db_path = notesmith_embed::embeddings_db_path(&vault_name).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("could not resolve embeddings db path: {error}") })),
        )
    })?;

    let metrics = notesmith_embed::metrics_for(&vault_name);
    let (p50_ms, p95_ms) = metrics.percentiles();
    let sample_count = metrics.sample_count();

    // Live worker progress: authoritative running/total/done for the current
    // (or most recent) pass, so the UI can show a determinate bar (#260).
    let progress = notesmith_embed::progress_for(&vault_name).snapshot();

    // File-level facts: size + last-write time. Absent file => never embedded.
    let (db_bytes, last_ingest_at) = match std::fs::metadata(&db_path) {
        Ok(meta) => {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            (meta.len(), mtime)
        }
        Err(_) => (0, None),
    };

    // Vector-level facts come from the store, opened read-only. A missing or
    // unreadable database degrades to an empty index rather than a 500.
    let (vector_count, dim, embedder_id) =
        match notesmith_embed::EmbeddingStore::open_read_only(&db_path) {
            Ok(store) => {
                let vector_count = store.chunk_count(&vault_name).unwrap_or(0);
                let dim = store.dim().ok().flatten();
                let embedder_id = store.embedder_id().ok().flatten();
                (vector_count, dim, embedder_id)
            }
            Err(_) => (0, None, None),
        };

    Ok(Json(EmbeddingStats {
        vector_count,
        db_bytes,
        dim,
        embedder_id,
        p50_ms,
        p95_ms,
        sample_count,
        last_ingest_at,
        running: progress.running,
        notes_total: progress.notes_total,
        notes_done: progress.notes_done,
        started_at: progress.started_at,
    }))
}
