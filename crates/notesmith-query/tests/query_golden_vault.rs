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
        "SELECT title, updated_at FROM v_notes ORDER BY title LIMIT 5",
    )
    .unwrap();
    assert_eq!(result.columns, vec!["title", "updated_at"]);
    assert!(result.row_count <= 5);
    assert!(result.row_count > 0);
}

#[test]
fn select_customers() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT n.title FROM v_notes n JOIN v_fields kind ON kind.vault_name = n.vault_name AND kind.note_path = n.path AND kind.key = 'kind' WHERE kind.value = 'customer' ORDER BY n.title",
    )
    .unwrap();
    assert!(result.row_count >= 2, "Should have Acme Corp and Globex");
}

#[test]
fn list_field_membership_is_exact_and_per_element() {
    let cache = build_cache();

    // The cross-customer meeting contributes one row per customer, so both
    // customers match exactly — no substring false positives.
    let acme = execute_sql(
        &cache,
        "SELECT note_path FROM v_field_values WHERE key = 'customers' AND value = '[[Acme Corp]]' ORDER BY note_path",
    )
    .unwrap();
    let globex = execute_sql(
        &cache,
        "SELECT note_path FROM v_field_values WHERE key = 'customers' AND value = '[[Globex]]' ORDER BY note_path",
    )
    .unwrap();

    assert!(acme.row_count >= 2);
    assert!(globex.row_count >= 2);

    let multi = execute_sql(
        &cache,
        "SELECT note_path, COUNT(*) AS customer_count FROM v_field_values WHERE key = 'customers' GROUP BY note_path HAVING customer_count > 1",
    )
    .unwrap();
    assert_eq!(
        multi.row_count, 1,
        "the cross-customer meeting is the fixture's multi-customer note"
    );
}

#[test]
fn tasks_inherit_their_notes_frontmatter() {
    let cache = build_cache();

    // Tasks inside the Acme meetings/streams inherit `customers` from the note.
    let inherited = execute_sql(
        &cache,
        "SELECT t.text FROM v_tasks t JOIN v_task_effective_fields c ON c.vault_name = t.vault_name AND c.task_id = t.id AND c.key = 'customers' AND c.value = '[[Acme Corp]]' WHERE t.status_group = 'open' AND c.source = 'note'",
    )
    .unwrap();
    assert!(
        inherited.row_count >= 1,
        "open Acme tasks should be reachable without task-level metadata"
    );

    // Delegation is task-level and overrides nothing else.
    let delegated = execute_sql(
        &cache,
        "SELECT t.text, o.value FROM v_tasks t JOIN v_task_effective_fields o ON o.vault_name = t.vault_name AND o.task_id = t.id AND o.key = 'owner' AND o.source = 'task'",
    )
    .unwrap();
    assert!(
        delegated.row_count >= 3,
        "fixture meetings delegate tasks via [owner:: ...]"
    );
}

#[test]
fn frontmatter_wikilinks_become_link_edges() {
    let cache = build_cache();

    let backlinks = execute_sql(
        &cache,
        "SELECT source_path FROM v_backlinks WHERE target_path = 'Acme Corp' AND source = 'frontmatter'",
    )
    .unwrap();
    assert!(
        backlinks.row_count >= 1,
        "customer notes get backlinks from frontmatter `customers` lists"
    );

    // Lazily-created people: linked as attendees, no People note yet.
    let dangling = execute_sql(
        &cache,
        "SELECT DISTINCT raw_target FROM v_dangling_links WHERE source = 'frontmatter'",
    )
    .unwrap();
    assert!(
        dangling.row_count >= 1,
        "unresolved attendees are the person-promotion signal"
    );
}

#[test]
fn select_tasks_by_status() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT text, status_group FROM v_tasks WHERE status_group = 'open'",
    )
    .unwrap();
    assert!(result.row_count >= 1);
}

