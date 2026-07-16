//! notesmith-ingest: local drop-folder ingestion worker
//! ([ADR 0022](../../docs/adr/0022-local-drop-folder-ingestion.md), issue #263).
//!
//! A colocated worker scans a per-vault **raw drop folder** for documents an
//! external tool has dropped in, extracts their text (reusing
//! [`notesmith_document`]), and writes a provenance-tracked **sidecar note** for
//! each — while leaving the raw file untouched (keep-in-place invariant, ADR
//! 0022 §2). The sidecar note is the durable processed-state ledger (§4):
//! identity and staleness are derived from `(source_path + source_hash)`, never
//! from tags (§3, §5).
//!
//! Per [ADR 0009](../../docs/adr/0009-resilience-to-malformed-content.md) every
//! dropped file is untrusted: extraction is panic-isolated inside
//! `notesmith-document`, and a per-item failure logs a `WARN` and skips without
//! aborting the batch (§9).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use notesmith_core::{VaultEngine, VaultPath};
use notesmith_document::{ChunkOptions, DocumentError, DocumentKind, parse_document};
use notesmith_vault::{NativeVaultEngine, apply_save_pipeline};
use sha2::{Digest, Sha256};

/// Frontmatter `status` for a successfully extracted document.
pub const STATUS_INGESTED: &str = "ingested";
/// Frontmatter `status` for a transient failure that should be retried.
pub const STATUS_FAILED: &str = "failed";
/// Frontmatter `status` for a document that cannot be extracted at all.
pub const STATUS_UNSUPPORTED: &str = "unsupported";

/// A fatal error that prevents the ingest pass from running at all (as opposed
/// to a per-item failure, which is recorded in the report and never aborts).
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// The vault could not be scanned for existing sidecar notes.
    #[error("failed to scan vault: {0}")]
    Scan(String),
    /// A generated sidecar note could not be written.
    #[error("failed to write note {path}: {reason}")]
    Write { path: String, reason: String },
}

/// The outcome of processing a single raw file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// A new document was extracted into a fresh sidecar note.
    Ingested,
    /// An existing sidecar note was up to date (hash match); nothing written.
    Unchanged,
    /// The raw file changed (new hash) or previously failed; re-extracted.
    Reingested,
    /// Same content re-appeared at a new path; the note was moved without
    /// re-extraction (ADR 0022 §3).
    Renamed,
    /// Extraction failed transiently; recorded `status: failed`, retried next
    /// tick.
    Failed,
    /// The file type is not extractable; recorded `status: unsupported`, not
    /// retried while its content is unchanged.
    Unsupported,
}

/// A single processed item and where its sidecar note lives.
#[derive(Debug, Clone)]
pub struct IngestItem {
    /// Vault-relative path of the raw file.
    pub source_path: String,
    /// Vault-relative path of the generated sidecar note.
    pub note_path: String,
    /// What happened to this item.
    pub outcome: ItemOutcome,
}

/// The result of one ingest pass over a vault.
#[derive(Debug, Clone, Default)]
pub struct IngestReport {
    /// Every raw file processed this pass, in scan order.
    pub items: Vec<IngestItem>,
    /// Sidecar notes whose raw file no longer exists (stale/orphaned). Reported,
    /// never deleted (keep-in-place is about raw files; notes are the user's).
    pub orphaned: Vec<String>,
}

impl IngestReport {
    fn count(&self, outcome: ItemOutcome) -> usize {
        self.items.iter().filter(|i| i.outcome == outcome).count()
    }
    /// Newly extracted documents.
    pub fn ingested(&self) -> usize {
        self.count(ItemOutcome::Ingested)
    }
    /// Up-to-date documents skipped.
    pub fn unchanged(&self) -> usize {
        self.count(ItemOutcome::Unchanged)
    }
    /// Changed/failed documents re-extracted.
    pub fn reingested(&self) -> usize {
        self.count(ItemOutcome::Reingested)
    }
    /// Documents whose note was moved to follow a raw-file rename.
    pub fn renamed(&self) -> usize {
        self.count(ItemOutcome::Renamed)
    }
    /// Transient failures (retried next tick).
    pub fn failed(&self) -> usize {
        self.count(ItemOutcome::Failed)
    }
    /// Unextractable file types.
    pub fn unsupported(&self) -> usize {
        self.count(ItemOutcome::Unsupported)
    }
}

