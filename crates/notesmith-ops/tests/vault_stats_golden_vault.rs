//! Golden-vault integration test for the `vault_stats` structure summary (#202).
//!
//! Builds a real on-disk note index (`VaultCache`) and Tantivy [`SearchIndex`]
//! over the shared `golden-vault/` fixture, constructs a [`LocalOps`], and
//! asserts `vault_stats` returns coherent totals and ranked lists — all from the
//! note index, with no embeddings involved.

use notesmith_config::VaultConfig;
use notesmith_core::VaultEngine;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_ops::{LocalOps, Ops};
use notesmith_vault::NativeVaultEngine;

const VAULT: &str = "golden";

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn build_ops() -> LocalOps {
    let root = golden_vault();
    let notes = NativeVaultEngine.scan(&root).unwrap();

    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex(VAULT, &notes).unwrap();

    let search_index = SearchIndex::open_in_memory().unwrap();
    search_index.reindex(VAULT, &notes).unwrap();

    let config = VaultConfig {
        name: VAULT.to_string(),
        ..Default::default()
    };

    LocalOps::new(VAULT.to_string(), root, cache, search_index, config)
}

#[test]
fn vault_stats_over_golden_vault_is_coherent() {
    let ops = build_ops();

    let stats = ops.vault_stats(Some(5)).unwrap();

    assert_eq!(stats["vault"], VAULT);

    let totals = &stats["totals"];
    let notes = totals["notes"].as_i64().unwrap();
    assert!(notes > 0, "golden vault should have notes");

    // Every total is a non-negative integer, and orphans cannot exceed notes.
    for key in ["notes", "tags", "links", "tasks", "words", "orphans"] {
        let v = totals[key].as_i64().unwrap_or(-1);
        assert!(v >= 0, "total `{key}` should be >= 0, got {v}");
    }
    assert!(
        totals["orphans"].as_i64().unwrap() <= notes,
        "orphans cannot exceed total notes"
    );

    // Ranked lists are capped by `top` and internally well-formed.
    let tags = stats["tags"].as_array().unwrap();
    assert!(tags.len() <= 5);
    for tag in tags {
        assert!(tag["tag"].is_string());
        assert!(tag["note_count"].as_i64().unwrap() >= 1);
    }

    let backlinks = stats["backlinks"].as_array().unwrap();
    assert!(backlinks.len() <= 5);
    for bl in backlinks {
        assert!(bl["path"].is_string());
        assert!(bl["backlink_count"].as_i64().unwrap() >= 1);
    }

    let orphans = stats["orphans"].as_array().unwrap();
    assert!(orphans.len() <= 5);
    for orphan in orphans {
        assert!(orphan["path"].is_string());
    }
}
