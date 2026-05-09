use rusqlite::Connection;

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS notes (
            vault_name TEXT NOT NULL,
            path TEXT NOT NULL,
            title TEXT NOT NULL,
            type TEXT NOT NULL,
            frontmatter_json TEXT NOT NULL,
            customer TEXT,
            stream TEXT,
            state TEXT,
            status TEXT,
            date TEXT,
            created_at TEXT,
            updated_at TEXT,
            archived INTEGER NOT NULL DEFAULT 0,
            mtime_unix INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            body_excerpt TEXT NOT NULL,
            PRIMARY KEY (vault_name, path)
        );

        CREATE TABLE IF NOT EXISTS links (
            vault_name TEXT NOT NULL,
            src_path TEXT NOT NULL,
            dst_path TEXT,
            raw_target TEXT NOT NULL,
            kind TEXT NOT NULL,
            heading_ref TEXT,
            block_ref TEXT
        );

        CREATE TABLE IF NOT EXISTS inline_fields (
            vault_name TEXT NOT NULL,
            note_path TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            value_json TEXT
        );

        CREATE TABLE IF NOT EXISTS tasks (
            vault_name TEXT NOT NULL,
            task_hash TEXT NOT NULL,
            note_path TEXT NOT NULL,
            heading_path TEXT,
            ordinal INTEGER NOT NULL,
            status TEXT NOT NULL,
            text TEXT NOT NULL,
            customer TEXT,
            stream TEXT,
            owner TEXT,
            due TEXT,
            scheduled TEXT,
            start_date TEXT,
            done_at TEXT,
            priority INTEGER,
            recurrence TEXT,
            raw_markdown TEXT NOT NULL,
            PRIMARY KEY (vault_name, task_hash)
        );

        CREATE VIEW IF NOT EXISTS v_notes AS
        SELECT vault_name, path, title, type, customer, stream, state,
               status, date, created_at, updated_at, archived,
               mtime_unix, frontmatter_json
        FROM notes;

        CREATE VIEW IF NOT EXISTS v_tasks AS
        SELECT vault_name, task_hash, note_path, heading_path, ordinal,
               status, text, customer, stream, owner, due, scheduled,
               start_date, done_at, priority
        FROM tasks;

        CREATE VIEW IF NOT EXISTS v_backlinks AS
        SELECT dst_path AS note_path, src_path AS backlink_path,
               kind, heading_ref, block_ref
        FROM links
        WHERE dst_path IS NOT NULL;

        CREATE VIEW IF NOT EXISTS v_customers AS
        SELECT * FROM v_notes WHERE type = 'customer';

        CREATE VIEW IF NOT EXISTS v_streams AS
        SELECT * FROM v_notes WHERE type = 'stream';
    ",
    )
}
