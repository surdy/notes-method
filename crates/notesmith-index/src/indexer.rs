use notesmith_config::PeriodicConfig;
use notesmith_core::{Frontmatter, LinkType, Note, PeriodKind, Task};
use regex::Regex;
use rusqlite::{Connection, params};
use serde_yaml::Value;
use std::{collections::HashSet, ops::Range, sync::OnceLock};
use tracing::warn;

pub struct CacheIndexer<'a> {
    conn: &'a Connection,
    periodic_config: Option<&'a PeriodicConfig>,
}

impl<'a> CacheIndexer<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            periodic_config: None,
        }
    }

    pub fn with_periodic_config(conn: &'a Connection, periodic_config: &'a PeriodicConfig) -> Self {
        Self {
            conn,
            periodic_config: Some(periodic_config),
        }
    }

    pub fn index_all(&self, vault_name: &str, notes: &[Note]) -> anyhow::Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;

        let result = (|| -> anyhow::Result<()> {
            self.clear_vault(vault_name)?;
            for (index, note) in notes.iter().enumerate() {
                let savepoint = format!("note_{index}");
                self.conn
                    .execute_batch(&format!("SAVEPOINT {savepoint};"))?;
                match self.index_note_inner(vault_name, note) {
                    Ok(()) => {
                        self.conn.execute_batch(&format!("RELEASE {savepoint};"))?;
                    }
                    Err(error) => {
                        warn!(
                            note = %note.path.as_str(),
                            stage = "index",
                            reason = %error,
                            "skipping note during cache index"
                        );
                        self.conn.execute_batch(&format!(
                            "ROLLBACK TO {savepoint}; RELEASE {savepoint};"
                        ))?;
                    }
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(err)
            }
        }
    }

    pub fn index_note(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
        let result = (|| -> anyhow::Result<()> {
            self.remove_note(vault_name, note.path.as_str())?;
            self.index_note_inner(vault_name, note)
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(err)
            }
        }
    }

    pub fn remove_note(&self, vault_name: &str, path: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "DELETE FROM task_fields WHERE vault_name = ?1 AND task_id IN (
                SELECT id FROM tasks WHERE vault_name = ?1 AND note_path = ?2
            )",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM tasks WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM fields WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM field_values WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM tags WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM links WHERE vault_name = ?1 AND source_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM periodic_notes WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM notes WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
        )?;
        Ok(())
    }

    fn clear_vault(&self, vault_name: &str) -> anyhow::Result<()> {
        for table in [
            "task_fields",
            "tasks",
            "fields",
            "field_values",
            "tags",
            "links",
            "periodic_notes",
            "notes",
        ] {
            self.conn.execute(
                &format!("DELETE FROM {table} WHERE vault_name = ?1"),
                params![vault_name],
            )?;
        }
        Ok(())
    }

    fn index_note_inner(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let (note_type, title, created_at, updated_at) = extract_note_metadata(note);
        let body_excerpt = note.body.chars().take(500).collect::<String>();
        let word_count = note.body.split_whitespace().count() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO notes (vault_name, path, title, created_at, updated_at, word_count, mtime_unix, content_hash, body_excerpt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                vault_name,
                note.path.as_str(),
                title,
                created_at,
                updated_at,
                word_count,
                0_i64,
                note.hash.as_str(),
                body_excerpt,
            ],
        )?;

        self.index_frontmatter_fields(vault_name, note)?;
        self.index_inline_fields(vault_name, note)?;
        self.index_tags(vault_name, note)?;
        self.index_links(vault_name, note)?;
        self.index_tasks(vault_name, note)?;
        self.index_periodic_note(vault_name, note, &note_type)?;

        Ok(())
    }

    fn index_frontmatter_fields(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let Some(frontmatter) = note.frontmatter.as_ref() else {
            return Ok(());
        };

        for (key, value) in &frontmatter.fields {
            self.conn.execute(
                "INSERT INTO fields (vault_name, note_path, key, value, value_type, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'frontmatter')",
                params![
                    vault_name,
                    note.path.as_str(),
                    key,
                    yaml_value_to_string(value),
                    yaml_value_type(value),
                ],
            )?;

            // Normalized member rows: lists explode to one row per element so
            // membership queries can use the exact (vault_name, key, value) index.
            match value {
                Value::Sequence(items) => {
                    for (ordinal, item) in items.iter().enumerate() {
                        self.insert_field_value(
                            vault_name,
                            note.path.as_str(),
                            key,
                            ordinal as i64,
                            &yaml_value_to_string(item),
                            yaml_value_type(item),
                            "frontmatter",
                        )?;
                    }
                }
                other => {
                    self.insert_field_value(
                        vault_name,
                        note.path.as_str(),
                        key,
                        0,
                        &yaml_value_to_string(other),
                        yaml_value_type(other),
                        "frontmatter",
                    )?;
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_field_value(
        &self,
        vault_name: &str,
        note_path: &str,
        key: &str,
        ordinal: i64,
        value: &str,
        value_type: &str,
        source: &str,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO field_values (vault_name, note_path, key, ordinal, value, value_type, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![vault_name, note_path, key, ordinal, value, value_type, source],
        )?;
        Ok(())
    }

    fn index_inline_fields(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        for field in &note.inline_fields {
            self.conn.execute(
                "INSERT INTO fields (vault_name, note_path, key, value, value_type, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'inline')",
                params![
                    vault_name,
                    note.path.as_str(),
                    field.key.as_str(),
                    field.value.as_str(),
                    scalar_value_type(field.value.as_str()),
                ],
            )?;

            self.insert_field_value(
                vault_name,
                note.path.as_str(),
                field.key.as_str(),
                0,
                field.value.as_str(),
                scalar_value_type(field.value.as_str()),
                "inline",
            )?;
        }

        Ok(())
    }

    fn index_tags(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let mut tags = HashSet::new();
        if let Some(frontmatter) = note.frontmatter.as_ref() {
            tags.extend(frontmatter.tags());
        }
        tags.extend(extract_inline_tags(&note.body));

        for tag in tags {
            self.conn.execute(
                "INSERT OR IGNORE INTO tags (vault_name, note_path, tag) VALUES (?1, ?2, ?3)",
                params![vault_name, note.path.as_str(), tag],
            )?;
        }

        Ok(())
    }

    fn index_links(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        for link in &note.links {
            let kind = match link.link_type {
                LinkType::WikiLink => "wikilink",
                LinkType::Embed => "embed",
                LinkType::HeadingRef => "heading_ref",
                LinkType::BlockRef => "block_ref",
                LinkType::Anchor => "anchor",
                LinkType::MarkdownLink => "markdown_link",
                LinkType::ExternalLink => "external_link",
            };
            let target_path = match link.link_type {
                LinkType::WikiLink
                | LinkType::Embed
                | LinkType::HeadingRef
                | LinkType::BlockRef => Some(link.target.as_str()),
                LinkType::Anchor | LinkType::MarkdownLink | LinkType::ExternalLink => None,
            };

            self.conn.execute(
                "INSERT INTO links (vault_name, source_path, target_path, raw_target, link_text, kind, line_number)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    vault_name,
                    note.path.as_str(),
                    target_path,
                    link.target.as_str(),
                    link.display_text.as_deref(),
                    kind,
                    link.position.line as i64,
                ],
            )?;
        }

        Ok(())
    }

    fn index_tasks(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        for task in &note.tasks {
            let task_id = task_id(note.path.as_str(), task);
            self.conn.execute(
                "INSERT OR REPLACE INTO tasks (vault_name, id, note_path, line_number, text, status_char, status_group, content_hash, raw_markdown)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    vault_name,
                    task_id,
                    note.path.as_str(),
                    task.position.line as i64,
                    task.content.as_str(),
                    task.status_char.to_string(),
                    status_group_name(task),
                    task.content_hash.as_deref(),
                    render_raw_task(task),
                ],
            )?;

            for (key, value) in &task.inline_fields {
                self.conn.execute(
                    "INSERT OR REPLACE INTO task_fields (vault_name, task_id, key, value)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![vault_name, task_id, key, value],
                )?;
            }
        }

        Ok(())
    }

    fn index_periodic_note(
        &self,
        vault_name: &str,
        note: &Note,
        note_type: &str,
    ) -> anyhow::Result<()> {
        let Some(periodic) =
            extract_periodic_note(note.path.as_str(), note_type, self.periodic_config)
        else {
            return Ok(());
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO periodic_notes (vault_name, note_path, period_kind, period_key, period_start, period_end)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                vault_name,
                note.path.as_str(),
                periodic.kind,
                periodic.key,
                periodic.start,
                periodic.end,
            ],
        )?;
        Ok(())
    }
}

