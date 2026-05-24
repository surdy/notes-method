use notesmith_core::{VaultEngine, VaultName, VaultPath};
use notesmith_index::VaultCache;
use notesmith_vault::{NativeVaultEngine, parse_note};

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

fn build_cache_for_content(path: &str, content: &str) -> VaultCache {
    let note = parse_note(&VaultName::new("test"), &VaultPath::new(path), content);
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("test", &[note]).unwrap();
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
fn fields_include_frontmatter_values_with_types() {
    let cache = build_cache();
    let row = cache
        .connection()
        .query_row(
            "SELECT value, value_type, source FROM fields WHERE note_path = ?1 AND key = ?2",
            ["General/Prototype Notes.md", "_icon"],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(row.0, "🔬");
    assert_eq!(row.1, "string");
    assert_eq!(row.2, "frontmatter");
}

#[test]
fn fields_include_inline_fields() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM fields WHERE note_path = ?1 AND key = 'owner' AND source = 'inline'",
            ["Inbox/Daily/2025-01-15.md"],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        count >= 1,
        "Should index owner inline fields from note body"
    );
}

#[test]
fn tags_include_frontmatter_and_inline_hashtags() {
    let cache = build_cache();
    let daily: i64 = cache
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM tags WHERE note_path = ?1 AND tag = 'daily'",
            ["Inbox/Daily/2025-01-15.md"],
            |row| row.get(0),
        )
        .unwrap();
    let inline: i64 = cache
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM tags WHERE note_path = ?1 AND tag = 'acme'",
            ["Customers/Acme/Acme Corp.md"],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(daily, 1);
    assert_eq!(inline, 1);
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
fn v_tasks_exposes_status_chars_and_groups() {
    let cache = build_cache();
    let rows: Vec<(String, String)> = {
        let conn = cache.connection();
        let mut stmt = conn
            .prepare("SELECT DISTINCT status_char, status_group FROM v_tasks ORDER BY status_char")
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };

    assert!(rows.contains(&(" ".to_string(), "open".to_string())));
    assert!(rows.contains(&("x".to_string(), "done".to_string())));
    assert!(rows.contains(&("-".to_string(), "done".to_string())));
}

#[test]
fn task_fields_populated_from_task_inline_fields() {
    let cache = build_cache_for_content(
        "Inbox/Tasks.md",
        "---\ntitle: Tasks\n---\n- [ ] Follow up [due:: 2026-05-10] [owner:: me]\n",
    );

    let fields: Vec<(String, String)> = {
        let conn = cache.connection();
        let mut stmt = conn
            .prepare("SELECT key, value FROM v_task_fields ORDER BY key")
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };

    assert_eq!(
        fields,
        vec![
            ("due".to_string(), "2026-05-10".to_string()),
            ("owner".to_string(), "me".to_string())
        ]
    );
}

#[test]
fn periodic_notes_are_detected() {
    let cache = build_cache();
    let count: i64 = cache
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM v_periodic WHERE period_kind = 'daily' AND note_path = ?1",
            ["Inbox/Daily/2025-01-15.md"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn periodic_notes_follow_configured_folder_templates() {
    let note = parse_note(
        &VaultName::new("test"),
        &VaultPath::new("Weekly/Week 2026-W21.md"),
        "# Weekly review\n",
    );
    let cache = VaultCache::open_in_memory().unwrap();
    let config = notesmith_config::PeriodicConfig {
        weekly: Some(notesmith_config::PeriodKindConfig {
            folder: "Weekly".to_string(),
            template: Some("weekly".to_string()),
            filename: "Week {{ week }}".to_string(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        }),
        ..Default::default()
    };

    cache
        .reindex_with_periodic("test", &[note], &config)
        .unwrap();

    let row: (String, String, String, String) = cache
        .connection()
        .query_row(
            "SELECT period_kind, period_key, period_start, period_end FROM v_periodic WHERE note_path = ?1",
            ["Weekly/Week 2026-W21.md"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "weekly".to_string(),
            "2026-W21".to_string(),
            "2026-05-18".to_string(),
            "2026-05-24".to_string(),
        )
    );
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
fn right_rail_queries_match_schema() {
    let cache = build_cache();
    let conn = cache.connection();

    let backlinks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT DISTINCT source_path, COALESCE(source_title, source_path) AS source_title FROM v_backlinks WHERE target_path = 'Acme Corp' ORDER BY source_title)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let outgoing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT DISTINCT target_path, COALESCE(n.title, target_path) AS target FROM v_backlinks b LEFT JOIN v_notes n ON n.path = b.target_path WHERE b.source_path = 'Customers/Acme/Acme Corp.md' ORDER BY target)",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(backlinks >= 1);
    assert!(outgoing >= 1);
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

    assert_eq!(initial_count, after_count);
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
