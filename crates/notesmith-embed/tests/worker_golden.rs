//! Integration tests for the embed worker against real vault fixtures.
//!
//! Uses the shared `golden-vault/` fixture (happy path) and
//! `test-fixtures/malformed-vault/` (resilience / no-panic), per ADR 0009 and
//! the #248 acceptance criteria.

use std::path::PathBuf;

use notesmith_embed::{BruteForceStore, EmbeddingStore, Filter, HashEmbedder, VectorStore};
use tempfile::TempDir;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/notesmith-embed
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn embeds_golden_vault_and_search_returns_grounded_hits() {
    let vault = workspace_root().join("golden-vault");
    assert!(vault.exists(), "golden-vault fixture missing");
    let data = TempDir::new().unwrap();
    let store = EmbeddingStore::open(&data.path().join("embeddings.db")).unwrap();
    let emb = HashEmbedder::new(256);

    let worker = notesmith_embed::EmbedWorker::new("golden", &vault, &store, &emb);
    let report = worker.run().unwrap();
    assert!(report.embedded >= 20, "expected most notes embedded");
    assert_eq!(report.failed, 0, "golden vault should not fail any note");
    assert!(report.chunks_written >= report.embedded);

    // Second pass is fully incremental — nothing changed.
    let report2 = worker.run().unwrap();
    assert_eq!(report2.embedded, 0);
    assert_eq!(report2.skipped, report.embedded);

    // Search returns chunk refs with citation offsets we can slice back.
    let vs = BruteForceStore::new(std::sync::Arc::new(store));
    let query = emb_one(&emb, "customer stream migration");
    let hits = vs.search(&query, &Filter::vault("golden"), 5).unwrap();
    assert!(!hits.is_empty());
    for (chunk_ref, distance) in &hits {
        assert!(*distance >= 0.0, "raw distance is non-negative");
        assert!(chunk_ref.char_end >= chunk_ref.char_start);
        assert!(chunk_ref.path.ends_with(".md"));
    }
}

#[test]
fn malformed_vault_does_not_panic_and_embeds_degraded() {
    let vault = workspace_root().join("test-fixtures/malformed-vault");
    assert!(vault.exists(), "malformed-vault fixture missing");
    let data = TempDir::new().unwrap();
    let store = EmbeddingStore::open(&data.path().join("embeddings.db")).unwrap();
    let emb = HashEmbedder::new(128);

    let report = notesmith_embed::EmbedWorker::new("malformed", &vault, &store, &emb)
        .run()
        .unwrap();
    // Every malformed note degrades to a valid (possibly empty) chunk set; none
    // should hard-fail the batch.
    assert_eq!(report.failed, 0);
    assert!(report.embedded >= 1);
}

fn emb_one(emb: &HashEmbedder, text: &str) -> Vec<f32> {
    use notesmith_embed::Embedder;
    emb.embed(&[text.to_string()]).unwrap().remove(0)
}