pub(crate) type NoteMetadata = (String, String, Option<String>, Option<String>);

pub(crate) fn extract_note_metadata(note: &Note) -> NoteMetadata {
    let frontmatter = note.frontmatter.as_ref();
    let note_type = frontmatter
        .and_then(|fm| fm.get_str("type").or_else(|| fm.get_str("kind")))
        .unwrap_or("note")
        .to_string();
    let title = frontmatter
        .and_then(Frontmatter::title)
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| note.path.stem().unwrap_or("Untitled").to_string());
    let created_at = frontmatter.and_then(|fm| fm.get_string("created"));
    let updated_at = frontmatter.and_then(|fm| fm.get_string("updated"));

    (note_type, title, created_at, updated_at)
}

fn yaml_value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn yaml_value_type(value: &Value) -> &'static str {
    match value {
        Value::Sequence(_) => "list",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(text) => scalar_value_type(text),
        _ => "string",
    }
}

fn scalar_value_type(value: &str) -> &'static str {
    if looks_like_date(value) {
        "date"
    } else if value.parse::<f64>().is_ok() {
        "number"
    } else if matches!(value, "true" | "false") {
        "boolean"
    } else if value.starts_with("[[") && value.ends_with("]]") {
        "link"
    } else {
        "string"
    }
}

