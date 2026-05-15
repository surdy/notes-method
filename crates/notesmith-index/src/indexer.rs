use notesmith_core::{Frontmatter, LinkType, Note, TaskPriority, TaskStatus};
use rusqlite::{Connection, params};

pub struct CacheIndexer<'a> {
    conn: &'a Connection,
}

impl<'a> CacheIndexer<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn index_all(&self, vault_name: &str, notes: &[Note]) -> anyhow::Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;

        let result = (|| -> anyhow::Result<()> {
            self.clear_vault(vault_name)?;
            for note in notes {
                self.index_note_inner(vault_name, note)?;
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
        self.remove_note(vault_name, note.path.as_str())?;
        self.index_note_inner(vault_name, note)
    }

    pub fn remove_note(&self, vault_name: &str, path: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "DELETE FROM tasks WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM links WHERE vault_name = ?1 AND src_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM inline_fields WHERE vault_name = ?1 AND note_path = ?2",
            params![vault_name, path],
        )?;
        self.conn.execute(
            "DELETE FROM notes WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
        )?;
        Ok(())
    }

    fn clear_vault(&self, vault_name: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "DELETE FROM tasks WHERE vault_name = ?1",
            params![vault_name],
        )?;
        self.conn.execute(
            "DELETE FROM links WHERE vault_name = ?1",
            params![vault_name],
        )?;
        self.conn.execute(
            "DELETE FROM inline_fields WHERE vault_name = ?1",
            params![vault_name],
        )?;
        self.conn.execute(
            "DELETE FROM notes WHERE vault_name = ?1",
            params![vault_name],
        )?;
        Ok(())
    }

    fn index_note_inner(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let (
            note_type,
            title,
            customer,
            stream,
            state,
            status,
            date,
            created_at,
            updated_at,
            archived,
        ) = extract_note_metadata(note);
        let frontmatter_json = serialize_frontmatter_json(note)?;

        let body_excerpt = note.body.chars().take(500).collect::<String>();

        self.conn.execute(
            "INSERT OR REPLACE INTO notes (vault_name, path, title, type, frontmatter_json, customer, stream, state, status, date, created_at, updated_at, archived, mtime_unix, content_hash, body_excerpt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                vault_name,
                note.path.as_str(),
                title,
                note_type,
                frontmatter_json,
                customer,
                stream,
                state,
                status,
                date,
                created_at,
                updated_at,
                i32::from(archived),
                0_i64,
                note.hash.as_str(),
                body_excerpt
            ],
        )?;

        for link in &note.links {
            let (kind, heading_ref, block_ref) = match link.link_type {
                LinkType::WikiLink => ("wikilink", None, None),
                LinkType::Embed => ("embed", None, None),
                LinkType::HeadingRef => ("heading_ref", Some(link.target.as_str()), None),
                LinkType::BlockRef => ("block_ref", None, Some(link.target.as_str())),
                LinkType::Anchor => ("anchor", None, None),
                LinkType::MarkdownLink => ("markdown_link", None, None),
                LinkType::ExternalLink => ("external_link", None, None),
            };
            let dst_path = match link.link_type {
                LinkType::WikiLink
                | LinkType::Embed
                | LinkType::HeadingRef
                | LinkType::BlockRef => Some(link.target.as_str()),
                LinkType::Anchor | LinkType::MarkdownLink | LinkType::ExternalLink => None,
            };

            self.conn.execute(
                "INSERT INTO links (vault_name, src_path, dst_path, raw_target, kind, heading_ref, block_ref)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    vault_name,
                    note.path.as_str(),
                    dst_path,
                    link.target.as_str(),
                    kind,
                    heading_ref,
                    block_ref
                ],
            )?;
        }

        for field in &note.inline_fields {
            self.conn.execute(
                "INSERT INTO inline_fields (vault_name, note_path, key, value, value_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    vault_name,
                    note.path.as_str(),
                    field.key.as_str(),
                    field.value.as_str(),
                    Option::<String>::None
                ],
            )?;
        }

        for (ordinal, task) in note.tasks.iter().enumerate() {
            let status_str = match task.status {
                TaskStatus::Todo => "todo",
                TaskStatus::InProgress => "in_progress",
                TaskStatus::Blocked => "blocked",
                TaskStatus::Waiting => "waiting",
                TaskStatus::OnHold => "on_hold",
                TaskStatus::Done => "done",
                TaskStatus::Cancelled => "cancelled",
            };

            let (task_customer, task_stream, task_owner) =
                extract_task_inline_fields(task.content.as_str());

            let priority_int = task.priority.map(|priority| match priority {
                TaskPriority::Highest => 5,
                TaskPriority::High => 4,
                TaskPriority::Medium => 3,
                TaskPriority::Low => 2,
                TaskPriority::Lowest => 1,
            });

            let task_hash = task.content_hash.as_deref().unwrap_or_default();

            self.conn.execute(
                "INSERT OR REPLACE INTO tasks (vault_name, task_hash, note_path, heading_path, ordinal, status, text, customer, stream, owner, due, scheduled, start_date, done_at, priority, recurrence, raw_markdown)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    vault_name,
                    task_hash,
                    note.path.as_str(),
                    Option::<String>::None,
                    ordinal as i32,
                    status_str,
                    task.content.as_str(),
                    task_customer,
                    task_stream,
                    task_owner,
                    task.due_date.map(|date| date.to_string()),
                    task.scheduled_date.map(|date| date.to_string()),
                    task.start_date.map(|date| date.to_string()),
                    task.done_date.map(|date| date.to_string()),
                    priority_int,
                    task.recurrence.clone(),
                    task.content.as_str()
                ],
            )?;
        }

        Ok(())
    }
}

