//! Vector-search benchmark harness (#250, ADR 0018 §5).
//!
//! Measures *this host's* brute-force k-NN latency curve so the decision to
//! switch from the SQLite brute-force store to sqlite-vec / LanceDB stays
//! **data-triggered** rather than guessed. Two things are measured:
//!
//! 1. [`synthetic_knn_bench`] — inserts synthetic vectors at increasing scales
//!    (e.g. 50k…1M) and times k-NN, so [`crossover`] can report the vector
//!    count at which p95 latency first exceeds a threshold (default 150 ms warn
//!    / 300 ms switch, feeding #244).
//! 2. [`golden_baseline`] — embeds a real vault and times searches over it, to
//!    anchor the synthetic curve to real content.
//!
//! Vectors are generated with a tiny deterministic PRNG (no `rand` dependency)
//! so runs are reproducible and CI-friendly.

use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::store::EmbeddingStore;
use crate::vector::{BruteForceStore, Filter, VectorStore};
use crate::{Chunk, Embedder, Result};

/// Latency thresholds (ms) that trigger monitoring action (ADR 0018 §5, #244).
pub const WARN_P95_MS: f64 = 150.0;
pub const SWITCH_P95_MS: f64 = 300.0;

/// A single scale point on the latency curve.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScaleResult {
    /// Number of vectors in the store for this measurement.
    pub count: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    /// Mean k-NN query time in milliseconds.
    pub mean_ms: f64,
}

/// Result of embedding + searching a real vault baseline.
#[derive(Debug, Clone, Serialize)]
pub struct BaselineResult {
    pub notes_embedded: usize,
    pub chunks_written: usize,
    pub embed_ms: f64,
    pub search_p50_ms: f64,
    pub search_p95_ms: f64,
}

/// Deterministic PRNG (SplitMix64) → reproducible synthetic vectors, no deps.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Map to roughly [-1, 1).
        ((z >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0
    }
}