/// Existing sidecar-note state discovered by scanning the vault.
#[derive(Debug, Clone)]
struct Sidecar {
    note_path: String,
    source_path: String,
    source_hash: String,
    status: String,
    body: String,
}

/// One raw file found in the drop folder.
struct RawFile {
    /// Vault-relative path, e.g. `raw/talk.pdf`.
    rel_path: String,
    /// Absolute path on disk.
    abs_path: PathBuf,
    /// Inferred document kind, or `None` for unsupported extensions.
    kind: Option<DocumentKind>,
}

/// The drop-folder ingestion worker for a single vault.
pub struct IngestWorker {
    vault_root: PathBuf,
    raw_dir: String,
    notes_dir: String,
    now: DateTime<Utc>,
}

impl IngestWorker {
    /// Build a worker for `vault_root` using the given raw/notes folders.
    pub fn new(vault_root: impl Into<PathBuf>, raw_dir: &str, notes_dir: &str) -> Self {
        Self {
            vault_root: vault_root.into(),
            raw_dir: normalize_dir(raw_dir),
            notes_dir: normalize_dir(notes_dir),
            now: Utc::now(),
        }
    }

    /// Override the ingest timestamp (for deterministic tests).
    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }

    /// Run one incremental ingest pass. Never panics on bad input; per-item
    /// failures are recorded in the report rather than aborting the batch.
    pub fn run(&self) -> Result<IngestReport, IngestError> {
        let engine = NativeVaultEngine;
        let sidecars = self.scan_sidecars(&engine)?;
        let by_source_path: HashMap<&str, &Sidecar> = sidecars
            .iter()
            .map(|s| (s.source_path.as_str(), s))
            .collect();
        let by_hash: HashMap<&str, &Sidecar> = sidecars
            .iter()
            .map(|s| (s.source_hash.as_str(), s))
            .collect();

        let raw_files = self.scan_raw_files();
        let raw_set: std::collections::HashSet<&str> =
            raw_files.iter().map(|r| r.rel_path.as_str()).collect();

        let mut report = IngestReport::default();

        for raw in &raw_files {
            let outcome = self.process_raw(
                &engine,
                raw,
                &by_source_path,
                &by_hash,
                &raw_set,
                &mut report,
            );
            match outcome {
                Ok(()) => {}
                Err(error) => return Err(error),
            }
        }

        for sidecar in &sidecars {
            if !raw_set.contains(sidecar.source_path.as_str()) {
                report.orphaned.push(sidecar.note_path.clone());
            }
        }
        report.orphaned.sort();

        Ok(report)
    }

    fn process_raw(
        &self,
        engine: &NativeVaultEngine,
        raw: &RawFile,
        by_source_path: &HashMap<&str, &Sidecar>,
        by_hash: &HashMap<&str, &Sidecar>,
        raw_set: &std::collections::HashSet<&str>,
        report: &mut IngestReport,
    ) -> Result<(), IngestError> {
        let bytes = match std::fs::read(&raw.abs_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(
                    item = %raw.rel_path,
                    stage = "read",
                    reason = %error,
                    "skipping unreadable raw file"
                );
                report.items.push(IngestItem {
                    source_path: raw.rel_path.clone(),
                    note_path: self.note_path_for(&raw.rel_path),
                    outcome: ItemOutcome::Failed,
                });
                return Ok(());
            }
        };
        let current_hash = hash_bytes(&bytes);
        let expected_note = self.note_path_for(&raw.rel_path);

        // Case 1: a note already tracks this exact raw path.
        if let Some(sidecar) = by_source_path.get(raw.rel_path.as_str()) {
            let up_to_date = sidecar.source_hash == current_hash && sidecar.status != STATUS_FAILED;
            if up_to_date {
                report.items.push(IngestItem {
                    source_path: raw.rel_path.clone(),
                    note_path: sidecar.note_path.clone(),
                    outcome: ItemOutcome::Unchanged,
                });
                return Ok(());
            }
            let outcome =
                self.extract_and_write(engine, raw, &bytes, &current_hash, &sidecar.note_path)?;
            report.items.push(IngestItem {
                source_path: raw.rel_path.clone(),
                note_path: sidecar.note_path.clone(),
                outcome: promote_to_reingest(outcome),
            });
            return Ok(());
        }

        // Case 2: same content seen before at a path that no longer exists —
        // this is a rename/move; reuse the extracted body, don't re-extract.
        if let Some(sidecar) = by_hash.get(current_hash.as_str()) {
            if !raw_set.contains(sidecar.source_path.as_str()) {
                self.rename_note(engine, sidecar, raw, &current_hash, &expected_note)?;
                report.items.push(IngestItem {
                    source_path: raw.rel_path.clone(),
                    note_path: expected_note,
                    outcome: ItemOutcome::Renamed,
                });
                return Ok(());
            }
        }

        // Case 3: a genuinely new raw file.
        let outcome = self.extract_and_write(engine, raw, &bytes, &current_hash, &expected_note)?;
        report.items.push(IngestItem {
            source_path: raw.rel_path.clone(),
            note_path: expected_note,
            outcome,
        });
        Ok(())
    }

    /// Extract `raw` and write its sidecar note. Returns the base outcome
    /// (`Ingested` / `Failed` / `Unsupported`); callers map `Ingested` to
    /// `Reingested` when updating an existing note.
    fn extract_and_write(
        &self,
        engine: &NativeVaultEngine,
        raw: &RawFile,
        bytes: &[u8],
        current_hash: &str,
        note_path: &str,
    ) -> Result<ItemOutcome, IngestError> {
        let mtime = file_mtime(&raw.abs_path);
        let filename = file_name(&raw.rel_path);

        let Some(_kind) = raw.kind else {
            // Unsupported extension: record once so it is not retried forever.
            tracing::warn!(
                item = %raw.rel_path,
                stage = "detect",
                reason = "unsupported file type",
                "recording unsupported raw file"
            );
            let note = self.render_note(
                filename,
                &file_extension(&raw.rel_path),
                &raw.rel_path,
                current_hash,
                mtime,
                STATUS_UNSUPPORTED,
                None,
                Some("unsupported file type"),
                "",
            );
            self.write_note(engine, note_path, &note)?;
            return Ok(ItemOutcome::Unsupported);
        };

        match parse_document(filename, bytes, &ChunkOptions::default()) {
            Ok(parsed) => {
                let title = parsed
                    .meta
                    .title
                    .clone()
                    .unwrap_or_else(|| stem(filename).to_string());
                let unit_field = format!("{}_count", parsed.meta.unit_label);
                let note = self.render_note(
                    &title,
                    &parsed.meta.source_type,
                    &raw.rel_path,
                    current_hash,
                    mtime,
                    STATUS_INGESTED,
                    Some((unit_field, parsed.meta.unit_count)),
                    None,
                    &parsed.text,
                );
                self.write_note(engine, note_path, &note)?;
                Ok(ItemOutcome::Ingested)
            }
            Err(error) => {
                let (status, outcome) = classify_error(&error);
                tracing::warn!(
                    item = %raw.rel_path,
                    stage = "extract",
                    reason = %error,
                    status,
                    "recording failed/unsupported document"
                );
                let note = self.render_note(
                    stem(filename),
                    _kind.source_type(),
                    &raw.rel_path,
                    current_hash,
                    mtime,
                    status,
                    None,
                    Some(&error.to_string()),
                    "",
                );
                self.write_note(engine, note_path, &note)?;
                Ok(outcome)
            }
        }
    }

    /// Move a sidecar note to follow a raw-file rename, reusing the note body so
    /// the document is not re-extracted (ADR 0022 §3).
    fn rename_note(
        &self,
        engine: &NativeVaultEngine,
        sidecar: &Sidecar,
        raw: &RawFile,
        current_hash: &str,
        new_note_path: &str,
    ) -> Result<(), IngestError> {
        let mtime = file_mtime(&raw.abs_path);
        let note = self.render_note_with_body(
            file_name(&raw.rel_path),
            &raw.rel_path,
            current_hash,
            mtime,
            &sidecar.status,
            &sidecar.body,
        );
        self.write_note(engine, new_note_path, &note)?;
        if sidecar.note_path != new_note_path {
            let _ = engine.delete(&self.vault_root, &VaultPath::new(sidecar.note_path.clone()));
        }
        Ok(())
    }

    fn write_note(
        &self,
        engine: &NativeVaultEngine,
        note_path: &str,
        content: &str,
    ) -> Result<(), IngestError> {
        let finalized = apply_save_pipeline(content);
        engine
            .write(
                &self.vault_root,
                &VaultPath::new(note_path.to_string()),
                None,
                &finalized,
            )
            .map_err(|error| IngestError::Write {
                path: note_path.to_string(),
                reason: error.to_string(),
            })?;
        Ok(())
    }

    /// Render a full sidecar note (provenance frontmatter + extracted body).
    #[allow(clippy::too_many_arguments)]
    fn render_note(
        &self,
        title: &str,
        source_type: &str,
        source_path: &str,
        source_hash: &str,
        source_mtime: Option<DateTime<Utc>>,
        status: &str,
        unit: Option<(String, usize)>,
        reason: Option<&str>,
        body: &str,
    ) -> String {
        let mut fm = serde_yaml::Mapping::new();
        fm_put(&mut fm, "title", title.into());
        fm_put(&mut fm, "source_type", source_type.into());
        fm_put(&mut fm, "source_path", source_path.into());
        fm_put(&mut fm, "source_hash", source_hash.into());
        if let Some(mtime) = source_mtime {
            fm_put(&mut fm, "source_mtime", rfc3339(mtime).into());
        }
        fm_put(&mut fm, "ingested_at", rfc3339(self.now).into());
        fm_put(&mut fm, "status", status.into());
        if let Some((field, count)) = unit {
            fm_put(&mut fm, &field, (count as u64).into());
        }
        if let Some(reason) = reason {
            fm_put(&mut fm, "reason", reason.into());
        }
        let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(fm)).unwrap_or_default();
        format!("---\n{yaml}---\n\n# {title}\n\n{body}\n")
    }

    fn render_note_with_body(
        &self,
        title: &str,
        source_path: &str,
        source_hash: &str,
        source_mtime: Option<DateTime<Utc>>,
        status: &str,
        body: &str,
    ) -> String {
        // Reused on rename: keep the original heading/body, refresh provenance.
        let mut fm = serde_yaml::Mapping::new();
        fm_put(&mut fm, "title", title.into());
        fm_put(&mut fm, "source_path", source_path.into());
        fm_put(&mut fm, "source_hash", source_hash.into());
        if let Some(mtime) = source_mtime {
            fm_put(&mut fm, "source_mtime", rfc3339(mtime).into());
        }
        fm_put(&mut fm, "ingested_at", rfc3339(self.now).into());
        fm_put(&mut fm, "status", status.into());
        let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(fm)).unwrap_or_default();
        format!("---\n{yaml}---\n\n{body}\n")
    }

    /// Deterministic sidecar note path for a raw file: mirror its path under the
    /// notes folder with the extension replaced by `.md`.
    fn note_path_for(&self, raw_rel: &str) -> String {
        let without_prefix = raw_rel
            .strip_prefix(&format!("{}/", self.raw_dir))
            .unwrap_or(raw_rel);
        let base = match without_prefix.rsplit_once('.') {
            Some((stem, _ext)) => stem,
            None => without_prefix,
        };
        format!("{}/{}.md", self.notes_dir, base)
    }

    fn scan_sidecars(&self, engine: &NativeVaultEngine) -> Result<Vec<Sidecar>, IngestError> {
        let notes = engine
            .scan(&self.vault_root)
            .map_err(|error| IngestError::Scan(error.to_string()))?;
        let mut sidecars = Vec::new();
        for note in notes {
            let Some(frontmatter) = &note.frontmatter else {
                continue;
            };
            let Some(source_path) = frontmatter.get_str("source_path") else {
                continue;
            };
            // Only treat notes generated by this worker (pointing into raw_dir).
            if !source_path.starts_with(&format!("{}/", self.raw_dir)) {
                continue;
            }
            let source_hash = frontmatter.get_str("source_hash").unwrap_or_default();
            let status = frontmatter
                .get_str("status")
                .unwrap_or(STATUS_INGESTED)
                .to_string();
            sidecars.push(Sidecar {
                note_path: note.path.to_string(),
                source_path: source_path.to_string(),
                source_hash: source_hash.to_string(),
                status,
                body: note.body.clone(),
            });
        }
        Ok(sidecars)
    }

    fn scan_raw_files(&self) -> Vec<RawFile> {
        let raw_root = self.vault_root.join(&self.raw_dir);
        if !raw_root.is_dir() {
            return Vec::new();
        }
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&raw_root)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if name.starts_with('.') {
                continue; // skip dotfiles (.DS_Store, partial uploads, etc.)
            }
            let Ok(rel) = entry.path().strip_prefix(&self.vault_root) else {
                continue;
            };
            let Some(rel_str) = rel.to_str() else {
                continue; // non-UTF-8 path: skip
            };
            let rel_path = rel_str.replace('\\', "/");
            let kind = DocumentKind::from_filename(&rel_path);
            files.push(RawFile {
                rel_path,
                abs_path: entry.path().to_path_buf(),
                kind,
            });
        }
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        files
    }
}

