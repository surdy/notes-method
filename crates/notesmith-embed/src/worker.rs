//! The `notesmith embed` worker (ADR 0018 §2/§3/§8).
//!
//! The worker is the **sole writer** of `embeddings.db`. It walks the vault on
//! its own (decoupled from the daemon's rebuildable cache index), hashes each
//! note, and re-embeds only notes whose content changed since the last run
//! (incremental). Deleted notes have their chunks removed.
//!
//! Resilience (ADR 0009): each note is processed independently. A note that
//! fails to embed is logged (`WARN note=<path> stage=embed reason=<...>`) and
//! skipped; the batch continues. Per-note writes are transactional, so a bad
//! note never corrupts another note's chunks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use notesmith_core::types::{VaultName, VaultPath};
use walkdir::WalkDir;

use crate::chunker::{ChunkerOptions, chunk_note};
use crate::embedder::Embedder;
use crate::store::EmbeddingStore;
use crate::{Chunk, Result};

/// Summary of one worker pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkerReport {
    /// Notes re-embedded (new or changed).
    pub embedded: usize,
    /// Notes unchanged since last run (skipped).
    pub skipped: usize,
    /// Notes whose chunks were removed (file deleted).
    pub deleted: usize,
    /// Notes that errored and were skipped with a WARN.
    pub failed: usize,
    /// Total chunk rows written this pass.
    pub chunks_written: usize,
}

/// Keeps a vault's `embeddings.db` fresh.
pub struct EmbedWorker<'a> {
    vault_name: String,
    vault_root: PathBuf,
    store: &'a EmbeddingStore,
    embedder: &'a dyn Embedder,
    options: ChunkerOptions,
}

impl<'a> EmbedWorker<'a> {
    pub fn new(
        vault_name: impl Into<String>,
        vault_root: impl Into<PathBuf>,
        store: &'a EmbeddingStore,
        embedder: &'a dyn Embedder,
    ) -> Self {
        Self {
            vault_name: vault_name.into(),
            vault_root: vault_root.into(),
            store,
            embedder,
            options: ChunkerOptions::default(),
        }
    }

    /// Override chunker options (mostly for tests).
    pub fn with_options(mut self, options: ChunkerOptions) -> Self {
        self.options = options;
        self
    }

    /// Run one incremental pass over the vault.
    pub fn run(&self) -> Result<WorkerReport> {
        self.store
            .ensure_embedder(self.embedder.id(), self.embedder.dim())?;

        let stored: HashMap<String, String> = self
            .store
            .stored_hashes(&self.vault_name)?
            .into_iter()
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut report = WorkerReport::default();

        for entry in WalkDir::new(&self.vault_root)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !is_hidden(e.path()))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(error) => {
                    tracing::warn!(stage = "walk", reason = %error, "skipping unreadable entry");
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = match relative_path(&self.vault_root, entry.path()) {
                Some(rel) => rel,
                None => continue,
            };
            seen.insert(rel.clone());

            match self.process_note(entry.path(), &rel, &stored) {
                Ok(Outcome::Skipped) => report.skipped += 1,
                Ok(Outcome::Embedded { chunks }) => {
                    report.embedded += 1;
                    report.chunks_written += chunks;
                }
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(
                        note = %rel,
                        stage = "embed",
                        reason = %error,
                        "skipping note; batch continues"
                    );
                }
            }
        }

        // Notes present in the store but no longer on disk → delete.
        for path in stored.keys() {
            if !seen.contains(path) {
                if let Err(error) = self.store.delete_note(&self.vault_name, path) {
                    tracing::warn!(note = %path, stage = "delete", reason = %error, "delete failed");
                } else {
                    report.deleted += 1;
                }
            }
        }

        Ok(report)
    }

    fn process_note(
        &self,
        abs_path: &Path,
        rel: &str,
        stored: &HashMap<String, String>,
    ) -> Result<Outcome> {
        let bytes = std::fs::read(abs_path)?;
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        if stored.get(rel) == Some(&content_hash) {
            return Ok(Outcome::Skipped);
        }

        let content = String::from_utf8_lossy(&bytes);
        let note = notesmith_vault::parse_note(
            &VaultName::new(self.vault_name.clone()),
            &VaultPath::new(rel.to_string()),
            &content,
        );

        let spans = chunk_note(&note.body, &self.options);
        if spans.is_empty() {
            // No embeddable content — clear any stale chunks, count as embedded.
            self.store.replace_note_chunks(&self.vault_name, rel, &[])?;
            return Ok(Outcome::Embedded { chunks: 0 });
        }

        let texts: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
        let vectors = self.embedder.embed(&texts)?;
        if vectors.len() != spans.len() {
            return Err(crate::EmbedError::Embed(format!(
                "embedder returned {} vectors for {} chunks",
                vectors.len(),
                spans.len()
            )));
        }

        let chunks: Vec<Chunk> = spans
            .iter()
            .zip(vectors)
            .enumerate()
            .map(|(i, (span, vector))| Chunk {
                vault_name: self.vault_name.clone(),
                path: rel.to_string(),
                chunk_id: i as i64,
                char_start: span.char_start as i64,
                char_end: span.char_end as i64,
                media_ts_start: None,
                media_ts_end: None,
                content_hash: content_hash.clone(),
                vector,
            })
            .collect();

        let written = chunks.len();
        self.store
            .replace_note_chunks(&self.vault_name, rel, &chunks)?;
        Ok(Outcome::Embedded { chunks: written })
    }
}

