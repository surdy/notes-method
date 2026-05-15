use notesmith_core::VaultEngine;
use notesmith_index::VaultCache;
use notesmith_vault::NativeVaultEngine;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn build_cache() -> VaultCache {
    let engine = NativeVaultEngine;
    let notes = engine.scan(&golden_vault()).unwrap();
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("test", &notes).unwrap();
    cache
}

#[test]
fn index_populates_notes_table() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    assert!(count >= 20, "Should have at least 20 notes, got {count}");
}

#[test]
fn v_notes_view_works() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM v_notes", [], |row| row.get(0))
        .unwrap();
    assert!(count >= 20);
}

#[test]
fn v_notes_keeps_icon_frontmatter() {
    let cache = build_cache();
    let frontmatter_json: String = cache
        .connection()
        .query_row(
            "SELECT frontmatter_json FROM v_notes WHERE path = ?1",
            ["General/Prototype Notes.md"],
            |row| row.get(0),
        )
        .unwrap();
    let frontmatter: serde_json::Value = serde_json::from_str(&frontmatter_json).unwrap();

    assert_eq!(frontmatter["_icon"], "🔬");
}

#[test]
fn v_notes_has_typed_notes() {
    let cache = build_cache();
    let types: Vec<String> = {
        let conn = cache.connection();
        let mut stmt = conn
            .prepare("SELECT DISTINCT type FROM v_notes ORDER BY type")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert!(types.contains(&"daily".to_string()));
    assert!(types.contains(&"customer".to_string()));
    assert!(types.contains(&"meeting".to_string()));
    assert!(types.contains(&"stream".to_string()));
}

#[test]
fn v_customers_view() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM v_customers", [], |row| row.get(0))
        .unwrap();
    assert!(
        count >= 2,
        "Should have at least 2 customers (Acme, Globex)"
    );
}

#[test]
fn v_tasks_populated() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM v_tasks", [], |row| row.get(0))
        .unwrap();
    assert!(count >= 7, "Should have at least 7 tasks, got {count}");
}

#[test]
fn v_tasks_has_all_statuses() {
    let cache = build_cache();
    let statuses: Vec<String> = {
        let conn = cache.connection();
        let mut stmt = conn
            .prepare("SELECT DISTINCT status FROM v_tasks ORDER BY status")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert!(statuses.contains(&"todo".to_string()));
    assert!(statuses.contains(&"in_progress".to_string()));
    assert!(statuses.contains(&"blocked".to_string()));
    assert!(statuses.contains(&"done".to_string()));
    assert!(statuses.contains(&"cancelled".to_string()));
}

#[test]
fn links_table_populated() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM links", [], |row| row.get(0))
        .unwrap();
    assert!(count >= 10, "Should have many links, got {count}");
}

#[test]
fn v_backlinks_view() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM v_backlinks", [], |row| row.get(0))
        .unwrap();
    assert!(count >= 1, "Should have backlinks");
}

#[test]
fn inline_fields_populated() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM inline_fields", [], |row| row.get(0))
        .unwrap();
    assert!(count >= 5, "Should have inline fields, got {count}");
}

#[test]
fn incremental_update_note() {
    let engine = NativeVaultEngine;
    let notes = engine.scan(&golden_vault()).unwrap();
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("test", &notes).unwrap();

    let initial_count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();

    if let Some(note) = notes.first() {
        cache.update_note("test", note).unwrap();
    }

    let after_count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        initial_count, after_count,
        "Count should be same after re-indexing existing note"
    );
}

#[test]
fn remove_note_from_cache() {
    let engine = NativeVaultEngine;
    let notes = engine.scan(&golden_vault()).unwrap();
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("test", &notes).unwrap();

    let initial_count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();

    if let Some(note) = notes.first() {
        cache.remove_note("test", note.path.as_str()).unwrap();
    }

    let after_count: i64 = cache
        .connection()
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();

    assert_eq!(initial_count - 1, after_count);
}
