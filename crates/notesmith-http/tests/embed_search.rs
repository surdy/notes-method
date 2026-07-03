//! Integration tests for the daemon-side embedding search primitive (#249).
//!
//! Frontend-backend contract style: builds a *real* `embeddings.db` (via the
//! worker) and a *real* note index (`cache.sqlite`), then runs the exact
//! ATTACH-JOIN query the daemon uses to filter by metadata.

use std::sync::Arc;

use notesmith_core::types::{VaultName, VaultPath};
use notesmith_embed::{EmbeddingSearch, EmbeddingStore, HashEmbedder, MetaFilter};
use notesmith_index::VaultCache;
use tempfile::TempDir;

const VAULT: &str = "contract";

fn write_note(root: &std::path::Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Build a real cache.sqlite index over the vault so tag JOINs have data.
fn build_index(cache_path: &std::path::Path, vault_root: &std::path::Path) {
    let cache = VaultCache::open(cache_path).unwrap();
    let mut notes = Vec::new();
    for entry in walkdir::WalkDir::new(vault_root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|e| e.to_str()) == Some("md")
        {
            let rel = entry
                .path()
                .strip_prefix(vault_root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(entry.path()).unwrap();
            notes.push(notesmith_vault::parse_note(
                &VaultName::new(VAULT),
                &VaultPath::new(rel),
                &content,
            ));
        }
    }
    cache.reindex(VAULT, &notes).unwrap();
}

#[test]
fn query_time_embedding_and_attach_join_filter() {
    let data = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    write_note(
        vault.path(),
        "projects/alpha.md",
        "---\ntags: [work, urgent]\n---\n\n# Alpha\n\nvector search and embeddings project",
    );
    write_note(
        vault.path(),
        "personal/hobby.md",
        "---\ntags: [personal]\n---\n\n# Hobby\n\nmountain hiking and photography",
    );

    // Real embeddings.db via the worker.
    let db_path = data.path().join("embeddings.db");
    let store = EmbeddingStore::open(&db_path).unwrap();
    let embedder = HashEmbedder::new(128);
    notesmith_embed::EmbedWorker::new(VAULT, vault.path(), &store, &embedder)
        .run()
        .unwrap();
    drop(store); // release the writer; the daemon opens read-only

    // Real note index for the metadata JOIN.
    let cache_path = data.path().join("cache.sqlite");
    build_index(&cache_path, vault.path());

    let embedder: Arc<dyn notesmith_embed::Embedder> = Arc::new(HashEmbedder::new(128));
    let search = EmbeddingSearch::open(VAULT, &db_path, &cache_path, embedder).unwrap();

    // Unfiltered: both notes are candidates.
    let all = search
        .search("project", 10, &MetaFilter::default())
        .unwrap();
    let all_paths: Vec<_> = all.iter().map(|s| s.chunk.path.clone()).collect();
    assert!(all_paths.iter().any(|p| p == "projects/alpha.md"));
    assert!(all_paths.iter().any(|p| p == "personal/hobby.md"));
    for hit in &all {
        assert!(hit.distance >= 0.0, "raw distance surfaced");
    }

    // Tag filter via ATTACH JOIN: only the 'work'-tagged note survives.
    let filtered = search
        .search(
            "project",
            10,
            &MetaFilter {
                tag: Some("work".to_string()),
                path_prefix: None,
            },
        )
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].chunk.path, "projects/alpha.md");

    // A tag no note has yields nothing.
    let none = search
        .search(
            "project",
            10,
            &MetaFilter {
                tag: Some("nonexistent".to_string()),
                path_prefix: None,
            },
        )
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn embedder_mismatch_fails_loudly() {
    let data = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    write_note(vault.path(), "a.md", "# A\n\nsome content");

    let db_path = data.path().join("embeddings.db");
    let store = EmbeddingStore::open(&db_path).unwrap();
    let embedder = HashEmbedder::new(128);
    notesmith_embed::EmbedWorker::new(VAULT, vault.path(), &store, &embedder)
        .run()
        .unwrap();
    drop(store);

    let cache_path = data.path().join("cache.sqlite");
    build_index(&cache_path, vault.path());

    // Different dim → different id → hard error at open.
    let wrong: Arc<dyn notesmith_embed::Embedder> = Arc::new(HashEmbedder::new(64));
    let msg = match EmbeddingSearch::open(VAULT, &db_path, &cache_path, wrong) {
        Ok(_) => panic!("expected embedder mismatch to fail"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("does not match"),
        "expected loud mismatch error, got: {msg}"
    );
}
