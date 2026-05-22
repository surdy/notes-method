use notesmith_core::VaultEngine;
use notesmith_index::VaultCache;
use notesmith_query::{
    QueryFormat, QueryRequest, QueryResult, execute_sql, execute_sql_with_options,
    format_query_as_markdown_table,
};
use notesmith_vault::NativeVaultEngine;
use serde_json::json;
use std::path::{Path, PathBuf};

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
    assert!(json.contains("truncated"));
}

#[test]
fn format_query_results_as_markdown_table() {
    let result = QueryResult {
        columns: vec!["text".to_string(), "notes".to_string()],
        rows: vec![vec![json!("Follow\nup"), json!("A | B")]],
        row_count: 1,
        truncated: false,
    };

    assert_eq!(
        format_query_as_markdown_table(&result),
        "| text | notes |\n| --- | --- |\n| Follow up | A \\| B |"
    );
}

#[test]
fn execute_sql_truncates_to_max_rows() {
    let cache = build_cache();
    let result =
        execute_sql_with_options(&cache, "SELECT title FROM v_notes ORDER BY title", Some(1))
            .unwrap();

    assert_eq!(result.columns, vec!["title"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.row_count, 1);
    assert!(result.truncated);
}

#[test]
fn query_request_supports_default_and_markdown_formats() {
    let default_request: QueryRequest = serde_json::from_value(json!({
        "sql": "SELECT title FROM v_notes"
    }))
    .unwrap();
    assert_eq!(default_request.format, QueryFormat::Json);
    assert_eq!(default_request.max_rows_or_default(), 10_000);

    let markdown_request: QueryRequest = serde_json::from_value(json!({
        "sql": "SELECT title FROM v_notes",
        "max_rows": 5,
        "format": "markdown"
    }))
    .unwrap();
    assert_eq!(markdown_request.format, QueryFormat::Markdown);
    assert_eq!(markdown_request.max_rows_or_default(), 5);
}

#[test]
fn notesmith_sql_fences_in_golden_vault_execute() {
    let cache = build_cache();
    let mut failures = Vec::new();

    for path in markdown_files(&golden_vault()) {
        let content = std::fs::read_to_string(&path).unwrap();
        for sql in notesmith_sql_blocks(&content) {
            if let Err(error) = execute_sql(&cache, &sql) {
                failures.push(format!(
                    "{}\nSQL:\n{}\nError: {error}",
                    path.strip_prefix(golden_vault()).unwrap().display(),
                    sql
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "All notesmith sql fences in golden-vault should execute.\n\n{}",
        failures.join("\n\n---\n\n")
    );
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_markdown_files(root, &mut files);
    files.sort();
    files
}

fn collect_markdown_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            collect_markdown_files(&entry.unwrap().path(), files);
        }
        return;
    }

    if path.extension().is_some_and(|extension| extension == "md") {
        files.push(path.to_path_buf());
    }
}

fn notesmith_sql_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut block_lines: Option<Vec<&str>> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(lines) = block_lines.as_mut() {
            if trimmed == "```" {
                let sql = lines.join("\n").trim().to_string();
                if !sql.is_empty() {
                    blocks.push(sql);
                }
                block_lines = None;
            } else {
                lines.push(line);
            }
            continue;
        }

        if let Some(info) = trimmed.strip_prefix("```") {
            let normalized = info
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if normalized == "notesmith" || normalized == "notesmith sql" {
                block_lines = Some(Vec::new());
            }
        }
    }

    blocks
}
