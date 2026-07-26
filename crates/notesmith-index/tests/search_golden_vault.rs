use notesmith_core::{Note, VaultEngine};
use notesmith_index::SearchIndex;
use notesmith_vault::NativeVaultEngine;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn load_notes() -> Vec<Note> {
    let engine = NativeVaultEngine;
    engine.scan(&golden_vault()).unwrap()
}

fn build_search_index() -> SearchIndex {
    let notes = load_notes();
    let index = SearchIndex::open_in_memory().unwrap();
    index.reindex("test", &notes).unwrap();
    index
}

#[test]
fn search_by_title() {
    let index = build_search_index();

    let results = index.search("Acme", 10).unwrap();

    assert!(
        results
            .iter()
            .any(|result| result.path == "Customers/Acme Corp/Acme Corp.md")
    );
    assert!(results.iter().any(|result| result.title == "Acme Corp"));
}

#[test]
fn search_by_body_content() {
    let index = build_search_index();

    let results = index.search("zero-downtime", 10).unwrap();

    assert!(results.iter().any(|result| {
        result.path == "Meetings/2025/01/2025-01-14 - Acme Corp - Customer Check-in.md"
    }));
}

#[test]
fn search_returns_snippets() {
    let index = build_search_index();

    let results = index.search("zero-downtime", 10).unwrap();
    let result = results
        .into_iter()
        .find(|result| {
            result.path == "Meetings/2025/01/2025-01-14 - Acme Corp - Customer Check-in.md"
        })
        .unwrap();

    assert!(!result.snippet.trim().is_empty());
}

#[test]
fn search_limit_respected() {
    let index = build_search_index();

    let results = index.search("Acme", 2).unwrap();

    assert!(results.len() <= 2);
}

#[test]
fn search_no_results() {
    let index = build_search_index();

    let results = index.search("qzxwplmno123", 10).unwrap();

    assert!(results.is_empty());
}

#[test]
fn incremental_update() {
    let notes = load_notes();
    let index = SearchIndex::open_in_memory().unwrap();
    index.reindex("test", &notes).unwrap();

    let mut updated = notes
        .iter()
        .find(|note| note.path.as_str() == "Customers/Acme Corp/Acme Corp.md")
        .unwrap()
        .clone();
    updated
        .body
        .push_str("\n\nThe searchfreshneedle token should be indexed immediately.\n");

    index.update_note("test", &updated).unwrap();

    let results = index.search("searchfreshneedle", 10).unwrap();

    assert!(
        results
            .iter()
            .any(|result| result.path == updated.path.as_str())
    );
}

#[test]
fn remove_note_from_index() {
    let notes = load_notes();
    let index = SearchIndex::open_in_memory().unwrap();
    index.reindex("test", &notes).unwrap();

    index.remove_note("test", "Inbox/Quick Note.md").unwrap();

    let results = index.search("pricing changes", 10).unwrap();

    assert!(
        !results
            .iter()
            .any(|result| result.path == "Inbox/Quick Note.md")
    );
}