fn promote_to_reingest(outcome: ItemOutcome) -> ItemOutcome {
    match outcome {
        ItemOutcome::Ingested => ItemOutcome::Reingested,
        other => other,
    }
}

/// Map a parse error to a `(status, outcome)`. Malformed input is retryable
/// (`failed`); anything unextractable is terminal (`unsupported`).
fn classify_error(error: &DocumentError) -> (&'static str, ItemOutcome) {
    match error {
        DocumentError::Parse { .. } => (STATUS_FAILED, ItemOutcome::Failed),
        DocumentError::Empty | DocumentError::Encrypted | DocumentError::Unsupported(_) => {
            (STATUS_UNSUPPORTED, ItemOutcome::Unsupported)
        }
    }
}

fn fm_put(fm: &mut serde_yaml::Mapping, key: &str, value: serde_yaml::Value) {
    fm.insert(serde_yaml::Value::from(key), value);
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn rfc3339(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn file_mtime(path: &Path) -> Option<DateTime<Utc>> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(modified))
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name)
}

fn file_extension(rel: &str) -> String {
    let name = file_name(rel);
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

fn normalize_dir(dir: &str) -> String {
    dir.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_path_mirrors_raw_path_under_notes_dir() {
        let worker = IngestWorker::new("/vault", "raw", "ingested");
        assert_eq!(worker.note_path_for("raw/talk.pdf"), "ingested/talk.md");
        assert_eq!(worker.note_path_for("raw/sub/a.epub"), "ingested/sub/a.md");
    }

    #[test]
    fn hash_is_sha256_prefixed_and_stable() {
        let a = hash_bytes(b"hello");
        assert!(a.starts_with("sha256:"));
        assert_eq!(a, hash_bytes(b"hello"));
        assert_ne!(a, hash_bytes(b"world"));
    }

    #[test]
    fn parse_error_classification() {
        assert_eq!(
            classify_error(&DocumentError::Parse {
                kind: "pdf",
                reason: "boom".into()
            })
            .1,
            ItemOutcome::Failed
        );
        assert_eq!(
            classify_error(&DocumentError::Empty).1,
            ItemOutcome::Unsupported
        );
        assert_eq!(
            classify_error(&DocumentError::Encrypted).1,
            ItemOutcome::Unsupported
        );
    }

    #[test]
    fn normalize_dir_strips_slashes() {
        assert_eq!(normalize_dir("/raw/"), "raw");
        assert_eq!(normalize_dir("ingested"), "ingested");
    }
}
