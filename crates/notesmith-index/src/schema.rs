use rusqlite::Connection;

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
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

        CREATE VIEW IF NOT EXISTS v_notes AS
        SELECT vault_name, path, title, created_at, updated_at, word_count
        FROM notes;

        CREATE VIEW IF NOT EXISTS v_fields AS
        SELECT vault_name, note_path, key, value, value_type
        FROM fields;

        CREATE VIEW IF NOT EXISTS v_tasks AS
        SELECT t.vault_name, t.id, t.note_path, t.line_number, t.text,
               t.status_char, t.status_group, n.title as note_title
        FROM tasks t
        JOIN notes n ON t.vault_name = n.vault_name AND t.note_path = n.path;

        CREATE VIEW IF NOT EXISTS v_task_fields AS
        SELECT tf.vault_name, tf.task_id, tf.key, tf.value, t.note_path
        FROM task_fields tf
        JOIN tasks t ON tf.vault_name = t.vault_name AND tf.task_id = t.id;

        CREATE VIEW IF NOT EXISTS v_backlinks AS
        SELECT l.vault_name, l.source_path, l.target_path, l.link_text,
               n.title as source_title
        FROM links l
        JOIN notes n ON l.vault_name = n.vault_name AND l.source_path = n.path
        WHERE l.target_path IS NOT NULL;

        CREATE VIEW IF NOT EXISTS v_periodic AS
        SELECT pn.vault_name, pn.note_path, pn.period_kind, pn.period_key,
               pn.period_start, pn.period_end, n.title
        FROM periodic_notes pn
        JOIN notes n ON pn.vault_name = n.vault_name AND pn.note_path = n.path;
    ",
    )
}