fn synthetic_vector(rng: &mut SplitMix64, dim: usize) -> Vec<f32> {
    let mut v: Vec<f32> = (0..dim).map(|_| rng.next_f32()).collect();
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

fn percentile(sorted_ms: &[f64], pct: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let rank = (pct / 100.0 * (sorted_ms.len() as f64 - 1.0)).round() as usize;
    sorted_ms[rank.min(sorted_ms.len() - 1)]
}

/// Benchmark brute-force k-NN across the given vector `scales`.
///
/// For each scale N: fill a fresh temp store with N synthetic `dim`-vectors,
/// then run `queries` k-NN searches, recording p50/p95/mean query latency.
pub fn synthetic_knn_bench(
    dim: usize,
    scales: &[usize],
    k: usize,
    queries: usize,
) -> Result<Vec<ScaleResult>> {
    const VAULT: &str = "bench";
    let mut results = Vec::with_capacity(scales.len());

    for &n in scales {
        let tmp = tempfile::tempdir().map_err(crate::EmbedError::Io)?;
        let db_path = tmp.path().join("bench.db");
        let store = EmbeddingStore::open(&db_path)?;
        store.ensure_embedder("bench-embedder", dim)?;

        // Insert in batches to keep transactions reasonable.
        let mut rng = SplitMix64(0x1234_5678_9ABC_DEF0);
        let mut batch: Vec<Chunk> = Vec::with_capacity(10_000);
        for i in 0..n {
            batch.push(Chunk {
                vault_name: VAULT.to_string(),
                path: format!("n{}.md", i / 8),
                chunk_id: (i % 8) as i64,
                char_start: 0,
                char_end: 1,
                media_ts_start: None,
                media_ts_end: None,
                content_hash: String::new(),
                vector: synthetic_vector(&mut rng, dim),
            });
            if batch.len() == 10_000 {
                store.upsert_chunks(&batch)?;
                batch.clear();
            }
        }
        if !batch.is_empty() {
            store.upsert_chunks(&batch)?;
        }

        let ranker = BruteForceStore::new(std::sync::Arc::new(store));
        let filter = Filter::vault(VAULT);
        let mut samples = Vec::with_capacity(queries);
        for q in 0..queries {
            let mut qrng = SplitMix64(0xDEAD_BEEF_0000_0000 ^ q as u64);
            let query = synthetic_vector(&mut qrng, dim);
            let start = Instant::now();
            let _ = ranker.search(&query, &filter, k)?;
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean = samples.iter().sum::<f64>() / samples.len().max(1) as f64;
        results.push(ScaleResult {
            count: n,
            p50_ms: percentile(&samples, 50.0),
            p95_ms: percentile(&samples, 95.0),
            mean_ms: mean,
        });
    }
    Ok(results)
}

/// The smallest vector count whose p95 latency exceeds `threshold_ms`, if any.
pub fn crossover(results: &[ScaleResult], threshold_ms: f64) -> Option<usize> {
    results
        .iter()
        .find(|r| r.p95_ms > threshold_ms)
        .map(|r| r.count)
}

/// Embed a real vault and time searches over it, anchoring the synthetic curve.
pub fn golden_baseline(
    vault_name: &str,
    vault_root: &Path,
    embedder: &dyn Embedder,
    queries: &[&str],
) -> Result<BaselineResult> {
    let tmp = tempfile::tempdir().map_err(crate::EmbedError::Io)?;
    let db_path = tmp.path().join("baseline.db");
    let store = EmbeddingStore::open(&db_path)?;

    let embed_start = Instant::now();
    let report = crate::EmbedWorker::new(vault_name, vault_root, &store, embedder).run()?;
    let embed_ms = embed_start.elapsed().as_secs_f64() * 1000.0;

    let ranker = BruteForceStore::new(std::sync::Arc::new(store));
    let filter = Filter::vault(vault_name);
    let mut samples = Vec::new();
    for q in queries {
        let qvec = embedder
            .embed(&[q.to_string()])
            .map_err(|e| crate::EmbedError::Embed(e.to_string()))?
            .pop()
            .unwrap_or_default();
        let start = Instant::now();
        let _ = ranker.search(&qvec, &filter, 10)?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    Ok(BaselineResult {
        notes_embedded: report.embedded,
        chunks_written: report.chunks_written,
        embed_ms,
        search_p50_ms: percentile(&samples, 50.0),
        search_p95_ms: percentile(&samples, 95.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HashEmbedder;

    #[test]
    fn synthetic_bench_reports_increasing_or_stable_curve() {
        // Tiny scales so the test is fast; validates the harness plumbing.
        let results = synthetic_knn_bench(32, &[100, 500, 2_000], 10, 20).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].count, 100);
        assert_eq!(results[2].count, 2_000);
        for r in &results {
            assert!(r.p95_ms >= r.p50_ms);
            assert!(r.p50_ms >= 0.0);
        }
    }

    #[test]
    fn crossover_finds_first_exceeding_scale() {
        let results = vec![
            ScaleResult {
                count: 100,
                p50_ms: 1.0,
                p95_ms: 2.0,
                mean_ms: 1.2,
            },
            ScaleResult {
                count: 1_000,
                p50_ms: 5.0,
                p95_ms: 20.0,
                mean_ms: 6.0,
            },
        ];
        assert_eq!(crossover(&results, 10.0), Some(1_000));
        assert_eq!(crossover(&results, 1_000.0), None);
    }

    #[test]
    fn golden_baseline_over_temp_vault() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("a.md"),
            "# Alpha\n\nsome content about vectors and search",
        )
        .unwrap();
        let embedder = HashEmbedder::new(64);
        let base = golden_baseline("t", tmp.path(), &embedder, &["vectors", "search"]).unwrap();
        assert_eq!(base.notes_embedded, 1);
        assert!(base.chunks_written >= 1);
        assert!(base.search_p95_ms >= base.search_p50_ms);
    }
}
