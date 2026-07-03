//! Daemon-hosted embedding worker supervision (ADR 0018 §2/§8).
//!
//! The daemon spawns one long-lived task per vault that runs the embed worker
//! on an interval, keeping each vault's `embeddings.db` fresh. The worker is the
//! sole writer of that database; the daemon only ever reads it for search
//! (ADR 0018 §2). A pass that errors is logged and the loop continues
//! (supervision), so one bad run never kills the schedule.
//!
//! The embedder is built via [`make_embedder`]. With the `local-embed` feature
//! it is a real `fastembed` model; otherwise it is a non-semantic
//! `HashEmbedder` placeholder so the default daemon build stays lean.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notesmith_embed::{EmbedWorker, Embedder, EmbeddingStore, WorkerReport};
use tokio::task::JoinHandle;

use crate::server::SharedAppState;

/// Default interval between embed passes. Overridable via
/// `NOTESMITH_EMBED_INTERVAL_SECS` (mostly for tests / tuning).
const DEFAULT_EMBED_INTERVAL_SECS: u64 = 300;
/// Small delay before the first pass so startup isn't contended.
const INITIAL_DELAY_SECS: u64 = 10;

/// A running set of per-vault embed worker tasks.
pub struct EmbedSchedulers {
    _tasks: Vec<JoinHandle<()>>,
}

fn embed_interval() -> Duration {
    let secs = std::env::var("NOTESMITH_EMBED_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_EMBED_INTERVAL_SECS);
    Duration::from_secs(secs)
}

/// Run one incremental embed pass for a vault against a specific store path.
/// Extracted for testability: it opens the store, runs the worker, and returns
/// the report.
pub fn run_embed_pass(
    vault_name: &str,
    vault_root: &Path,
    db_path: &Path,
    embedder: &dyn Embedder,
) -> anyhow::Result<WorkerReport> {
    let store = EmbeddingStore::open(db_path)?;
    let worker = EmbedWorker::new(
        vault_name.to_string(),
        vault_root.to_path_buf(),
        &store,
        embedder,
    );
    Ok(worker.run()?)
}

/// Record today's `embed_metrics` trend row (#244): current vector count, the
/// on-disk `embeddings.db` size, and the rolling p95 search latency for this
/// vault. Keyed by calendar date so repeated passes in a day update in place.
/// Best-effort — a failure here must never abort an embed pass.
pub fn record_daily_trend(vault_name: &str, db_path: &Path) -> anyhow::Result<()> {
    let store = EmbeddingStore::open(db_path)?;
    let vector_count = store.chunk_count(vault_name)?;
    let db_bytes = std::fs::metadata(db_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let (_p50, p95) = notesmith_embed::metrics_for(vault_name).percentiles();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    store.record_daily_metric(&date, vault_name, vector_count, db_bytes, p95)?;
    Ok(())
}

/// Build the daemon's embedder. Feature-gated: real model with `local-embed`,
/// otherwise a placeholder.
/// Build the canonical embedder (delegates to [`notesmith_embed::default_embedder`]
/// so the worker and the daemon's query-time embedding always agree).
pub fn make_embedder() -> anyhow::Result<std::sync::Arc<dyn Embedder>> {
    Ok(notesmith_embed::default_embedder()?)
}

/// Spawn and supervise a per-vault embed worker for every configured vault.
pub async fn start_embed_workers(state: SharedAppState) -> EmbedSchedulers {
    let vaults: Vec<(String, PathBuf)> = {
        let state = state.read().await;
        state
            .vaults
            .iter()
            .map(|(name, vs)| (name.clone(), vs.root.clone()))
            .collect()
    };

    let interval = embed_interval();
    let mut tasks = Vec::new();

    for (vault_name, root) in vaults {
        let task = tokio::spawn(async move {
            // Build the embedder once per vault task. If it can't be built
            // (e.g. offline model download), log and give up on this vault
            // rather than spinning.
            let embedder = match make_embedder() {
                Ok(embedder) => embedder,
                Err(error) => {
                    tracing::warn!(
                        vault = %vault_name,
                        reason = %error,
                        "could not initialise embedder; embedding disabled for this vault"
                    );
                    return;
                }
            };

            tokio::time::sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let root = root.clone();
                let vault_name_inner = vault_name.clone();
                let db_path = match notesmith_embed::embeddings_db_path(&vault_name_inner) {
                    Ok(path) => path,
                    Err(error) => {
                        tracing::warn!(
                            vault = %vault_name_inner,
                            reason = %error,
                            "could not resolve embeddings.db path; skipping pass"
                        );
                        continue;
                    }
                };
                match run_embed_pass(&vault_name_inner, &root, &db_path, embedder.as_ref()) {
                    Ok(report) => {
                        if report.embedded > 0 || report.deleted > 0 || report.failed > 0 {
                            tracing::info!(
                                vault = %vault_name_inner,
                                embedded = report.embedded,
                                skipped = report.skipped,
                                deleted = report.deleted,
                                failed = report.failed,
                                chunks = report.chunks_written,
                                "embed pass complete"
                            );
                        }
                        if let Err(error) = record_daily_trend(&vault_name_inner, &db_path) {
                            tracing::warn!(
                                vault = %vault_name_inner,
                                reason = %error,
                                "could not record embed_metrics trend row"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            vault = %vault_name_inner,
                            reason = %error,
                            "embed pass failed; will retry next interval"
                        );
                    }
                }
            }
        });
        tasks.push(task);
    }

    EmbedSchedulers { _tasks: tasks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_embed::HashEmbedder;
    use tempfile::TempDir;

    #[test]
    fn run_embed_pass_embeds_a_temp_vault() {
        let data = TempDir::new().unwrap();
        let db_path = data.path().join("embeddings.db");
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("a.md"), "# A\n\nsome content to embed").unwrap();

        let embedder = HashEmbedder::new(64);
        let report = run_embed_pass("sched-test", vault.path(), &db_path, &embedder).unwrap();
        assert_eq!(report.embedded, 1);
        assert!(report.chunks_written >= 1);

        // A second pass is incremental.
        let report2 = run_embed_pass("sched-test", vault.path(), &db_path, &embedder).unwrap();
        assert_eq!(report2.embedded, 0);
        assert_eq!(report2.skipped, 1);
    }

    #[test]
    fn record_daily_trend_writes_a_row() {
        let data = TempDir::new().unwrap();
        let db_path = data.path().join("embeddings.db");
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("a.md"), "# A\n\ntrend content").unwrap();

        let embedder = HashEmbedder::new(64);
        run_embed_pass("trend-test", vault.path(), &db_path, &embedder).unwrap();

        record_daily_trend("trend-test", &db_path).unwrap();
        let store = EmbeddingStore::open(&db_path).unwrap();
        assert_eq!(store.metric_row_count().unwrap(), 1);

        // Re-recording the same day replaces (upsert by date), not appends.
        record_daily_trend("trend-test", &db_path).unwrap();
        assert_eq!(store.metric_row_count().unwrap(), 1);
    }

    #[test]
    fn interval_respects_env_override() {
        // SAFETY: single-threaded test.
        unsafe {
            std::env::set_var("NOTESMITH_EMBED_INTERVAL_SECS", "42");
        }
        assert_eq!(embed_interval(), Duration::from_secs(42));
        unsafe {
            std::env::remove_var("NOTESMITH_EMBED_INTERVAL_SECS");
        }
    }
}
