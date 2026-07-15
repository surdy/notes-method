//! Per-vault accumulator of parse warnings surfaced through `GET /api/status`.
//!
//! Per [ADR 0009](../../docs/adr/0009-resilience-to-malformed-content.md),
//! malformed note content is skipped/degraded silently (a note with broken YAML
//! frontmatter parses with `frontmatter: None` rather than failing indexing).
//! Users need to see *which* notes were affected without grepping daemon logs,
//! so each vault keeps a bounded, most-recent-wins list of warnings that the
//! status endpoint reports (issue #92).
//!
//! The accumulator is keyed by note path: re-recording a path replaces its
//! prior warning, and fixing a note clears it, so the list always reflects the
//! current state of the vault rather than a growing history.

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use notesmith_core::Note;
use serde::Serialize;

/// The maximum number of distinct warnings retained per vault. Older warnings
/// beyond this bound are dropped and the snapshot reports `truncated: true`.
pub const MAX_PARSE_WARNINGS: usize = 100;

/// A single note that degraded during parsing/indexing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteWarning {
    /// Vault-relative path of the offending note.
    pub path: String,
    /// The pipeline stage that degraded (e.g. `frontmatter`).
    pub stage: String,
    /// Human-readable reason (e.g. the YAML parse error).
    pub reason: String,
    /// When the warning was (most recently) recorded.
    pub occurred_at: DateTime<Utc>,
}

/// A point-in-time view of a vault's parse warnings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseWarningsSnapshot {
    /// Number of retained warnings.
    pub count: usize,
    /// `true` when warnings were dropped due to the [`MAX_PARSE_WARNINGS`] bound.
    pub truncated: bool,
    /// The retained warnings, oldest first.
    pub warnings: Vec<NoteWarning>,
}

/// A bounded, path-keyed, thread-safe accumulator of [`NoteWarning`]s.
#[derive(Debug, Default)]
pub struct ParseWarnings {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    warnings: VecDeque<NoteWarning>,
    truncated: bool,
}

impl ParseWarnings {
    /// Create an empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or replace) the warning for a note path. A path already present
    /// is updated in place, preserving list order; a new path is appended and
    /// may evict the oldest entry (setting the truncated flag).
    pub fn record(&self, warning: NoteWarning) {
        let mut inner = self.inner.lock().expect("parse-warnings mutex poisoned");
        if let Some(existing) = inner.warnings.iter_mut().find(|w| w.path == warning.path) {
            *existing = warning;
            return;
        }
        inner.warnings.push_back(warning);
        while inner.warnings.len() > MAX_PARSE_WARNINGS {
            inner.warnings.pop_front();
            inner.truncated = true;
        }
    }

    /// Clear any warning recorded for `path` (e.g. after the note is fixed and
    /// re-indexed). No-op when the path has no warning.
    pub fn clear_path(&self, path: &str) {
        let mut inner = self.inner.lock().expect("parse-warnings mutex poisoned");
        inner.warnings.retain(|w| w.path != path);
    }

    /// Replace the entire set from a full (re)scan of a vault's notes. Resets
    /// the truncated flag before applying the bound to the new set.
    pub fn replace_all<I>(&self, warnings: I)
    where
        I: IntoIterator<Item = NoteWarning>,
    {
        let mut inner = self.inner.lock().expect("parse-warnings mutex poisoned");
        inner.warnings.clear();
        inner.truncated = false;
        for warning in warnings {
            inner.warnings.push_back(warning);
            while inner.warnings.len() > MAX_PARSE_WARNINGS {
                inner.warnings.pop_front();
                inner.truncated = true;
            }
        }
    }

    /// Update the accumulator for a single note: record a warning when the note
    /// degraded, or clear a stale warning when it now parses cleanly.
    pub fn update_for_note(&self, note: &Note, now: DateTime<Utc>) {
        match note_parse_warning(note, now) {
            Some(warning) => self.record(warning),
            None => self.clear_path(note.path.as_str()),
        }
    }

    /// Take a point-in-time snapshot for the status endpoint.
    pub fn snapshot(&self) -> ParseWarningsSnapshot {
        let inner = self.inner.lock().expect("parse-warnings mutex poisoned");
        ParseWarningsSnapshot {
            count: inner.warnings.len(),
            truncated: inner.truncated,
            warnings: inner.warnings.iter().cloned().collect(),
        }
    }
}

