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
        }

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
