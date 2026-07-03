//! Golden-vault integration test for hybrid (lexical + semantic) search (#199).
//!
//! Builds real backends over the shared `golden-vault/` fixture — a Tantivy
//! [`SearchIndex`], an on-disk note index (`cache.sqlite`), and a real
//! `embeddings.db` produced by the embed worker — then drives [`HybridSearch`]
//! end to end, asserting the RRF fusion returns grounded (path + snippet) hits.

use std::sync::Arc;

use notesmith_core::{Note, VaultEngine};
use notesmith_embed::{EmbeddingSearch, EmbeddingStore, HashEmbedder};
use notesmith_index::SearchIndex;
use notesmith_ops::HybridSearch;
use notesmith_vault::NativeVaultEngine;
use tempfile::TempDir;

const VAULT: &str = "golden";
const DIM: usize = 256;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn load_notes() -> Vec<Note> {
    NativeVaultEngine.scan(&golden_vault()).unwrap()
}

#[test]
fn hybrid_search_over_golden_vault_returns_grounded_hits() {
    let notes = load_notes();

    // Lexical backend.
    let search_index = Arc::new(SearchIndex::open_in_memory().unwrap());
    search_index.reindex(VAULT, &notes).unwrap();

    // Semantic backend: real embeddings.db built by the worker.
    let data = TempDir::new().unwrap();
    let db_path = data.path().join("embeddings.db");
    let store = EmbeddingStore::open(&db_path).unwrap();
    let embedder = HashEmbedder::new(DIM);
    notesmith_embed::EmbedWorker::new(VAULT, golden_vault(), &store, &embedder)
        .run()
        .unwrap();
    drop(store);

    // On-disk note index so EmbeddingSearch can ATTACH it for metadata filters.
    let cache_path = data.path().join("cache.sqlite");
    let cache = notesmith_index::VaultCache::open(&cache_path).unwrap();
    cache.reindex(VAULT, &notes).unwrap();
    drop(cache);

    let query_embedder: Arc<dyn notesmith_embed::Embedder> = Arc::new(HashEmbedder::new(DIM));
    let embedding =
        Arc::new(EmbeddingSearch::open(VAULT, &db_path, &cache_path, query_embedder).unwrap());

    let hybrid = HybridSearch::new(search_index, embedding, golden_vault());

    let hits = hybrid.search("Acme", 10).unwrap();
    assert!(!hits.is_empty(), "hybrid search returned no hits");

    // Every hit is grounded: has a path, and a non-empty snippet.
    for hit in &hits {
        assert!(!hit.path.is_empty());
        assert!(
            !hit.snippet.trim().is_empty(),
            "hit {} has an empty snippet",
            hit.path
        );
        assert!(hit.lexical_rank.is_some() || hit.semantic_rank.is_some());
    }

    // The Acme customer note should surface for an "Acme" query.
    assert!(
        hits.iter().any(|h| h.path == "Customers/Acme/Acme Corp.md"),
        "expected Acme Corp note in hybrid results, got: {:?}",
        hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );
}
