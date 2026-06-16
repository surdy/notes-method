//! A shared, bounded diagnostics log for agent errors and ACP "wire" messages
//! (issue #192).
//!
//! This is the runtime counterpart to the on-demand agent-*discovery* trace
//! (ADR 0013): rather than probing binaries, it captures what actually happens
//! while a live ACP session runs, so the Settings "Diagnostics" surface can show
//! recent agent errors and — when verbose mode is on — a "wire-ish" log of the
//! protocol traffic.
//!
//! ## Wire-log scoping
//!
//! The `agent_client_protocol` crate (Zed) owns the raw JSON-RPC subprocess
//! transport (framing, request/response correlation, dispatch). We cannot
//! intercept the literal wire bytes without forking it. So the "wire log" is a
//! capture of every ACP message **at our own mediation boundary** in the
//! [`crate::acp`] driver: outgoing prompts, the normalized events we emit from
//! incoming `session/update`s, permission requests, and fs/terminal calls. This
//! is the honest, achievable "wire-ish" log — see the comments in
//! [`crate::acp`] `start_driver` for where each boundary is recorded.
//!
//! ## Resilience (ADR 0009)
//!
//! The log never `unwrap`/`expect`s on dynamic data. A poisoned mutex degrades
//! to a dropped record (or an empty snapshot) rather than a panic, and every
//! `summary`/`detail` string is capped so a chatty agent can never flood memory.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Default maximum number of retained entries; the oldest is evicted first.
const DEFAULT_CAPACITY: usize = 500;

/// Hard per-field character cap so a single entry can never grow without bound.
const FIELD_CAP: usize = 2000;

/// The kind of a recorded diagnostics entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagKind {
    /// An agent error (handshake failure, prompt/send failure, denial). Always
    /// recorded, regardless of the verbose toggle.
    Error,
    /// A mediated ACP message (prompt, emitted event, permission/fs/terminal
    /// request). Recorded only when verbose mode is on.
    Wire,
}

/// A single bounded diagnostics record crossing the Tauri boundary.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagEntry {
    /// Wall-clock capture time in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// Whether this is an error or a wire message.
    pub kind: DiagKind,
    /// The agent this entry relates to (typically the launched program name).
    pub agent: Option<String>,
    /// A short, human-readable one-line summary.
    pub summary: String,
    /// Optional longer detail (capped), shown expandable in the UI.
    pub detail: Option<String>,
}

/// Cap a string to [`FIELD_CAP`] characters without splitting a UTF-8 char.
fn cap(value: impl Into<String>) -> String {
    let value = value.into();
    if value.chars().count() <= FIELD_CAP {
        return value;
    }
    value.chars().take(FIELD_CAP).collect()
}

/// Cap an optional detail string, dropping `None`/empty.
fn cap_opt(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty()).map(cap)
}

/// Current wall-clock time in milliseconds since the Unix epoch (0 on error).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A cloneable, thread-safe, bounded ring buffer of [`DiagEntry`]s plus a
/// verbose toggle. Cloning shares the same underlying buffer and toggle, so the
/// process-global instance can be handed to every ACP session and read back by
/// the Settings UI.
#[derive(Clone)]
pub struct AgentDiagnosticsLog {
    entries: Arc<Mutex<VecDeque<DiagEntry>>>,
    verbose: Arc<AtomicBool>,
    capacity: usize,
}

