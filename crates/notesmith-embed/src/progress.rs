//! Live embed-worker progress signal (#260, exact-progress extension).
//!
//! The `/embeddings/stats` endpoint can only *infer* a running build from a
//! climbing vector count between polls. This module exposes an authoritative,
//! process-global progress signal instead: whether a pass is `running`, the
//! total notes it will visit (`notes_total`), and how many it has finished
//! (`notes_done`). Like [`crate::metrics_for`], it lives in a per-vault
//! registry so the worker (this crate) and the HTTP stats handler (in
//! `notesmith-http`) share it without threading state through.
//!
//! `notes_done` counts *every* note the pass visits (embedded, unchanged, or
//! failed), so the bar tracks the whole pass rather than only changed notes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// An immutable read of a vault's embed-pass progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedProgressSnapshot {
    /// Whether a pass is currently running.
    pub running: bool,
    /// Total notes the current (or most recent) pass will visit.
    pub notes_total: u64,
    /// Notes visited so far in the current (or most recent) pass.
    pub notes_done: u64,
    /// Unix seconds when the current (or most recent) pass began, if any.
    pub started_at: Option<u64>,
}

/// Mutable, lock-free progress counters for one vault's embed worker.
#[derive(Default)]
pub struct EmbedProgress {
    running: AtomicBool,
    notes_total: AtomicU64,
    notes_done: AtomicU64,
    /// Unix seconds of the current pass start; `0` means "never started".
    started_at: AtomicU64,
}

impl EmbedProgress {
    /// Mark the start of a pass over `total` notes, resetting the done counter.
    pub fn begin(&self, total: u64) {
        self.notes_total.store(total, Ordering::Relaxed);
        self.notes_done.store(0, Ordering::Relaxed);
        self.started_at.store(now_secs(), Ordering::Relaxed);
        self.running.store(true, Ordering::Relaxed);
    }

    /// Record that one more note has been visited.
    pub fn advance(&self) {
        self.notes_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Mark the pass complete. Snaps `notes_done` up to `notes_total` so the
    /// final snapshot reads a clean 100%, then clears the running flag.
    pub fn finish(&self) {
        let total = self.notes_total.load(Ordering::Relaxed);
        self.notes_done.store(total, Ordering::Relaxed);
        self.running.store(false, Ordering::Relaxed);
    }

    /// Take a consistent-enough read of the current counters.
    pub fn snapshot(&self) -> EmbedProgressSnapshot {
        let started = self.started_at.load(Ordering::Relaxed);
        EmbedProgressSnapshot {
            running: self.running.load(Ordering::Relaxed),
            notes_total: self.notes_total.load(Ordering::Relaxed),
            notes_done: self.notes_done.load(Ordering::Relaxed),
            started_at: (started != 0).then_some(started),
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

static REGISTRY: LazyLock<Mutex<HashMap<String, Arc<EmbedProgress>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The shared [`EmbedProgress`] for `vault`, created on first use.
pub fn progress_for(vault: &str) -> Arc<EmbedProgress> {
    let mut reg = REGISTRY.lock().expect("embed progress registry poisoned");
    reg.entry(vault.to_string())
        .or_insert_with(|| Arc::new(EmbedProgress::default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_advance_finish_tracks_progress() {
        let p = EmbedProgress::default();
        assert_eq!(
            p.snapshot(),
            EmbedProgressSnapshot {
                running: false,
                notes_total: 0,
                notes_done: 0,
                started_at: None,
            }
        );

        p.begin(3);
        let mid = p.snapshot();
        assert!(mid.running);
        assert_eq!(mid.notes_total, 3);
        assert_eq!(mid.notes_done, 0);
        assert!(mid.started_at.is_some());

        p.advance();
        p.advance();
        assert_eq!(p.snapshot().notes_done, 2);

        p.finish();
        let done = p.snapshot();
        assert!(!done.running);
        assert_eq!(done.notes_total, 3);
        // finish() snaps done up to total for a clean 100%.
        assert_eq!(done.notes_done, 3);
    }

    #[test]
    fn begin_resets_done_counter() {
        let p = EmbedProgress::default();
        p.begin(2);
        p.advance();
        p.finish();
        p.begin(5);
        let s = p.snapshot();
        assert_eq!(s.notes_done, 0);
        assert_eq!(s.notes_total, 5);
        assert!(s.running);
    }

    #[test]
    fn registry_shares_instances_by_vault() {
        let a = progress_for("progress-reg-test");
        a.begin(7);
        let b = progress_for("progress-reg-test");
        assert_eq!(b.snapshot().notes_total, 7);
    }
}