fn serialize_frontmatter_json(note: &Note) -> anyhow::Result<String> {
    if let Some(raw_frontmatter) = note.raw_frontmatter.as_deref() {
        if raw_frontmatter.trim().is_empty() {
            return Ok("{}".to_string());
        }
        let value: serde_yaml::Value = serde_yaml::from_str(raw_frontmatter)?;
        return serde_json::to_string(&value).map_err(Into::into);
    }

    note.frontmatter
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map(|value| value.unwrap_or_else(|| "{}".to_string()))
        .map_err(Into::into)
}

pub(crate) type NoteMetadata = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

pub(crate) fn extract_note_metadata(note: &Note) -> NoteMetadata {
    let title = note.path.stem().unwrap_or("Untitled").to_string();

    match &note.frontmatter {
        Some(Frontmatter::Daily(meta)) => (
            "daily".into(),
            title,
            None,
            None,
            None,
            None,
            Some(meta.date.to_string()),
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Meeting(meta)) => (
            "meeting".into(),
            title,
            Some(meta.customer.clone()),
            meta.stream.clone(),
            None,
            None,
            Some(meta.date.to_string()),
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Stream(meta)) => (
            "stream".into(),
            title,
            Some(meta.customer.clone()),
            Some(meta.stream.clone()),
            None,
            serde_json::to_value(&meta.status)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned)),
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Customer(meta)) => (
            "customer".into(),
            title,
            Some(meta.customer.clone()),
            None,
            serde_json::to_value(&meta.state)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned)),
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::AccountInfo(meta)) => (
            "account-info".into(),
            title,
            Some(meta.customer.clone()),
            None,
            None,
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Glossary(meta)) => (
            "glossary".into(),
            title,
            Some(meta.customer.clone()),
            None,
            None,
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Milestones(meta)) => (
            "milestones".into(),
            title,
            Some(meta.customer.clone()),
            None,
            None,
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Note(meta)) => (
            "note".into(),
            title,
            None,
            None,
            None,
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Dashboard(meta)) => (
            "dashboard".into(),
            title,
            None,
            None,
            None,
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Contact(meta)) => (
            "contact".into(),
            title,
            Some(meta.customer.clone()),
            None,
            None,
            None,
            None,
            meta.common.created.clone(),
            meta.common.updated.clone(),
            meta.common.archived.unwrap_or(false),
        ),
        Some(Frontmatter::Other) | None => (
            "other".into(),
            title,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
        ),
    }
}

fn extract_task_inline_fields(content: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut customer = None;
    let mut stream = None;
    let mut owner = None;
    let mut offset = 0;

    while let Some(start) = content[offset..].find('[').map(|index| offset + index) {
        if content[start..].starts_with("[[") {
            offset = start + 2;
            continue;
        }

        let Some(end) = content[start + 1..]
            .find(']')
            .map(|index| start + 1 + index)
        else {
            break;
        };

        if let Some((key, value)) = content[start + 1..end].split_once("::") {
            let value = value.trim().to_string();
            match key.trim() {
                "customer" => customer = Some(value),
                "stream" => stream = Some(value),
                "owner" => owner = Some(value),
                _ => {}
            }
        }

        offset = end + 1;
    }

    (customer, stream, owner)
}