impl Default for AgentDiagnosticsLog {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentDiagnosticsLog {
    /// Build a log with the default capacity ([`DEFAULT_CAPACITY`]).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Build a log retaining at most `capacity` entries (minimum 1).
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::new())),
            verbose: Arc::new(AtomicBool::new(false)),
            capacity: capacity.max(1),
        }
    }

    /// Enable or disable verbose (wire) capture.
    pub fn set_verbose(&self, verbose: bool) {
        self.verbose.store(verbose, Ordering::Relaxed);
    }

    /// Whether verbose (wire) capture is currently on.
    pub fn is_verbose(&self) -> bool {
        self.verbose.load(Ordering::Relaxed)
    }

    /// Record an error entry. Always retained, regardless of the verbose toggle.
    pub fn record_error(
        &self,
        agent: Option<&str>,
        summary: impl Into<String>,
        detail: Option<String>,
    ) {
        self.push(DiagKind::Error, agent, summary, detail);
    }

    /// Record a wire entry. A no-op unless verbose capture is on, keeping the
    /// hot path cheap when diagnostics are not being watched.
    pub fn record_wire(
        &self,
        agent: Option<&str>,
        summary: impl Into<String>,
        detail: Option<String>,
    ) {
        if !self.is_verbose() {
            return;
        }
        self.push(DiagKind::Wire, agent, summary, detail);
    }

    /// Append an entry, evicting the oldest when at capacity. A poisoned lock
    /// degrades to a dropped record (ADR 0009) rather than a panic.
    fn push(
        &self,
        kind: DiagKind,
        agent: Option<&str>,
        summary: impl Into<String>,
        detail: Option<String>,
    ) {
        let entry = DiagEntry {
            timestamp_ms: now_ms(),
            kind,
            agent: agent.map(|a| cap(a.to_string())),
            summary: cap(summary),
            detail: cap_opt(detail),
        };
        if let Ok(mut guard) = self.entries.lock() {
            while guard.len() >= self.capacity {
                guard.pop_front();
            }
            guard.push_back(entry);
        }
    }

    /// Return a snapshot of all retained entries, oldest first / newest last. A
    /// poisoned lock degrades to an empty snapshot rather than a panic.
    pub fn snapshot(&self) -> Vec<DiagEntry> {
        match self.entries.lock() {
            Ok(guard) => guard.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Drop every retained entry. A poisoned lock degrades to a no-op.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_recorded_even_when_not_verbose() {
        let log = AgentDiagnosticsLog::new();
        assert!(!log.is_verbose());
        log.record_error(Some("copilot"), "boom", Some("stack".to_string()));
        let snapshot = log.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].kind, DiagKind::Error);
        assert_eq!(snapshot[0].agent.as_deref(), Some("copilot"));
        assert_eq!(snapshot[0].summary, "boom");
        assert_eq!(snapshot[0].detail.as_deref(), Some("stack"));
    }

    #[test]
    fn wire_is_dropped_when_verbose_is_off_and_kept_when_on() {
        let log = AgentDiagnosticsLog::new();
        log.record_wire(None, "prompt", None);
        assert!(log.snapshot().is_empty());

        log.set_verbose(true);
        log.record_wire(None, "prompt", None);
        let snapshot = log.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].kind, DiagKind::Wire);
    }

    #[test]
    fn empty_detail_is_normalized_to_none() {
        let log = AgentDiagnosticsLog::new();
        log.record_error(None, "x", Some(String::new()));
        assert_eq!(log.snapshot()[0].detail, None);
    }

    #[test]
    fn bounded_capacity_evicts_oldest_entries() {
        let log = AgentDiagnosticsLog::with_capacity(3);
        for i in 0..5 {
            log.record_error(None, format!("e{i}"), None);
        }
        let snapshot = log.snapshot();
        assert_eq!(snapshot.len(), 3);
        // Oldest first / newest last: e0 and e1 evicted.
        let summaries: Vec<&str> = snapshot.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(summaries, vec!["e2", "e3", "e4"]);
    }

    #[test]
    fn capacity_is_at_least_one() {
        let log = AgentDiagnosticsLog::with_capacity(0);
        log.record_error(None, "a", None);
        log.record_error(None, "b", None);
        let snapshot = log.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].summary, "b");
    }

    #[test]
    fn snapshot_is_ordered_newest_last() {
        let log = AgentDiagnosticsLog::new();
        log.set_verbose(true);
        log.record_error(None, "first", None);
        log.record_wire(None, "second", None);
        log.record_error(None, "third", None);
        let snapshot = log.snapshot();
        let summaries: Vec<&str> = snapshot.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(summaries, vec!["first", "second", "third"]);
    }

    #[test]
    fn long_fields_are_capped() {
        let log = AgentDiagnosticsLog::new();
        let huge = "x".repeat(FIELD_CAP + 500);
        log.record_error(Some(&huge), huge.clone(), Some(huge.clone()));
        let entry = &log.snapshot()[0];
        assert_eq!(entry.summary.chars().count(), FIELD_CAP);
        assert_eq!(entry.agent.as_deref().unwrap().chars().count(), FIELD_CAP);
        assert_eq!(entry.detail.as_deref().unwrap().chars().count(), FIELD_CAP);
    }

    #[test]
    fn cap_respects_char_boundaries() {
        // Multi-byte characters must not be split mid-byte by the cap.
        let multibyte = "é".repeat(FIELD_CAP + 10);
        let capped = cap(multibyte);
        assert_eq!(capped.chars().count(), FIELD_CAP);
    }

    #[test]
    fn clear_drops_all_entries() {
        let log = AgentDiagnosticsLog::new();
        log.record_error(None, "a", None);
        log.clear();
        assert!(log.snapshot().is_empty());
    }

    #[test]
    fn serializes_with_camel_case_fields() {
        let entry = DiagEntry {
            timestamp_ms: 42,
            kind: DiagKind::Wire,
            agent: Some("copilot".to_string()),
            summary: "prompt".to_string(),
            detail: None,
        };
        let value = serde_json::to_value(&entry).unwrap();
        assert_eq!(value["timestampMs"], serde_json::json!(42));
        assert_eq!(value["kind"], serde_json::json!("wire"));
        assert_eq!(value["agent"], serde_json::json!("copilot"));
        assert_eq!(value["summary"], serde_json::json!("prompt"));
        assert_eq!(value["detail"], serde_json::Value::Null);
    }

    #[test]
    fn poisoned_lock_degrades_without_panicking() {
        let log = AgentDiagnosticsLog::new();
        log.record_error(None, "before", None);

        // Poison the mutex by panicking while holding the lock.
        let clone = log.clone();
        let _ = std::thread::spawn(move || {
            let _guard = clone.entries.lock().unwrap();
            panic!("poison the lock");
        })
        .join();

        // All operations must degrade gracefully rather than panic.
        log.record_error(None, "after", Some("d".to_string()));
        log.record_wire(None, "wire", None);
        assert!(log.snapshot().is_empty());
        log.clear();
    }
}
