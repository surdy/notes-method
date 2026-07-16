use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 3;

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    // Check schema version — if mismatch, drop all tables and recreate
    conn.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT)")?;
    let current_version: Option<i64> = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();

    if current_version != Some(SCHEMA_VERSION) {
        // Fresh start: drop old tables/views and recreate
        conn.execute_batch(
            "
            DROP VIEW IF EXISTS v_notes;
            DROP VIEW IF EXISTS v_fields;
            DROP VIEW IF EXISTS v_tasks;
            DROP VIEW IF EXISTS v_task_fields;
            DROP VIEW IF EXISTS v_backlinks;
            DROP VIEW IF EXISTS v_dangling_links;
            DROP VIEW IF EXISTS v_periodic;
            DROP VIEW IF EXISTS v_customers;
            DROP VIEW IF EXISTS v_streams;
            DROP TABLE IF EXISTS notes;
            DROP TABLE IF EXISTS fields;
            DROP TABLE IF EXISTS tags;
            DROP TABLE IF EXISTS tasks;
            DROP TABLE IF EXISTS task_fields;
            DROP TABLE IF EXISTS links;
            DROP TABLE IF EXISTS periodic_notes;
            DROP TABLE IF EXISTS route_log;
            ",
        )?;
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS notes (
            vault_name TEXT NOT NULL,
            path TEXT NOT NULL,
            title TEXT,
            created_at TEXT,
            updated_at TEXT,
            word_count INTEGER,
            mtime_unix INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            body_excerpt TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (vault_name, path)
        );

        CREATE TABLE IF NOT EXISTS fields (
            vault_name TEXT NOT NULL,
            note_path TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            value_type TEXT NOT NULL DEFAULT 'string',
            source TEXT NOT NULL DEFAULT 'frontmatter'
        );

        CREATE INDEX IF NOT EXISTS idx_fields_note ON fields(vault_name, note_path);
        CREATE INDEX IF NOT EXISTS idx_fields_key ON fields(vault_name, key);
        CREATE INDEX IF NOT EXISTS idx_fields_key_value ON fields(vault_name, key, value);

        CREATE TABLE IF NOT EXISTS tags (
            vault_name TEXT NOT NULL,
            note_path TEXT NOT NULL,
            tag TEXT NOT NULL,
            PRIMARY KEY (vault_name, note_path, tag)
        );

        CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(vault_name, tag);

        CREATE TABLE IF NOT EXISTS tasks (
            vault_name TEXT NOT NULL,
            id INTEGER NOT NULL,
            note_path TEXT NOT NULL,
            line_number INTEGER NOT NULL,
            text TEXT NOT NULL,
            status_char TEXT NOT NULL DEFAULT ' ',
            status_group TEXT NOT NULL DEFAULT 'open',
            content_hash TEXT,
            raw_markdown TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (vault_name, id)
        );

        CREATE INDEX IF NOT EXISTS idx_tasks_note ON tasks(vault_name, note_path);
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(vault_name, status_group);

        CREATE TABLE IF NOT EXISTS task_fields (
            vault_name TEXT NOT NULL,
            task_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (vault_name, task_id, key)
        );

        CREATE TABLE IF NOT EXISTS links (
            vault_name TEXT NOT NULL,
            source_path TEXT NOT NULL,
            target_path TEXT,
            raw_target TEXT NOT NULL,
            link_text TEXT,
            kind TEXT NOT NULL,
            line_number INTEGER
        );

        CREATE INDEX IF NOT EXISTS idx_links_source ON links(vault_name, source_path);
        CREATE INDEX IF NOT EXISTS idx_links_target ON links(vault_name, target_path);

        CREATE TABLE IF NOT EXISTS periodic_notes (
            vault_name TEXT NOT NULL,
            note_path TEXT NOT NULL,
            period_kind TEXT NOT NULL,
            period_key TEXT NOT NULL,
            period_start TEXT NOT NULL,
            period_end TEXT NOT NULL,
            PRIMARY KEY (vault_name, note_path)
        );

        CREATE INDEX IF NOT EXISTS idx_periodic_kind ON periodic_notes(vault_name, period_kind);

        CREATE TABLE IF NOT EXISTS route_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            vault_name TEXT NOT NULL,
            note_path TEXT NOT NULL,
            rule_id TEXT,
            from_path TEXT NOT NULL,
            to_path TEXT NOT NULL,
            mutations_json TEXT,
            routed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        DROP VIEW IF EXISTS v_notes;
        CREATE VIEW v_notes AS
        SELECT vault_name, path, title, created_at, updated_at, word_count
        FROM notes;

        DROP VIEW IF EXISTS v_fields;
        CREATE VIEW v_fields AS
        SELECT vault_name, note_path, key, value, value_type
        FROM fields;

        DROP VIEW IF EXISTS v_tasks;
        CREATE VIEW v_tasks AS
        SELECT t.vault_name, t.id, t.note_path, t.line_number, t.text,
               t.status_char, t.status_group, n.title as note_title
        FROM tasks t
        JOIN notes n ON t.vault_name = n.vault_name AND t.note_path = n.path;

        DROP VIEW IF EXISTS v_task_fields;
        CREATE VIEW v_task_fields AS
        SELECT tf.vault_name, tf.task_id, tf.key, tf.value, t.note_path
        FROM task_fields tf
        JOIN tasks t ON tf.vault_name = t.vault_name AND tf.task_id = t.id;

        DROP VIEW IF EXISTS v_backlinks;
        CREATE VIEW v_backlinks AS
        SELECT l.vault_name, l.source_path, l.target_path, l.link_text,
               n.title as source_title
        FROM links l
        JOIN notes n ON l.vault_name = n.vault_name AND l.source_path = n.path
        WHERE l.target_path IS NOT NULL;

        DROP VIEW IF EXISTS v_dangling_links;
        CREATE VIEW v_dangling_links AS
        SELECT l.vault_name, l.source_path, n.title AS source_title,
               l.raw_target, l.link_text, l.kind, l.line_number
        FROM links l
        JOIN notes n ON l.vault_name = n.vault_name AND l.source_path = n.path
        WHERE l.target_path IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM notes t
              WHERE t.vault_name = l.vault_name
                AND t.path <> l.source_path
                AND ( t.title = l.target_path
                   OR t.path = l.target_path
                   OR t.path = l.target_path || '.md'
                   OR t.path LIKE '%/' || l.target_path || '.md' )
          );

        DROP VIEW IF EXISTS v_periodic;
        CREATE VIEW v_periodic AS
        SELECT pn.vault_name, pn.note_path, pn.period_kind, pn.period_key,
               pn.period_start, pn.period_end, n.title
        FROM periodic_notes pn
        JOIN notes n ON pn.vault_name = n.vault_name AND pn.note_path = n.path;
    ",
    )?;

    // Stamp the schema version
    conn.execute(
        "INSERT OR REPLACE INTO _meta (key, value) VALUES ('schema_version', ?1)",
        [SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}