fn looks_like_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.chars().enumerate().all(|(index, ch)| match index {
            4 | 7 => ch == '-',
            _ => ch.is_ascii_digit(),
        })
}

fn extract_inline_tags(body: &str) -> Vec<String> {
    let excluded = find_code_like_ranges(body);
    let mut tags = Vec::new();

    for captures in tag_regex().captures_iter(body) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        if excluded
            .iter()
            .any(|range| full.start() < range.end && full.end() > range.start)
        {
            continue;
        }
        if let Some(tag) = captures.name("tag") {
            tags.push(tag.as_str().to_string());
        }
    }

    tags
}

fn tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?m)(^|[\s(])#(?P<tag>[A-Za-z][A-Za-z0-9/_-]*)").expect("valid tag regex")
    })
}

fn find_code_like_ranges(body: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut active_fence: Option<(usize, char, usize)> = None;

    for segment in body.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + segment.len();
        let trimmed = segment
            .trim_end_matches(['\r', '\n'])
            .trim_start_matches([' ', '\t']);

        if let Some((fence_start, marker, len)) = active_fence {
            if is_fence_delimiter(trimmed, marker, len) {
                ranges.push(fence_start..line_end);
                active_fence = None;
            }
        } else if let Some((marker, len)) = parse_fence_start(trimmed) {
            active_fence = Some((line_start, marker, len));
        }

        offset = line_end;
    }

    if let Some((fence_start, _, _)) = active_fence {
        ranges.push(fence_start..body.len());
    }

    let mut inline_ranges = find_inline_code_ranges(body, &ranges);
    ranges.append(&mut inline_ranges);
    ranges.sort_by_key(|range| range.start);
    merge_ranges(ranges)
}

fn parse_fence_start(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = line.chars().take_while(|ch| *ch == marker).count();
    (len >= 3).then_some((marker, len))
}

fn is_fence_delimiter(line: &str, marker: char, len: usize) -> bool {
    let run = line.chars().take_while(|ch| *ch == marker).count();
    run >= len && line[run..].trim().is_empty()
}

