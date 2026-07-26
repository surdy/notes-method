use rusqlite::Connection;
use std::path::Path;

/// Load user-defined SQL views from .notesmith/views.sql.
/// Each statement is executed independently — a bad statement is logged and skipped.
pub fn load_user_views(conn: &Connection, vault_root: &Path) -> Vec<String> {
    let path = vault_root.join(".notesmith").join("views.sql");
    if !path.exists() {
        return Vec::new();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(path = %path.display(), "Failed to read views.sql: {error}");
            return Vec::new();
        }
    };

    let mut loaded_views = Vec::new();

    for stmt in content.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }

        let upper = stmt.to_uppercase();
        if !upper.starts_with("CREATE VIEW") && !upper.starts_with("CREATE OR REPLACE VIEW") {
            tracing::warn!(
                "Skipping non-CREATE VIEW statement in views.sql: {}",
                &stmt[..stmt.len().min(80)]
            );
            continue;
        }

        let view_name = extract_view_name(stmt);
        let sql = normalize_create_view_statement(stmt, view_name.as_deref());

        match conn.execute_batch(&sql) {
            Ok(()) => {
                if let Some(name) = &view_name {
                    loaded_views.push(name.clone());
                    tracing::info!("Loaded user view: {name}");
                }
            }
            Err(error) => {
                tracing::warn!(
                    "Failed to create user view: {error} — statement: {}",
                    &stmt[..stmt.len().min(100)]
                );
            }
        }
    }

    loaded_views
}

/// Drop all previously loaded user views (for hot-reload).
pub fn drop_user_views(conn: &Connection, view_names: &[String]) {
    for name in view_names {
        if let Err(error) = conn.execute_batch(&format!("DROP VIEW IF EXISTS {name};")) {
            tracing::warn!("Failed to drop user view '{name}': {error}");
        }
    }
}

/// Extract the view name from a CREATE VIEW statement.
fn extract_view_name(stmt: &str) -> Option<String> {
    let upper = stmt.to_uppercase();
    let after_view = if let Some(index) = upper.find("VIEW") {
        &stmt[index + 4..]
    } else {
        return None;
    };

    let trimmed = after_view.trim_start();
    let trimmed = if trimmed.to_uppercase().starts_with("IF NOT EXISTS") {
        trimmed[13..].trim_start()
    } else {
        trimmed
    };

    trimmed
        .split_whitespace()
        .next()
        .map(|name| name.to_string())
}

fn normalize_create_view_statement(stmt: &str, view_name: Option<&str>) -> String {
    let upper = stmt.to_uppercase();
    if upper.starts_with("CREATE OR REPLACE VIEW") {
        let after_view = &stmt[upper.find("VIEW").expect("view keyword present") + 4..];
        match view_name {
            Some(name) => format!("DROP VIEW IF EXISTS {name}; CREATE VIEW{after_view};"),
            None => format!("CREATE VIEW{after_view};"),
        }
    } else {
        format!("{stmt};")
    }
}

#[cfg(test)]
mod tests {
    use super::{drop_user_views, extract_view_name, load_user_views};
    use crate::schema::create_schema;
    use rusqlite::Connection;
    use std::fs;

    fn write_views_file(root: &std::path::Path, content: &str) {
        fs::create_dir_all(root.join(".notesmith")).unwrap();
        fs::write(root.join(".notesmith/views.sql"), content).unwrap();
    }