/// Detect whether a parsed note degraded, returning a [`NoteWarning`] if so.
///
/// The [`Note`] carries both the raw frontmatter block (`raw_frontmatter`,
/// `Some` when a `---` block was present) and the parsed frontmatter
/// (`frontmatter`, `None` when `serde_yaml` rejected it). When a raw block
/// exists but parsing produced nothing, the frontmatter was malformed and was
/// silently dropped by the parser — exactly the ADR 0009 degradation we want to
/// surface. Re-parsing the raw block yields a precise reason.
pub fn note_parse_warning(note: &Note, now: DateTime<Utc>) -> Option<NoteWarning> {
    let raw = note.raw_frontmatter.as_deref()?;
    if note.frontmatter.is_some() {
        return None;
    }
    // A raw `---` block existed but did not parse. Recover the reason.
    let reason = serde_yaml::from_str::<notesmith_core::Frontmatter>(raw)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "malformed frontmatter".to_string());
    Some(NoteWarning {
        path: note.path.as_str().to_string(),
        stage: "frontmatter".to_string(),
        reason,
        occurred_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_core::{VaultName, VaultPath};
    use notesmith_vault::parse_note;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn note(path: &str, content: &str) -> Note {
        parse_note(&VaultName::new("work"), &VaultPath::new(path), content)
    }

    const MALFORMED: &str = "---\ntitle: [unterminated\n---\n\nbody\n";
    const VALID: &str = "---\ntitle: Fine\n---\n\nbody\n";

    #[test]
    fn detects_malformed_frontmatter() {
        let n = note("Bad.md", MALFORMED);
        let w = note_parse_warning(&n, now()).expect("malformed note should warn");
        assert_eq!(w.path, "Bad.md");
        assert_eq!(w.stage, "frontmatter");
        assert!(!w.reason.is_empty());
    }

    #[test]
    fn clean_note_yields_no_warning() {
        assert!(note_parse_warning(&note("Good.md", VALID), now()).is_none());
        // A note with no frontmatter block at all is not a warning.
        assert!(note_parse_warning(&note("Plain.md", "# Just body\n"), now()).is_none());
    }

    #[test]
    fn record_is_idempotent_per_path() {
        let acc = ParseWarnings::new();
        acc.update_for_note(&note("Bad.md", MALFORMED), now());
        acc.update_for_note(&note("Bad.md", MALFORMED), now());
        let snap = acc.snapshot();
        assert_eq!(snap.count, 1);
        assert!(!snap.truncated);
    }

    #[test]
    fn fixing_a_note_clears_its_warning() {
        let acc = ParseWarnings::new();
        acc.update_for_note(&note("Bad.md", MALFORMED), now());
        assert_eq!(acc.snapshot().count, 1);
        acc.update_for_note(&note("Bad.md", VALID), now());
        assert_eq!(acc.snapshot().count, 0);
    }

    #[test]
    fn bound_is_enforced_and_reports_truncation() {
        let acc = ParseWarnings::new();
        for i in 0..(MAX_PARSE_WARNINGS + 1) {
            acc.update_for_note(&note(&format!("Bad{i}.md"), MALFORMED), now());
        }
        let snap = acc.snapshot();
        assert_eq!(snap.count, MAX_PARSE_WARNINGS);
        assert!(snap.truncated);
        // Oldest (Bad0.md) evicted; newest retained.
        assert!(!snap.warnings.iter().any(|w| w.path == "Bad0.md"));
        assert!(
            snap.warnings
                .iter()
                .any(|w| w.path == format!("Bad{MAX_PARSE_WARNINGS}.md"))
        );
    }

    #[test]
    fn replace_all_recomputes_from_scan() {
        let acc = ParseWarnings::new();
        acc.update_for_note(&note("Old.md", MALFORMED), now());
        let notes = [note("A.md", MALFORMED), note("B.md", VALID)];
        let n = now();
        acc.replace_all(notes.iter().filter_map(|note| note_parse_warning(note, n)));
        let snap = acc.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.warnings[0].path, "A.md");
    }
}
