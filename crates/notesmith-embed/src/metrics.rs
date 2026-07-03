//! Vector-search latency instrumentation (#244, ADR 0018 §5).
//!
//! Every daemon-side vector search records a sample here so p50/p95 latency can
//! be surfaced by the stats endpoint and compared against the switch thresholds
//! (150 ms warn / 300 ms switch). Samples are kept in a small rolling window per
//! vault, held in a process-global registry so the search path (in
//! `notesmith-ops`/`notesmith-embed`) and the HTTP stats handler (in
//! `notesmith-http`) share the same data without threading state through.
//!
//! Recording a sample also emits the required tracing span:
//! `INFO stage=vector_search n_vectors=<N> k=<K> filtered=<bool> duration_ms=<ms>`.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

/// How many recent samples to retain per vault for percentile estimates.
const WINDOW: usize = 256;

/// One vector-search measurement.
#[derive(Debug, Clone, Copy)]
pub struct SearchSample {
    pub n_vectors: usize,
    pub k: usize,
    /// Whether a metadata prefilter narrowed the candidate set (only *unfiltered*
    /// whole-corpus searches force the LanceDB switch decision).
    pub filtered: bool,
    pub duration_ms: f64,
}

/// A rolling window of recent search latencies for one vault.
#[derive(Default)]
pub struct SearchMetrics {
    durations: Mutex<VecDeque<f64>>,
}

impl SearchMetrics {
    /// Record a sample: emit the tracing span and push into the rolling window.
    pub fn record(&self, sample: SearchSample) {
        tracing::info!(
            stage = "vector_search",
            n_vectors = sample.n_vectors,
            k = sample.k,
            filtered = sample.filtered,
            duration_ms = sample.duration_ms,
            "vector search latency"
        );
        let mut w = self.durations.lock().expect("search metrics poisoned");
        if w.len() == WINDOW {
            w.pop_front();
        }
        w.push_back(sample.duration_ms);
    }

    /// `(p50_ms, p95_ms)` over the current window; `(0, 0)` when empty.
    pub fn percentiles(&self) -> (f64, f64) {
        let w = self.durations.lock().expect("search metrics poisoned");
        if w.is_empty() {
            return (0.0, 0.0);
        }
        let mut sorted: Vec<f64> = w.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        (percentile(&sorted, 50.0), percentile(&sorted, 95.0))
    }

    /// Number of samples currently retained.
    pub fn sample_count(&self) -> usize {
        self.durations
            .lock()
            .expect("search metrics poisoned")
            .len()
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (pct / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

static REGISTRY: LazyLock<Mutex<HashMap<String, std::sync::Arc<SearchMetrics>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The shared [`SearchMetrics`] for `vault`, created on first use.
pub fn metrics_for(vault: &str) -> std::sync::Arc<SearchMetrics> {
    let mut reg = REGISTRY.lock().expect("metrics registry poisoned");
    reg.entry(vault.to_string())
        .or_insert_with(|| std::sync::Arc::new(SearchMetrics::default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_track_recorded_samples() {
        let m = SearchMetrics::default();
        for ms in [10.0, 20.0, 30.0, 40.0, 100.0] {
            m.record(SearchSample {
                n_vectors: 1000,
                k: 10,
                filtered: false,
                duration_ms: ms,
            });
        }
        let (p50, p95) = m.percentiles();
        assert!((20.0..=40.0).contains(&p50), "p50 was {p50}");
        assert!(p95 >= 40.0, "p95 was {p95}");
        assert_eq!(m.sample_count(), 5);
    }

    #[test]
    fn window_is_bounded() {
        let m = SearchMetrics::default();
        for i in 0..(WINDOW + 50) {
            m.record(SearchSample {
                n_vectors: 1,
                k: 1,
                filtered: true,
                duration_ms: i as f64,
            });
        }
        assert_eq!(m.sample_count(), WINDOW);
    }

    #[test]
    fn registry_shares_instances_by_vault() {
        let a = metrics_for("reg-test-vault");
        a.record(SearchSample {
            n_vectors: 1,
            k: 1,
            filtered: false,
            duration_ms: 5.0,
        });
        let b = metrics_for("reg-test-vault");
        assert_eq!(b.sample_count(), 1);
    }

    #[test]
    fn empty_metrics_report_zero() {
        let m = SearchMetrics::default();
        assert_eq!(m.percentiles(), (0.0, 0.0));
    }
}