    fn golden_vault() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
    }

    #[test]
    fn loads_views_from_sql_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE source (name TEXT); INSERT INTO source (name) VALUES ('Acme'), ('Globex');").unwrap();
        write_views_file(
            temp_dir.path(),
            "CREATE VIEW customer_names AS SELECT name FROM source ORDER BY name;",
        );

        let loaded = load_user_views(&conn, temp_dir.path());

        assert_eq!(loaded, vec!["customer_names".to_string()]);
        let names = conn
            .prepare("SELECT name FROM customer_names")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(names, vec!["Acme".to_string(), "Globex".to_string()]);
    }

    #[test]
    fn skips_bad_statements_and_continues() {
        let temp_dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE source (name TEXT); INSERT INTO source (name) VALUES ('Acme');",
        )
        .unwrap();
        write_views_file(
            temp_dir.path(),
            r#"
DROP TABLE source;
CREATE VIEW good_view AS SELECT name FROM source;
CREATE VIEW broken_view AS SELECT FROM source;
"#,
        );

        let loaded = load_user_views(&conn, temp_dir.path());

        assert_eq!(loaded, vec!["good_view".to_string()]);
        let value: String = conn
            .query_row("SELECT name FROM good_view", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "Acme");
        assert!(conn.prepare("SELECT name FROM broken_view").is_err());
    }

    #[test]
    fn extracts_view_names() {
        assert_eq!(
            extract_view_name("CREATE VIEW dashboard AS SELECT 1"),
            Some("dashboard".to_string())
        );
        assert_eq!(
            extract_view_name("CREATE OR REPLACE VIEW dashboard AS SELECT 1"),
            Some("dashboard".to_string())
        );
        assert_eq!(
            extract_view_name("CREATE VIEW IF NOT EXISTS dashboard AS SELECT 1"),
            Some("dashboard".to_string())
        );
    }

    #[test]
    fn drops_and_recreates_views_for_hot_reload() {
        let temp_dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE source (name TEXT); INSERT INTO source (name) VALUES ('Acme'), ('Globex');").unwrap();
        write_views_file(
            temp_dir.path(),
            "CREATE VIEW dashboard AS SELECT name FROM source WHERE name = 'Acme';",
        );
        let loaded = load_user_views(&conn, temp_dir.path());
        let first: String = conn
            .query_row("SELECT name FROM dashboard", [], |row| row.get(0))
            .unwrap();
        assert_eq!(first, "Acme");

        drop_user_views(&conn, &loaded);
        assert!(conn.prepare("SELECT name FROM dashboard").is_err());

        write_views_file(
            temp_dir.path(),
            "CREATE VIEW dashboard AS SELECT name FROM source WHERE name = 'Globex';",
        );
        load_user_views(&conn, temp_dir.path());
        let reloaded: String = conn
            .query_row("SELECT name FROM dashboard", [], |row| row.get(0))
            .unwrap();
        assert_eq!(reloaded, "Globex");
    }

    #[test]
    fn loads_golden_vault_view_fixture() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO notes (vault_name, path, title, created_at, updated_at, word_count, mtime_unix, content_hash, body_excerpt)
             VALUES ('test', 'Customers/Acme Corp/Acme Corp.md', 'Acme Corp', NULL, NULL, 0, 0, 'hash', ''),
                    ('test', 'Streams/Migration to v2.md', 'Migration to v2', NULL, NULL, 0, 0, 'hash', '');
             INSERT INTO field_values (vault_name, note_path, key, ordinal, value, value_type, source)
             VALUES ('test', 'Customers/Acme Corp/Acme Corp.md', 'kind', 0, 'customer', 'string', 'frontmatter'),
                    ('test', 'Streams/Migration to v2.md', 'kind', 0, 'stream', 'string', 'frontmatter'),
                    ('test', 'Streams/Migration to v2.md', 'status', 0, 'active', 'string', 'frontmatter'),
                    ('test', 'Streams/Migration to v2.md', 'priority', 0, 'P1', 'string', 'frontmatter'),
                    ('test', 'Streams/Migration to v2.md', 'customers', 0, '[[Acme Corp]]', 'string', 'frontmatter');",
        )
        .unwrap();

        let loaded = load_user_views(&conn, &golden_vault());

        assert!(loaded.contains(&"customer_notes".to_string()));
        let customer: String = conn
            .query_row("SELECT title FROM customer_notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(customer, "Acme Corp");

        assert!(loaded.contains(&"stream_rollup".to_string()));
        let stream: (String, String, String, String) = conn
            .query_row(
                "SELECT title, status, priority, customer FROM stream_rollup",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            stream,
            (
                "Migration to v2".to_string(),
                "active".to_string(),
                "P1".to_string(),
                "[[Acme Corp]]".to_string()
            )
        );
    }
}