#[test]
fn select_backlinks() {
    let cache = build_cache();
    let result = execute_sql(
        &cache,
        "SELECT target_path, source_path FROM v_backlinks LIMIT 10",
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
    let err = execute_sql(&cache, "INSERT INTO notes (vault_name, path, mtime_unix, content_hash, body_excerpt) VALUES ('x','x',0,'x','x')").unwrap_err();
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
    let result = execute_sql(&cache, "SELECT title, created_at FROM v_notes LIMIT 3").unwrap();
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

/// The vault's `.notesmith/` config also carries SQL — sidebar queries, badge
/// counts, and prompt context queries. Nothing else executes them, so they rot
/// silently against schema changes unless a test runs them.
#[test]
fn sql_in_vault_config_executes() {
    let temp = tempfile::tempdir().unwrap();
    let notes = NativeVaultEngine.scan(&golden_vault()).unwrap();
    let cache =
        VaultCache::open_for_vault(&temp.path().join("cache.sqlite"), &golden_vault()).unwrap();
    cache.reindex("test", &notes).unwrap();

    let mut statements = Vec::new();
    for relative in ["sidebar.yaml", "sidebar-views.yaml"] {
        let path = golden_vault().join(".notesmith").join(relative);
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_yaml::Value = serde_yaml::from_str(&content)
            .unwrap_or_else(|error| panic!("{relative} is not valid YAML: {error}"));
        collect_sql_values(&value, relative, &mut statements);
    }

    let prompts_dir = golden_vault().join(".notesmith").join("prompts");
    if prompts_dir.is_dir() {
        for entry in std::fs::read_dir(&prompts_dir).unwrap() {
            let path = entry.unwrap().path();
            let content = std::fs::read_to_string(&path).unwrap();
            let Some(frontmatter) = content
                .strip_prefix("---\n")
                .and_then(|rest| rest.split_once("\n---").map(|(head, _)| head))
            else {
                continue;
            };
            let label = path.file_name().unwrap().to_string_lossy().to_string();
            let value: serde_yaml::Value = serde_yaml::from_str(frontmatter)
                .unwrap_or_else(|error| panic!("{label} frontmatter is not valid YAML: {error}"));
            collect_sql_values(&value, &label, &mut statements);
        }
    }

    assert!(
        statements.len() >= 8,
        "expected the fixture config to carry SQL, found {}",
        statements.len()
    );

    let failures: Vec<String> = statements
        .iter()
        .filter_map(|(source, sql)| {
            execute_sql(&cache, sql)
                .err()
                .map(|error| format!("{source}\nSQL:\n{sql}\nError: {error}"))
        })
        .collect();

    assert!(
        failures.is_empty(),
        "All SQL in golden-vault/.notesmith should execute.\n\n{}",
        failures.join("\n\n---\n\n")
    );
}

/// Recursively collect values held under SQL-bearing keys, so the walk survives
/// config-shape changes.
fn collect_sql_values(value: &serde_yaml::Value, source: &str, out: &mut Vec<(String, String)>) {
    const SQL_KEYS: [&str; 4] = ["query", "badge_query", "data_source", "sql"];

    match value {
        serde_yaml::Value::Mapping(map) => {
            for (key, child) in map {
                if let (Some(key), Some(sql)) = (key.as_str(), child.as_str()) {
                    if SQL_KEYS.contains(&key) {
                        out.push((format!("{source} ({key})"), sql.to_string()));
                        continue;
                    }
                }
                collect_sql_values(child, source, out);
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                collect_sql_values(item, source, out);
            }
        }
        _ => {}
    }
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_markdown_files(root, &mut files);
    files.sort();
    files
}

fn collect_markdown_files(path: &Path, files: &mut Vec<PathBuf>) {
    // Mirror the vault scanner: hidden directories (`.notesmith/`) hold config
    // and Jinja templates, not notes. Their SQL is validated separately by
    // `sql_in_vault_config_executes`.
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
    {
        return;
    }

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