enum Outcome {
    Skipped,
    Embedded { chunks: usize },
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for comp in rel.components() {
        parts.push(comp.as_os_str().to_str()?.to_string());
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::HashEmbedder;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn open_store(dir: &TempDir) -> EmbeddingStore {
        EmbeddingStore::open(&dir.path().join("data").join("embeddings.db")).unwrap()
    }

    #[test]
    fn embeds_all_notes_on_first_run() {
        let vault = TempDir::new().unwrap();
        write(vault.path(), "a.md", "# A\n\nalpha content here");
        write(vault.path(), "sub/b.md", "# B\n\nbeta content here");
        let data = TempDir::new().unwrap();
        let store = open_store(&data);
        let emb = HashEmbedder::new(64);

        let worker = EmbedWorker::new("v", vault.path(), &store, &emb);
        let report = worker.run().unwrap();
        assert_eq!(report.embedded, 2);
        assert_eq!(report.skipped, 0);
        assert!(report.chunks_written >= 2);
        assert_eq!(
            store.chunk_count("v").unwrap(),
            report.chunks_written as i64
        );
    }

    #[test]
    fn second_run_skips_unchanged_and_reembeds_changed() {
        let vault = TempDir::new().unwrap();
        write(vault.path(), "a.md", "alpha content");
        write(vault.path(), "b.md", "beta content");
        let data = TempDir::new().unwrap();
        let store = open_store(&data);
        let emb = HashEmbedder::new(64);

        EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();

        // Change only a.md.
        write(vault.path(), "a.md", "alpha content changed a lot now");
        let report = EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();
        assert_eq!(report.embedded, 1, "only changed note re-embedded");
        assert_eq!(report.skipped, 1, "unchanged note skipped");
    }

    #[test]
    fn deleted_note_chunks_are_removed() {
        let vault = TempDir::new().unwrap();
        write(vault.path(), "a.md", "alpha");
        write(vault.path(), "b.md", "beta");
        let data = TempDir::new().unwrap();
        let store = open_store(&data);
        let emb = HashEmbedder::new(64);
        EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();

        std::fs::remove_file(vault.path().join("b.md")).unwrap();
        let report = EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();
        assert_eq!(report.deleted, 1);
        assert_eq!(store.stored_hashes("v").unwrap().len(), 1);
    }

    #[test]
    fn hidden_dirs_are_skipped() {
        let vault = TempDir::new().unwrap();
        write(vault.path(), "a.md", "alpha");
        write(vault.path(), ".notesmith/templates/t.md", "template");
        write(vault.path(), ".git/config.md", "gitish");
        let data = TempDir::new().unwrap();
        let store = open_store(&data);
        let emb = HashEmbedder::new(64);
        let report = EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();
        assert_eq!(report.embedded, 1);
    }

    /// Embedder that fails for notes containing a poison string, to exercise the
    /// per-note resilience path.
    struct FlakyEmbedder {
        inner: HashEmbedder,
        failures: Mutex<usize>,
    }
    impl Embedder for FlakyEmbedder {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            if texts.iter().any(|t| t.contains("BOOM")) {
                *self.failures.lock().unwrap() += 1;
                return Err(crate::EmbedError::Embed("boom".into()));
            }
            self.inner.embed(texts)
        }
        fn id(&self) -> &str {
            self.inner.id()
        }
        fn dim(&self) -> usize {
            self.inner.dim()
        }
    }

    #[test]
    fn failing_note_is_skipped_and_batch_continues() {
        let vault = TempDir::new().unwrap();
        write(vault.path(), "good.md", "perfectly fine note");
        write(vault.path(), "bad.md", "this note goes BOOM");
        let data = TempDir::new().unwrap();
        let store = open_store(&data);
        let emb = FlakyEmbedder {
            inner: HashEmbedder::new(64),
            failures: Mutex::new(0),
        };
        let report = EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();
        assert_eq!(report.embedded, 1);
        assert_eq!(report.failed, 1);
        assert!(*emb.failures.lock().unwrap() >= 1);
    }

    #[test]
    fn malformed_frontmatter_does_not_panic() {
        let vault = TempDir::new().unwrap();
        write(
            vault.path(),
            "broken.md",
            "---\nkey: [unclosed\n  nested: {{ bad\n---\n\n# Body\n\nreal content survives",
        );
        let data = TempDir::new().unwrap();
        let store = open_store(&data);
        let emb = HashEmbedder::new(64);
        let report = EmbedWorker::new("v", vault.path(), &store, &emb)
            .run()
            .unwrap();
        assert_eq!(report.embedded, 1);
        assert_eq!(report.failed, 0);
    }
}