fn find_inline_code_ranges(body: &str, excluded: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut active: Option<(usize, usize)> = None;
    let mut index = 0;

    while index < body.len() {
        if let Some(range) = excluded
            .iter()
            .find(|range| range.start <= index && index < range.end)
        {
            index = range.end;
            continue;
        }

        if body.as_bytes()[index] == b'`' {
            let run = body[index..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if let Some((start, delimiter_len)) = active {
                if delimiter_len == run {
                    ranges.push(start..index + run);
                    active = None;
                }
            } else {
                active = Some((index, run));
            }
            index += run;
            continue;
        }

        index += body[index..].chars().next().map_or(1, char::len_utf8);
    }

    ranges
}

fn merge_ranges(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn task_id(note_path: &str, task: &Task) -> i64 {
    let seed = format!(
        "{note_path}:{}:{}",
        task.position.line,
        task.content_hash
            .as_deref()
            .unwrap_or(task.content.as_str())
    );
    let bytes = blake3::hash(seed.as_bytes());
    let mut raw = [0_u8; 8];
    raw.copy_from_slice(&bytes.as_bytes()[..8]);
    (u64::from_le_bytes(raw) & i64::MAX as u64) as i64
}

fn status_group_name(task: &Task) -> &'static str {
    if task.status_group.is_open() {
        "open"
    } else {
        "done"
    }
}

fn render_raw_task(task: &Task) -> String {
    let mut rendered = format!("- [{}] {}", task.status_char, task.content);
    let mut fields = task.inline_fields.iter().collect::<Vec<_>>();
    fields.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in fields {
        rendered.push_str(&format!(" [{key}:: {value}]"));
    }
    rendered
}

struct PeriodicNoteRecord {
    kind: String,
    key: String,
    start: String,
    end: String,
}

fn extract_periodic_note(
    path: &str,
    note_type: &str,
    periodic_config: Option<&PeriodicConfig>,
) -> Option<PeriodicNoteRecord> {
    if let Some(config) = periodic_config {
        if let Some(periodic) = config.match_note_path(path) {
            return Some(PeriodicNoteRecord {
                kind: periodic.kind.to_string(),
                key: periodic.key,
                start: periodic.period_start.to_string(),
                end: periodic.period_end.to_string(),
            });
        }
    }

    let stem = path.rsplit('/').next()?.strip_suffix(".md")?;
    for kind in PeriodKind::ALL {
        if note_type == kind.as_str() || kind.bounds_for_key(stem).is_some() {
            let (start, end) = kind.bounds_for_key(stem)?;
            return Some(PeriodicNoteRecord {
                kind: kind.to_string(),
                key: stem.to_string(),
                start: start.to_string(),
                end: end.to_string(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::create_schema;
    use notesmith_core::{
        Frontmatter, InlineField, Link, LinkType, Note, SourcePosition, StatusGroup, Task,
        VaultName, VaultPath,
    };
    use rusqlite::{Connection, params};
    use std::collections::HashMap;

    const VAULT_NAME: &str = "test";

    #[test]
    fn index_all_uses_per_note_savepoints_to_keep_other_notes() {
        let conn = test_connection();
        // The indexer only sees already-parsed frontmatter, so simulate a malformed note
        // by forcing field insertion to fail for a single note inside its savepoint.
        conn.execute_batch(
            "
            CREATE TRIGGER fail_bad_frontmatter
            BEFORE INSERT ON fields
            WHEN NEW.note_path = 'bad.md' AND NEW.key = 'explode'
            BEGIN
                SELECT RAISE(FAIL, 'simulated malformed frontmatter');
            END;
            ",
        )
        .unwrap();

        let mut good_one = make_note("good-one.md", "good one body");
        good_one.frontmatter = Some(make_frontmatter("title: Good One\ncategory: alpha\n"));

        let mut bad = make_note("bad.md", "bad body");
        bad.frontmatter = Some(make_frontmatter("title: Bad\nexplode: yes\n"));

        let mut good_two = make_note("good-two.md", "good two body");
        good_two.frontmatter = Some(make_frontmatter("title: Good Two\ncategory: beta\n"));

        CacheIndexer::new(&conn)
            .index_all(VAULT_NAME, &[good_one, bad, good_two])
            .unwrap();

        let paths = query_note_paths(&conn);
        assert_eq!(
            paths,
            vec!["good-one.md".to_string(), "good-two.md".to_string()]
        );

        let field_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fields WHERE vault_name = ?1 AND note_path = 'bad.md'",
                [VAULT_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(field_count, 0);
    }

    #[test]
    fn index_note_extracts_frontmatter_fields_with_value_types() {
        let conn = test_connection();
        let mut note = make_note("frontmatter.md", "frontmatter body");
        note.frontmatter = Some(make_frontmatter(
            "title: Frontmatter Example\nsummary: hello world\ncount: 7\nitems:\n  - alpha\n  - beta\nnested:\n  owner: surdy\n  priority: 3\n",
        ));

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT key, value, value_type, source
                 FROM fields
                 WHERE vault_name = ?1 AND note_path = ?2
                 ORDER BY key",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![VAULT_NAME, note.path.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(rows.len(), 5);
        assert!(rows.contains(&(
            "count".to_string(),
            "7".to_string(),
            "number".to_string(),
            "frontmatter".to_string(),
        )));
        assert!(rows.contains(&(
            "items".to_string(),
            "- alpha\n- beta".to_string(),
            "list".to_string(),
            "frontmatter".to_string(),
        )));
        assert!(rows.contains(&(
            "summary".to_string(),
            "hello world".to_string(),
            "string".to_string(),
            "frontmatter".to_string(),
        )));

        let nested = rows
            .iter()
            .find(|(key, _, _, _)| key == "nested")
            .expect("nested field indexed");
        assert_eq!(nested.2, "string");
        assert_eq!(nested.3, "frontmatter");
        assert!(nested.1.contains("owner: surdy"));
        assert!(nested.1.contains("priority: 3"));
    }

    #[test]
    fn v_field_values_explodes_list_fields_one_row_per_member() {
        let conn = test_connection();
        let mut note = make_note("Meetings/2026/07/acme-renewal.md", "renewal body");
        note.frontmatter = Some(make_frontmatter(
            "kind: meeting\naudience: internal\ndate: 2026-07-17\ncustomers:\n  - \"[[Acme]]\"\n  - \"[[Globex]]\"\nstreams: []\n",
        ));

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let rows = query_field_values(&conn, note.path.as_str());

        let customers: Vec<_> = rows.iter().filter(|row| row.0 == "customers").collect();
        assert_eq!(
            customers,
            vec![
                &(
                    "customers".to_string(),
                    0,
                    "[[Acme]]".to_string(),
                    "link".to_string(),
                    "frontmatter".to_string(),
                ),
                &(
                    "customers".to_string(),
                    1,
                    "[[Globex]]".to_string(),
                    "link".to_string(),
                    "frontmatter".to_string(),
                ),
            ]
        );

        // Scalars appear in the same view (uniform query surface), ordinal 0.
        assert!(rows.contains(&(
            "audience".to_string(),
            0,
            "internal".to_string(),
            "string".to_string(),
            "frontmatter".to_string(),
        )));
        assert!(rows.contains(&(
            "date".to_string(),
            0,
            "2026-07-17".to_string(),
            "date".to_string(),
            "frontmatter".to_string(),
        )));

        // A zero-item list contributes no member rows.
        assert!(rows.iter().all(|row| row.0 != "streams"));
    }

    #[test]
    fn v_field_values_exact_membership_has_no_substring_false_positives() {
        let conn = test_connection();
        let mut acme = make_note("acme-meeting.md", "acme body");
        acme.frontmatter = Some(make_frontmatter("customers:\n  - \"[[Acme]]\"\n"));
        let mut acme_corp = make_note("acme-corp-meeting.md", "acme corp body");
        acme_corp.frontmatter = Some(make_frontmatter("customers:\n  - \"[[AcmeCorp]]\"\n"));

        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(VAULT_NAME, &acme).unwrap();
        indexer.index_note(VAULT_NAME, &acme_corp).unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT note_path FROM v_field_values
                 WHERE vault_name = ?1 AND key = 'customers' AND value = '[[Acme]]'
                 ORDER BY note_path",
            )
            .unwrap();
        let paths = stmt
            .query_map([VAULT_NAME], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(paths, vec!["acme-meeting.md".to_string()]);
    }

    #[test]
    fn v_field_values_includes_inline_fields_and_serializes_nested_elements() {
        let conn = test_connection();
        let mut note = make_note("mixed.md", "mixed body");
        note.frontmatter = Some(make_frontmatter(
            "mixed:\n  - plain\n  - owner: surdy\n  - - a\n    - b\n",
        ));
        note.inline_fields = vec![make_inline_field("owner", "alice", 2)];

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let rows = query_field_values(&conn, note.path.as_str());

        assert!(rows.contains(&(
            "owner".to_string(),
            0,
            "alice".to_string(),
            "string".to_string(),
            "inline".to_string(),
        )));

        let mixed: Vec<_> = rows.iter().filter(|row| row.0 == "mixed").collect();
        assert_eq!(mixed.len(), 3);
        assert_eq!(
            (&mixed[0].1, mixed[0].2.as_str(), mixed[0].3.as_str()),
            (&0, "plain", "string")
        );
        // A nested map element is serialized without panicking.
        assert_eq!(mixed[1].1, 1);
        assert!(mixed[1].2.contains("owner: surdy"));
        assert_eq!(mixed[1].3, "string");
        // A nested list element stays a serialized list.
        assert_eq!(
            (&mixed[2].1, mixed[2].2.as_str(), mixed[2].3.as_str()),
            (&2, "- a\n- b", "list")
        );
    }

    #[test]
    fn v_task_effective_fields_inherits_note_frontmatter_and_lets_task_override() {
        let conn = test_connection();
        let mut note = make_note("Meetings/2026/07/acme-sync.md", "sync body");
        note.frontmatter = Some(make_frontmatter(
            "kind: meeting\ndate: 2026-07-17\ncustomers:\n  - \"[[Acme]]\"\n  - \"[[Globex]]\"\n",
        ));

        let mut inherits = make_task(' ', StatusGroup::Open, "send proposal", 10);
        inherits
            .inline_fields
            .insert("due".to_string(), "2026-07-24".to_string());
        let mut overrides = make_task(' ', StatusGroup::Open, "side quest", 12);
        overrides
            .inline_fields
            .insert("customers".to_string(), "[[Solo]]".to_string());
        note.tasks = vec![inherits.clone(), overrides.clone()];

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let effective = |task: &Task| -> Vec<(String, String, String)> {
            let mut stmt = conn
                .prepare(
                    "SELECT f.key, f.value, f.source
                     FROM v_task_effective_fields f
                     JOIN tasks t ON t.vault_name = f.vault_name AND t.id = f.task_id
                     WHERE f.vault_name = ?1 AND t.text = ?2
                     ORDER BY f.key, f.value",
                )
                .unwrap();
            stmt.query_map(params![VAULT_NAME, task.content.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };

        // First task inherits the note's list members and keeps its own due date.
        // (Rows come back ordered by key, value.)
        assert_eq!(
            effective(&inherits),
            vec![
                (
                    "customers".to_string(),
                    "[[Acme]]".to_string(),
                    "note".to_string()
                ),
                (
                    "customers".to_string(),
                    "[[Globex]]".to_string(),
                    "note".to_string()
                ),
                (
                    "date".to_string(),
                    "2026-07-17".to_string(),
                    "note".to_string()
                ),
                ("due".to_string(), "2026-07-24".to_string(), "task".to_string()),
                (
                    "kind".to_string(),
                    "meeting".to_string(),
                    "note".to_string()
                ),
            ]
        );

        // Second task's task-level customers wins: no inherited customer rows at all.
        let rows = effective(&overrides);
        let customers: Vec<_> = rows.iter().filter(|row| row.0 == "customers").collect();
        assert_eq!(
            customers,
            vec![&(
                "customers".to_string(),
                "[[Solo]]".to_string(),
                "task".to_string()
            )]
        );
        // Non-overridden note fields still inherited.
        assert!(rows.contains(&(
            "date".to_string(),
            "2026-07-17".to_string(),
            "note".to_string()
        )));
    }

    #[test]
    fn v_task_effective_fields_does_not_inherit_note_inline_fields() {
        let conn = test_connection();
        let mut note = make_note("decisions.md", "decision body");
        note.frontmatter = Some(make_frontmatter("kind: meeting\n"));
        note.inline_fields = vec![make_inline_field("owner", "[[Alice]]", 3)];
        note.tasks = vec![make_task(' ', StatusGroup::Open, "follow up", 5)];

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let owner_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM v_task_effective_fields WHERE vault_name = ?1 AND key = 'owner'",
                [VAULT_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(owner_rows, 0, "paragraph-scoped inline fields must not leak onto tasks");
    }

    #[test]
    fn index_note_extracts_links() {
        let conn = test_connection();
        let mut note = make_note("links.md", "links body");
        note.links = vec![
            make_link(LinkType::WikiLink, "Target Note", Some("Shown"), 3),
            make_link(LinkType::Embed, "assets/image.png", None, 4),
            make_link(
                LinkType::ExternalLink,
                "https://example.com",
                Some("Example"),
                5,
            ),
        ];

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT target_path, raw_target, link_text, kind, line_number
                 FROM links
                 WHERE vault_name = ?1 AND source_path = ?2
                 ORDER BY line_number",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![VAULT_NAME, note.path.as_str()], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            rows,
            vec![
                (
                    Some("Target Note".to_string()),
                    "Target Note".to_string(),
                    Some("Shown".to_string()),
                    "wikilink".to_string(),
                    3,
                ),
                (
                    Some("assets/image.png".to_string()),
                    "assets/image.png".to_string(),
                    None,
                    "embed".to_string(),
                    4,
                ),
                (
                    None,
                    "https://example.com".to_string(),
                    Some("Example".to_string()),
                    "external_link".to_string(),
                    5,
                ),
            ]
        );
    }

    #[test]
    fn index_note_extracts_dangling_wikilinks_into_view() {
        let conn = test_connection();

        // Source note references four wikilink-style targets and two
        // non-wikilink links (external + markdown, which store NULL target_path
        // and must never appear as dangling).
        let mut source = make_note("hub.md", "hub body");
        source.links = vec![
            make_link(LinkType::WikiLink, "Existing Note", Some("here"), 1),
            make_link(LinkType::WikiLink, "Ghost Concept", None, 2),
            make_link(LinkType::Embed, "assets/missing.png", None, 3),
            make_link(LinkType::ExternalLink, "https://example.com", None, 4),
            make_link(LinkType::MarkdownLink, "./other.md", Some("other"), 5),
        ];

        // A target note that resolves the first wikilink by its stem/title.
        let existing = make_note("Existing Note.md", "existing body");

        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(VAULT_NAME, &source).unwrap();
        indexer.index_note(VAULT_NAME, &existing).unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT source_path, source_title, raw_target, kind, line_number
                 FROM v_dangling_links ORDER BY line_number",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Only the unresolved wikilink + embed are dangling; the resolved
        // wikilink and both NULL-target links are excluded.
        assert_eq!(
            rows,
            vec![
                (
                    "hub.md".to_string(),
                    Some("hub".to_string()),
                    "Ghost Concept".to_string(),
                    "wikilink".to_string(),
                    2,
                ),
                (
                    "hub.md".to_string(),
                    Some("hub".to_string()),
                    "assets/missing.png".to_string(),
                    "embed".to_string(),
                    3,
                ),
            ]
        );
    }

    #[test]
    fn dangling_link_view_honors_all_resolution_forms() {
        let conn = test_connection();

        let mut source = make_note("refs.md", "refs body");
        source.links = vec![
            make_link(LinkType::WikiLink, "By Title", None, 1),
            make_link(LinkType::WikiLink, "notes/by-path", None, 2),
            make_link(LinkType::WikiLink, "sub/deep", None, 3),
            make_link(LinkType::WikiLink, "Nowhere", None, 4),
        ];

        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(VAULT_NAME, &source).unwrap();
        // Title match (stem defaults to title), path+".md" match, and
        // "%/target.md" subdirectory match respectively resolve the first three.
        indexer
            .index_note(VAULT_NAME, &make_note("By Title.md", "a"))
            .unwrap();
        indexer
            .index_note(VAULT_NAME, &make_note("notes/by-path.md", "b"))
            .unwrap();
        indexer
            .index_note(VAULT_NAME, &make_note("wiki/sub/deep.md", "c"))
            .unwrap();

        let dangling: Vec<String> = conn
            .prepare("SELECT raw_target FROM v_dangling_links ORDER BY raw_target")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(dangling, vec!["Nowhere".to_string()]);
    }

    #[test]
    fn index_note_extracts_tasks() {
        let conn = test_connection();
        let mut note = make_note("tasks.md", "tasks body");
        note.tasks = vec![
            make_task(' ', StatusGroup::Open, "open task", 2),
            make_task('x', StatusGroup::Done, "done task", 5),
        ];

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT line_number, text, status_char, status_group, raw_markdown
                 FROM tasks
                 WHERE vault_name = ?1 AND note_path = ?2
                 ORDER BY line_number",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![VAULT_NAME, note.path.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            rows,
            vec![
                (
                    2,
                    "open task".to_string(),
                    " ".to_string(),
                    "open".to_string(),
                    "- [ ] open task".to_string(),
                ),
                (
                    5,
                    "done task".to_string(),
                    "x".to_string(),
                    "done".to_string(),
                    "- [x] done task".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn index_note_extracts_inline_fields_into_fields_table() {
        let conn = test_connection();
        let mut note = make_note("inline-fields.md", "inline body");
        note.inline_fields = vec![
            make_inline_field("owner", "alice", 2),
            make_inline_field("due", "2026-05-10", 2),
            make_inline_field("estimate", "3", 2),
            make_inline_field("related", "[[Project]]", 2),
        ];

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT key, value, value_type, source
                 FROM fields
                 WHERE vault_name = ?1 AND note_path = ?2
                 ORDER BY key",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![VAULT_NAME, note.path.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(rows.len(), 4);
        assert!(rows.contains(&(
            "due".to_string(),
            "2026-05-10".to_string(),
            "date".to_string(),
            "inline".to_string(),
        )));
        assert!(rows.contains(&(
            "estimate".to_string(),
            "3".to_string(),
            "number".to_string(),
            "inline".to_string(),
        )));
        assert!(rows.contains(&(
            "owner".to_string(),
            "alice".to_string(),
            "string".to_string(),
            "inline".to_string(),
        )));
        assert!(rows.contains(&(
            "related".to_string(),
            "[[Project]]".to_string(),
            "link".to_string(),
            "inline".to_string(),
        )));
    }

    #[test]
    fn index_note_replaces_existing_rows_for_the_same_note() {
        let conn = test_connection();
        let mut original = make_note("incremental.md", "first body");
        original.frontmatter = Some(make_frontmatter("status: draft\n"));
        original.inline_fields = vec![make_inline_field("owner", "alice", 2)];
        original.links = vec![make_link(LinkType::WikiLink, "Old Target", None, 2)];
        let mut original_task = make_task(' ', StatusGroup::Open, "first task", 2);
        original_task
            .inline_fields
            .insert("due".to_string(), "2026-05-10".to_string());
        original.tasks = vec![original_task];

        let mut updated = make_note("incremental.md", "second body with different content");
        updated.frontmatter = Some(make_frontmatter("status: published\n"));
        updated.inline_fields = vec![make_inline_field("owner", "bob", 4)];
        updated.links = vec![make_link(
            LinkType::ExternalLink,
            "https://example.com",
            None,
            4,
        )];
        updated.tasks = vec![make_task('x', StatusGroup::Done, "replacement task", 4)];

        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(VAULT_NAME, &original).unwrap();
        indexer.index_note(VAULT_NAME, &updated).unwrap();

        let field_rows = conn
            .query_row(
                "SELECT COUNT(*) FROM fields WHERE vault_name = ?1 AND note_path = ?2 AND key = 'status' AND value = 'published'",
                params![VAULT_NAME, updated.path.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(field_rows, 1);

        let field_value_rows = conn
            .query_row(
                "SELECT COUNT(*) FROM v_field_values WHERE vault_name = ?1 AND note_path = ?2 AND key = 'status'",
                params![VAULT_NAME, updated.path.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(field_value_rows, 1);
        let field_value: String = conn
            .query_row(
                "SELECT value FROM v_field_values WHERE vault_name = ?1 AND note_path = ?2 AND key = 'status'",
                params![VAULT_NAME, updated.path.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(field_value, "published");

        let old_link_rows = conn
            .query_row(
                "SELECT COUNT(*) FROM links WHERE vault_name = ?1 AND source_path = ?2 AND raw_target = 'Old Target'",
                params![VAULT_NAME, updated.path.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(old_link_rows, 0);

        let old_task_field_rows = conn
            .query_row(
                "SELECT COUNT(*) FROM task_fields WHERE vault_name = ?1 AND key = 'due'",
                [VAULT_NAME],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(old_task_field_rows, 0);

        let task_row = conn
            .query_row(
                "SELECT text, status_char, status_group FROM tasks WHERE vault_name = ?1 AND note_path = ?2",
                params![VAULT_NAME, updated.path.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            task_row,
            (
                "replacement task".to_string(),
                "x".to_string(),
                "done".to_string(),
            )
        );
    }

    #[test]
    fn remove_note_deletes_note_and_related_rows() {
        let conn = test_connection();
        let mut note = make_note("remove-me.md", "remove me body #cleanup");
        note.frontmatter = Some(make_frontmatter("status: active\ntags:\n  - cleanup\n"));
        note.inline_fields = vec![make_inline_field("owner", "alice", 2)];
        note.links = vec![make_link(LinkType::WikiLink, "Target", None, 2)];
        let mut task = make_task(' ', StatusGroup::Open, "cleanup task", 2);
        task.inline_fields
            .insert("owner".to_string(), "alice".to_string());
        note.tasks = vec![task];

        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(VAULT_NAME, &note).unwrap();
        indexer.remove_note(VAULT_NAME, note.path.as_str()).unwrap();

        for (table, column) in [
            ("notes", "path"),
            ("fields", "note_path"),
            ("field_values", "note_path"),
            ("tags", "note_path"),
            ("tasks", "note_path"),
        ] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {table} WHERE vault_name = ?1 AND {column} = ?2"
                    ),
                    params![VAULT_NAME, note.path.as_str()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "expected {table} rows to be deleted");
        }

        let link_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM links WHERE vault_name = ?1 AND source_path = ?2",
                params![VAULT_NAME, note.path.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(link_count, 0);

        let task_field_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_fields WHERE vault_name = ?1",
                [VAULT_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(task_field_count, 0);
    }

    #[test]
    fn index_note_truncates_body_excerpt_to_five_hundred_characters() {
        let conn = test_connection();
        let body = "🙂".repeat(600);
        let note = make_note("excerpt.md", &body);

        CacheIndexer::new(&conn)
            .index_note(VAULT_NAME, &note)
            .unwrap();

        let excerpt: String = conn
            .query_row(
                "SELECT body_excerpt FROM notes WHERE vault_name = ?1 AND path = ?2",
                params![VAULT_NAME, note.path.as_str()],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(excerpt, body.chars().take(500).collect::<String>());
        assert_eq!(excerpt.chars().count(), 500);
    }

    fn test_connection() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn make_note(path: &str, body: &str) -> Note {
        Note {
            vault: VaultName::new(VAULT_NAME),
            path: VaultPath::new(path),
            frontmatter: None,
            raw_frontmatter: None,
            body: body.to_string(),
            tasks: Vec::new(),
            links: Vec::new(),
            inline_fields: Vec::new(),
            blocks: Vec::new(),
            hash: blake3::hash(body.as_bytes()).to_hex().to_string(),
        }
    }

    fn make_frontmatter(yaml: &str) -> Frontmatter {
        serde_yaml::from_str(yaml).unwrap()
    }

    fn make_link(
        link_type: LinkType,
        target: &str,
        display_text: Option<&str>,
        line: usize,
    ) -> Link {
        Link {
            link_type,
            target: target.to_string(),
            display_text: display_text.map(str::to_string),
            position: SourcePosition::new(line, 1, 0, target.len()),
        }
    }

    fn make_task(status_char: char, status_group: StatusGroup, content: &str, line: usize) -> Task {
        Task {
            status_char,
            status_group,
            content: content.to_string(),
            position: SourcePosition::new(line, 1, 0, content.len()),
            inline_fields: HashMap::new(),
            content_hash: Some(blake3::hash(content.as_bytes()).to_hex().to_string()),
        }
    }

    fn make_inline_field(key: &str, value: &str, line: usize) -> InlineField {
        InlineField {
            key: key.to_string(),
            value: value.to_string(),
            position: SourcePosition::new(line, 1, 0, key.len() + value.len()),
        }
    }

    fn query_field_values(conn: &Connection, note_path: &str) -> Vec<(String, i64, String, String, String)> {
        let mut stmt = conn
            .prepare(
                "SELECT key, ordinal, value, value_type, source
                 FROM v_field_values
                 WHERE vault_name = ?1 AND note_path = ?2
                 ORDER BY key, ordinal",
            )
            .unwrap();
        stmt.query_map(params![VAULT_NAME, note_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    }

    fn query_note_paths(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT path FROM notes WHERE vault_name = ?1 ORDER BY path")
            .unwrap();
        stmt.query_map([VAULT_NAME], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
}
