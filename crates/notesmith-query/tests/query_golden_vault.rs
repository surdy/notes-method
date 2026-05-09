use notesmith_core::VaultEngine;
use notesmith_index::VaultCache;
use notesmith_query::execute_sql;
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
fn select_from_v_notes() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT title, type FROM v_notes ORDER BY title LIMIT 5",
    )
    .unwrap();
    assert_eq!(result.columns, vec!["title", "type"]);
    assert!(result.row_count <= 5);
    assert!(result.row_count > 0);
}

#[test]
fn select_customers() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT title, state FROM v_customers ORDER BY title",
    )
    .unwrap();
    assert!(result.row_count >= 2, "Should have Acme and Globex");
}

#[test]
fn select_tasks_by_status() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT text, status FROM v_tasks WHERE status = 'todo'",
    )
    .unwrap();
    assert!(result.row_count >= 1);
}

#[test]
fn select_backlinks() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT note_path, backlink_path FROM v_backlinks LIMIT 10",
    )
    .unwrap();
    assert!(result.row_count >= 1);
}

#[test]
fn reject_non_select() {
    let cache = build_cache();
    let err = execute_sql(&cache, "DELETE FROM notes").unwrap_err();
    assert!(matches!(err, notesmith_query::QueryError::NotReadOnly));
}

#[test]
fn reject_insert() {
    let cache = build_cache();
    let err = execute_sql(&cache, "INSERT INTO notes (vault_name, path, title, type, frontmatter_json, archived, mtime_unix, content_hash, body_excerpt) VALUES ('x','x','x','x','{}',0,0,'x','x')").unwrap_err();
    assert!(matches!(err, notesmith_query::QueryError::NotReadOnly));
}

#[test]
fn with_cte_allowed() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "WITH t AS (SELECT * FROM v_notes) SELECT COUNT(*) as cnt FROM t",
    )
    .unwrap();
    assert_eq!(result.columns, vec!["cnt"]);
    assert_eq!(result.row_count, 1);
}

#[test]
fn query_result_is_json_serializable() {
    let cache = build_cache();
    let result = execute_sql(&cache, "SELECT title, type FROM v_notes LIMIT 3").unwrap();
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("columns"));
    assert!(json.contains("rows"));
}
