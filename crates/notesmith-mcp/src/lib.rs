//! notesmith-mcp: MCP adapter wrapping vault operations for MCP clients.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use chrono::{Local, NaiveDate};
use notesmith_config::VaultConfig;
use notesmith_core::{Note, NotesmithError, VaultEngine, VaultName, VaultPath, WriteResult};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_query::execute_sql;
use notesmith_routing::RoutingEngine;
use notesmith_tasks::toggle_task;
use notesmith_vault::{NativeVaultEngine, apply_save_pipeline, parse_note};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext,
};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use serde_yaml::{Mapping, Value as YamlValue};

pub struct NotesmithMcp {
    vault_name: String,
    vault_root: PathBuf,
    engine: NativeVaultEngine,
    cache: Arc<VaultCache>,
    search_index: Arc<SearchIndex>,
    template_engine: Arc<notesmith_templates::TemplateEngine>,
    vault_config: VaultConfig,
}

#[derive(Debug, Deserialize)]
struct CreateNoteParams {
    title: String,
    content: Option<String>,
    folder: Option<String>,
    frontmatter: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
struct GetNoteParams {
    path: String,
}

#[derive(Debug, Deserialize)]
struct UpdateNoteParams {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AppendToNoteParams {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ArchiveNoteParams {
    path: String,
}

#[derive(Debug, Deserialize)]
struct SearchNotesParams {
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct QuerySqlParams {
    sql: String,
}

#[derive(Debug, Deserialize)]
struct ListNotesParams {
    #[serde(rename = "type")]
    note_type: Option<String>,
    customer: Option<String>,
    archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListTasksParams {
    status: Option<String>,
    customer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateTaskStatusParams {
    note_path: String,
    task_hash: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct InboxAddParams {
    content: String,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateDailyNoteParams {
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateFromTemplateParams {
    template_name: String,
    prompts: Option<HashMap<String, String>>,
}

impl NotesmithMcp {
    pub fn new(
        vault_name: String,
        vault_root: PathBuf,
        cache: VaultCache,
        search_index: SearchIndex,
        vault_config: VaultConfig,
    ) -> Self {
        let template_engine = notesmith_templates::TemplateEngine::new(vault_root.clone(), None);
        Self {
            vault_name,
            vault_root,
            engine: NativeVaultEngine,
            cache: Arc::new(cache),
            search_index: Arc::new(search_index),
            template_engine: Arc::new(template_engine),
            vault_config,
        }
    }

    pub fn create_note(
        &self,
        title: &str,
        content: Option<&str>,
        folder: Option<&str>,
        frontmatter: Option<&Map<String, Value>>,
    ) -> anyhow::Result<Value> {
        let folder = folder.unwrap_or("Inbox");
        let note_path = VaultPath::new(format!("{folder}/{title}.md"));
        self.ensure_note_missing(&note_path)?;

        let initial_content = build_note_document(frontmatter, content)?;
        let content = apply_save_pipeline(&initial_content);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    pub fn get_note(&self, path: &str) -> anyhow::Result<Value> {
        let note_path = VaultPath::new(path.to_string());
        let content = self.engine.read(&self.vault_root, &note_path)?;
        let parsed = parse_note(
            &VaultName::new(self.vault_name.clone()),
            &note_path,
            &content,
        );

        Ok(json!({
            "path": note_path.as_str(),
            "content": content,
            "frontmatter": parsed.frontmatter,
        }))
    }

    pub fn update_note(&self, path: &str, content: &str) -> anyhow::Result<Value> {
        let note_path = VaultPath::new(path.to_string());
        let content = apply_save_pipeline(content);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    pub fn append_to_note(&self, path: &str, content: &str) -> anyhow::Result<Value> {
        let note_path = VaultPath::new(path.to_string());
        let current_content = self.engine.read(&self.vault_root, &note_path)?;
        let separator = if current_content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let appended = format!("{current_content}{separator}{content}");
        let content = apply_save_pipeline(&appended);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    pub fn archive_note(&self, path: &str) -> anyhow::Result<Value> {
        let routing = RoutingEngine::load(&self.vault_root)?;
        let result = routing.apply(&self.vault_root, path, &self.engine)?;
        self.remove_from_indexes(path)?;
        self.refresh_indexes(&VaultPath::new(result.to.clone()))?;
        Ok(serde_json::to_value(result)?)
    }

    pub fn search_notes(&self, query: &str, limit: Option<usize>) -> anyhow::Result<Value> {
        let results = self.search_index.search(query, limit.unwrap_or(20))?;
        Ok(serde_json::to_value(results)?)
    }

    pub fn query_sql(&self, sql: &str) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(execute_sql(&self.cache, sql)?)?)
    }

    pub fn list_notes(
        &self,
        note_type: Option<&str>,
        customer: Option<&str>,
        archived: Option<bool>,
    ) -> anyhow::Result<Value> {
        let conn = self.cache.connection();
        let mut stmt = conn.prepare(
            "SELECT path, title, created_at, updated_at, mtime_unix FROM notes ORDER BY path",
        )?;
        let base_rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut rows = Vec::new();
        for (path, title, created_at, updated_at, mtime_unix) in base_rows {
            let frontmatter = load_note_frontmatter(&conn, &self.vault_name, &path)?;
            let resolved_type =
                frontmatter_string(&frontmatter, "type").unwrap_or_else(|| "note".to_string());
            let resolved_customer = frontmatter_string(&frontmatter, "customer");
            let resolved_archived = frontmatter_bool(&frontmatter, "archived");
            if note_type.is_some_and(|expected| expected != resolved_type) {
                continue;
            }
            if customer.is_some_and(|expected| resolved_customer.as_deref() != Some(expected)) {
                continue;
            }
            if archived.is_some_and(|expected| expected != resolved_archived) {
                continue;
            }

            rows.push(json!({
                "path": path,
                "title": title,
                "type": resolved_type,
                "customer": resolved_customer,
                "stream": frontmatter_string(&frontmatter, "stream"),
                "state": frontmatter_string(&frontmatter, "state"),
                "status": frontmatter_string(&frontmatter, "status"),
                "date": frontmatter_string(&frontmatter, "date"),
                "created_at": created_at,
                "updated_at": updated_at,
                "archived": resolved_archived,
                "mtime_unix": mtime_unix,
                "frontmatter": frontmatter,
            }));
        }

        Ok(Value::Array(rows))
    }

    pub fn list_tasks(
        &self,
        status: Option<&str>,
        customer: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut conditions = vec!["1=1".to_string()];
        if let Some(status) = status {
            let status_char = parse_status_str(status).map_err(anyhow::Error::msg)?;
            conditions.push(format!(
                "t.status_char = '{}'",
                escape_sql_string(&status_char.to_string())
            ));
        }
        if let Some(customer) = customer {
            conditions.push(format!(
                "customer.value = '{}'",
                escape_sql_string(customer)
            ));
        }

        let sql = format!(
            "SELECT t.content_hash, t.note_path, t.line_number, t.status_char, t.status_group, t.text, n.title, customer.value, stream.value, owner.value, due.value, priority.value \
             FROM tasks t \
             JOIN notes n ON n.vault_name = t.vault_name AND n.path = t.note_path \
             LEFT JOIN task_fields customer ON customer.vault_name = t.vault_name AND customer.task_id = t.id AND customer.key = 'customer' \
             LEFT JOIN task_fields stream ON stream.vault_name = t.vault_name AND stream.task_id = t.id AND stream.key = 'stream' \
             LEFT JOIN task_fields owner ON owner.vault_name = t.vault_name AND owner.task_id = t.id AND owner.key = 'owner' \
             LEFT JOIN task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due' \
             LEFT JOIN task_fields priority ON priority.vault_name = t.vault_name AND priority.task_id = t.id AND priority.key = 'priority' \
             WHERE {} ORDER BY due.value IS NULL, due.value ASC, t.line_number ASC",
            conditions.join(" AND ")
        );

        let conn = self.cache.connection();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map([], |row| {
                let status_char: String = row.get(3)?;
                Ok(json!({
                    "task_hash": row.get::<_, Option<String>>(0)?,
                    "note_path": row.get::<_, String>(1)?,
                    "line_number": row.get::<_, i64>(2)?,
                    "status": status_name_for_char(&status_char),
                    "status_char": status_char,
                    "status_group": row.get::<_, String>(4)?,
                    "text": row.get::<_, String>(5)?,
                    "note_title": row.get::<_, Option<String>>(6)?,
                    "customer": row.get::<_, Option<String>>(7)?,
                    "stream": row.get::<_, Option<String>>(8)?,
                    "owner": row.get::<_, Option<String>>(9)?,
                    "due": row.get::<_, Option<String>>(10)?,
                    "priority": row.get::<_, Option<String>>(11)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Value::Array(rows))
    }

    pub fn update_task_status(
        &self,
        note_path: &str,
        task_hash: &str,
        status: &str,
    ) -> anyhow::Result<Value> {
        let new_status = parse_status_str(status).map_err(anyhow::Error::msg)?;
        let note_path = VaultPath::new(note_path.to_string());
        let current_content = self.engine.read(&self.vault_root, &note_path)?;
        let updated = toggle_task(&current_content, task_hash, new_status)?;
        let content = apply_save_pipeline(&updated);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
            "status": status,
        }))
    }

    pub fn inbox_add(&self, content: &str, title: Option<&str>) -> anyhow::Result<Value> {
        let capture_folder = &self.vault_config.capture.folder;
        let timestamp = Local::now().format("%Y-%m-%d %H-%M-%S").to_string();
        let slug = match title {
            Some(title) => sanitize_slug(title),
            None => sanitize_slug(&content.chars().take(40).collect::<String>()),
        };
        let filename = if slug.is_empty() {
            format!("{timestamp}.md")
        } else {
            format!("{timestamp} - {slug}.md")
        };
        let note_path = if capture_folder.is_empty() {
            VaultPath::new(filename)
        } else {
            VaultPath::new(format!("{capture_folder}/{filename}"))
        };
        let hash = self.write_content(&note_path, None, content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    pub fn create_daily_note(&self, date: Option<&str>) -> anyhow::Result<Value> {
        let parsed_date = match date {
            Some(date) => NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .with_context(|| format!("invalid date: {date}"))?,
            None => Local::now().date_naive(),
        };
        let date_str = parsed_date.format("%Y-%m-%d").to_string();
        let note_path = VaultPath::new(format!(
            "{}/{}.md",
            self.vault_config.daily.folder, date_str
        ));

        match self.engine.read(&self.vault_root, &note_path) {
            Ok(_) => {
                return Ok(json!({
                    "path": note_path.as_str(),
                    "created": false,
                }));
            }
            Err(NotesmithError::NoteNotFound { .. }) => {}
            Err(error) => return Err(error.into()),
        }

        let mut prompts = HashMap::new();
        prompts.insert("today".to_string(), date_str);
        let rendered = self.template_engine.instantiate(
            &self.vault_config.daily.template,
            &prompts,
            &self.engine,
        )?;
        let rendered_path = VaultPath::new(rendered.path.clone());
        self.refresh_indexes(&rendered_path)?;

        Ok(json!({
            "path": rendered.path,
            "created": true,
        }))
    }

    pub fn create_from_template(
        &self,
        template_name: &str,
        prompts: Option<HashMap<String, String>>,
    ) -> anyhow::Result<Value> {
        let rendered = self.template_engine.instantiate(
            template_name,
            &prompts.unwrap_or_default(),
            &self.engine,
        )?;
        let note_path = VaultPath::new(rendered.path.clone());
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": rendered.path,
            "content": rendered.content,
        }))
    }

    fn registered_tools(&self) -> Vec<Tool> {
        vec![
            tool_definition(
                "create_note",
                "Create a new note in the vault",
                json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "content": {"type": "string"},
                        "folder": {"type": "string"},
                        "frontmatter": {"type": "object"}
                    },
                    "required": ["title"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "get_note",
                "Read a note by vault-relative path",
                json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "update_note",
                "Replace a note's content",
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "append_to_note",
                "Append content to an existing note",
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "archive_note",
                "Apply routing rules and archive a note",
                json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "search_notes",
                "Search notes by title and body content",
                json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1}
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "query_sql",
                "Execute read-only SQL against the vault cache",
                json!({
                    "type": "object",
                    "properties": {"sql": {"type": "string"}},
                    "required": ["sql"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "list_notes",
                "List notes with optional type, customer, and archive filters",
                json!({
                    "type": "object",
                    "properties": {
                        "type": {"type": "string"},
                        "customer": {"type": "string"},
                        "archived": {"type": "boolean"}
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "list_tasks",
                "List tasks with optional status and customer filters",
                json!({
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "customer": {"type": "string"}
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "update_task_status",
                "Set the status of a task in a note",
                json!({
                    "type": "object",
                    "properties": {
                        "note_path": {"type": "string"},
                        "task_hash": {"type": "string"},
                        "status": {"type": "string"}
                    },
                    "required": ["note_path", "task_hash", "status"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "inbox_add",
                "Quick-capture content into the inbox folder",
                json!({
                    "type": "object",
                    "properties": {
                        "content": {"type": "string"},
                        "title": {"type": "string"}
                    },
                    "required": ["content"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "create_daily_note",
                "Ensure a daily note exists for a date",
                json!({
                    "type": "object",
                    "properties": {
                        "date": {"type": "string", "description": "YYYY-MM-DD"}
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "create_from_template",
                "Instantiate a note from a configured template",
                json!({
                    "type": "object",
                    "properties": {
                        "template_name": {"type": "string"},
                        "prompts": {"type": "object", "additionalProperties": {"type": "string"}}
                    },
                    "required": ["template_name"],
                    "additionalProperties": false
                }),
            ),
        ]
    }

    fn registered_resources(&self) -> Vec<Resource> {
        vec![
            resource_definition(
                "note:///{vault-path}",
                "note",
                Some("Read an individual note by vault-relative path"),
            ),
            resource_definition(
                "note:///daily/{date}",
                "daily-note",
                Some("Read a daily note by date (YYYY-MM-DD)"),
            ),
            resource_definition(
                "note:///vault/structure",
                "vault-structure",
                Some("List all note paths in the vault"),
            ),
        ]
    }

    fn read_resource_value(&self, uri: &str) -> anyhow::Result<String> {
        if uri == "note:///vault/structure" {
            let notes = self.engine.scan(&self.vault_root)?;
            let paths = notes
                .into_iter()
                .map(|note| note.path.to_string())
                .collect::<Vec<_>>();
            return Ok(serde_json::to_string_pretty(&paths)?);
        }

        if let Some(date) = uri.strip_prefix("note:///daily/") {
            let path = format!("{}/{}.md", self.vault_config.daily.folder, date);
            return Ok(self.engine.read(&self.vault_root, &VaultPath::new(path))?);
        }

        if let Some(path) = uri.strip_prefix("note:///") {
            return Ok(self
                .engine
                .read(&self.vault_root, &VaultPath::new(path.to_string()))?);
        }

        anyhow::bail!("unknown resource: {uri}")
    }

    fn refresh_indexes(&self, path: &VaultPath) -> anyhow::Result<()> {
        let note = self.load_note(path)?;
        self.cache.update_note(&self.vault_name, &note)?;
        self.search_index.update_note(&self.vault_name, &note)?;
        Ok(())
    }

    fn remove_from_indexes(&self, path: &str) -> anyhow::Result<()> {
        self.cache.remove_note(&self.vault_name, path)?;
        self.search_index.remove_note(&self.vault_name, path)?;
        Ok(())
    }

    fn load_note(&self, path: &VaultPath) -> anyhow::Result<Note> {
        let content = self.engine.read(&self.vault_root, path)?;
        Ok(parse_note(
            &VaultName::new(self.vault_name.clone()),
            path,
            &content,
        ))
    }

    fn ensure_note_missing(&self, path: &VaultPath) -> anyhow::Result<()> {
        match self.engine.read(&self.vault_root, path) {
            Ok(_) => anyhow::bail!("note already exists: {}", path.as_str()),
            Err(NotesmithError::NoteNotFound { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_content(
        &self,
        path: &VaultPath,
        expected_hash: Option<&str>,
        content: &str,
    ) -> anyhow::Result<String> {
        match self
            .engine
            .write(&self.vault_root, path, expected_hash, content)?
        {
            WriteResult::Written { hash } => Ok(hash),
            WriteResult::Conflict { expected, actual } => anyhow::bail!(
                "write conflict for {} (expected {}, actual {})",
                path.as_str(),
                expected,
                actual
            ),
        }
    }

    fn handle_tool_call<T, F>(&self, arguments: Option<Map<String, Value>>, f: F) -> CallToolResult
    where
        T: for<'de> Deserialize<'de>,
        F: FnOnce(T) -> anyhow::Result<Value>,
    {
        match parse_arguments(arguments).and_then(f) {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::error(vec![Content::text(error.to_string())]),
        }
    }
}

impl ServerHandler for NotesmithMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "notesmith".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Notesmith MCP".to_string()),
                description: Some("MCP adapter for Notesmith vault operations".to_string()),
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            instructions: Some(
                "Use Notesmith tools to read, create, search, route, and update vault notes."
                    .to_string(),
            ),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.registered_tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.registered_tools()
            .into_iter()
            .find(|tool| tool.name.as_ref() == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let result = match request.name.as_ref() {
            "create_note" => {
                self.handle_tool_call::<CreateNoteParams, _>(request.arguments, |params| {
                    self.create_note(
                        &params.title,
                        params.content.as_deref(),
                        params.folder.as_deref(),
                        params.frontmatter.as_ref(),
                    )
                })
            }
            "get_note" => self.handle_tool_call::<GetNoteParams, _>(request.arguments, |params| {
                self.get_note(&params.path)
            }),
            "update_note" => self
                .handle_tool_call::<UpdateNoteParams, _>(request.arguments, |params| {
                    self.update_note(&params.path, &params.content)
                }),
            "append_to_note" => self
                .handle_tool_call::<AppendToNoteParams, _>(request.arguments, |params| {
                    self.append_to_note(&params.path, &params.content)
                }),
            "archive_note" => self
                .handle_tool_call::<ArchiveNoteParams, _>(request.arguments, |params| {
                    self.archive_note(&params.path)
                }),
            "search_notes" => self
                .handle_tool_call::<SearchNotesParams, _>(request.arguments, |params| {
                    self.search_notes(&params.query, params.limit)
                }),
            "query_sql" => self
                .handle_tool_call::<QuerySqlParams, _>(request.arguments, |params| {
                    self.query_sql(&params.sql)
                }),
            "list_notes" => {
                self.handle_tool_call::<ListNotesParams, _>(request.arguments, |params| {
                    self.list_notes(
                        params.note_type.as_deref(),
                        params.customer.as_deref(),
                        params.archived,
                    )
                })
            }
            "list_tasks" => self
                .handle_tool_call::<ListTasksParams, _>(request.arguments, |params| {
                    self.list_tasks(params.status.as_deref(), params.customer.as_deref())
                }),
            "update_task_status" => {
                self.handle_tool_call::<UpdateTaskStatusParams, _>(request.arguments, |params| {
                    self.update_task_status(&params.note_path, &params.task_hash, &params.status)
                })
            }
            "inbox_add" => self
                .handle_tool_call::<InboxAddParams, _>(request.arguments, |params| {
                    self.inbox_add(&params.content, params.title.as_deref())
                }),
            "create_daily_note" => self
                .handle_tool_call::<CreateDailyNoteParams, _>(request.arguments, |params| {
                    self.create_daily_note(params.date.as_deref())
                }),
            "create_from_template" => self
                .handle_tool_call::<CreateFromTemplateParams, _>(request.arguments, |params| {
                    self.create_from_template(&params.template_name, params.prompts)
                }),
            other => {
                return Err(McpError::new(
                    ErrorCode::METHOD_NOT_FOUND,
                    format!("unknown tool: {other}"),
                    None,
                ));
            }
        };
        Ok(result)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(
            self.registered_resources(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match self.read_resource_value(&request.uri) {
            Ok(content) => Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(content, request.uri)],
            }),
            Err(_) => Err(McpError::resource_not_found(
                format!("unknown resource: {}", request.uri),
                None,
            )),
        }
    }
}

pub async fn run_stdio(mcp: NotesmithMcp) -> anyhow::Result<()> {
    let service = mcp.serve((tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}

fn parse_arguments<T>(arguments: Option<Map<String, Value>>) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(Into::into)
}

fn tool_definition(name: &'static str, description: &'static str, schema: Value) -> Tool {
    Tool::new(name, description, schema_object(schema))
}

fn resource_definition(uri: &str, name: &str, description: Option<&str>) -> Resource {
    let mut resource = RawResource::new(uri.to_string(), name.to_string());
    resource.description = description.map(ToOwned::to_owned);
    resource.mime_type = Some("text/plain".to_string());
    Annotated::new(resource, None)
}

fn schema_object(schema: Value) -> Map<String, Value> {
    schema
        .as_object()
        .cloned()
        .expect("schema must be an object")
}

fn build_note_document(
    frontmatter: Option<&Map<String, Value>>,
    body: Option<&str>,
) -> anyhow::Result<String> {
    let frontmatter = match frontmatter {
        Some(frontmatter) => json_frontmatter_to_mapping(frontmatter)?,
        None => Mapping::new(),
    };
    build_note_document_from_yaml(&frontmatter, body.unwrap_or_default())
}

fn build_note_document_from_yaml(frontmatter: &Mapping, body: &str) -> anyhow::Result<String> {
    let yaml = serialize_yaml_mapping(frontmatter)?;
    Ok(if yaml.is_empty() {
        format!("---\n---\n{body}")
    } else {
        format!("---\n{yaml}\n---\n{body}")
    })
}

fn serialize_yaml_mapping(frontmatter: &Mapping) -> anyhow::Result<String> {
    let serialized = serde_yaml::to_string(&YamlValue::Mapping(frontmatter.clone()))?;
    Ok(serialized
        .strip_prefix("---\n")
        .unwrap_or(&serialized)
        .trim_end_matches('\n')
        .to_string())
}

fn json_frontmatter_to_mapping(frontmatter: &Map<String, Value>) -> anyhow::Result<Mapping> {
    if frontmatter.is_empty() {
        return Ok(Mapping::new());
    }

    let yaml_value = serde_yaml::to_value(Value::Object(frontmatter.clone()))?;
    match yaml_value {
        YamlValue::Mapping(mapping) => Ok(mapping),
        other => anyhow::bail!("expected frontmatter object, got {other:?}"),
    }
}

fn parse_status_str(s: &str) -> Result<char, String> {
    match s {
        "todo" => Ok(' '),
        "in_progress" => Ok('/'),
        "blocked" => Ok('b'),
        "waiting" => Ok('w'),
        "on_hold" => Ok('h'),
        "done" => Ok('x'),
        "cancelled" => Ok('-'),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Ok(ch),
                _ => Err(format!(
                    "unknown status '{other}'; expected one of: todo, in_progress, blocked, waiting, on_hold, done, cancelled"
                )),
            }
        }
    }
}

fn load_note_frontmatter(conn: &Connection, vault_name: &str, path: &str) -> anyhow::Result<Value> {
    let mut fields_stmt = conn.prepare(
        "SELECT key, value, value_type FROM fields WHERE vault_name = ?1 AND note_path = ?2 ORDER BY key",
    )?;
    let mut field_rows = fields_stmt.query(params![vault_name, path])?;
    let mut frontmatter = Map::new();
    while let Some(row) = field_rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        let value_type: String = row.get(2)?;
        frontmatter.insert(key, parse_field_json_value(&value, &value_type));
    }
    drop(field_rows);
    drop(fields_stmt);

    let mut tags_stmt =
        conn.prepare("SELECT tag FROM tags WHERE vault_name = ?1 AND note_path = ?2 ORDER BY tag")?;
    let mut tag_rows = tags_stmt.query(params![vault_name, path])?;
    let mut tags = Vec::new();
    while let Some(row) = tag_rows.next()? {
        tags.push(row.get::<_, String>(0)?);
    }
    if !tags.is_empty() {
        frontmatter.insert(
            "tags".to_string(),
            Value::Array(tags.into_iter().map(Value::String).collect()),
        );
    }

    Ok(Value::Object(frontmatter))
}

fn parse_field_json_value(value: &str, value_type: &str) -> Value {
    match value_type {
        "boolean" => Value::Bool(value == "true"),
        "number" => value
            .parse::<i64>()
            .map(|number| Value::Number(number.into()))
            .or_else(|_| {
                value
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or(())
            })
            .unwrap_or_else(|_| Value::String(value.to_string())),
        "list" => serde_yaml::from_str::<Value>(value)
            .unwrap_or_else(|_| Value::String(value.to_string())),
        _ => Value::String(value.to_string()),
    }
}

fn frontmatter_string(frontmatter: &Value, key: &str) -> Option<String> {
    frontmatter.get(key).and_then(|value| match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn frontmatter_bool(frontmatter: &Value, key: &str) -> bool {
    match frontmatter.get(key) {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => text == "true",
        _ => false,
    }
}

fn status_name_for_char(status_char: &str) -> String {
    match status_char.chars().next().unwrap_or(' ') {
        ' ' => "todo",
        '/' => "in_progress",
        'b' => "blocked",
        'w' => "waiting",
        'h' => "on_hold",
        'x' | 'X' => "done",
        '-' => "cancelled",
        other => return other.to_string(),
    }
    .to_string()
}

fn sanitize_slug(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == ' ' || ch == '-' {
                ch
            } else {
                ' '
            }
        })
        .collect();
    sanitized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape_sql_string(input: &str) -> String {
    input.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn vault_config() -> VaultConfig {
        VaultConfig {
            name: "test-vault".to_string(),
            capture: notesmith_config::CaptureConfig {
                folder: "Inbox".to_string(),
                template: "generic-note".to_string(),
            },
            ..Default::default()
        }
    }

    fn build_test_mcp(root: &Path) -> NotesmithMcp {
        let engine = NativeVaultEngine;
        let notes = engine.scan(root).unwrap();
        let cache = VaultCache::open_in_memory().unwrap();
        cache.reindex("test-vault", &notes).unwrap();
        let search_index = SearchIndex::open_in_memory().unwrap();
        search_index.reindex("test-vault", &notes).unwrap();
        NotesmithMcp::new(
            "test-vault".to_string(),
            root.to_path_buf(),
            cache,
            search_index,
            vault_config(),
        )
    }

    fn write_note(root: &Path, path: &str, content: &str) {
        let engine = NativeVaultEngine;
        let note_path = VaultPath::new(path.to_string());
        let content = apply_save_pipeline(content);
        engine.write(root, &note_path, None, &content).unwrap();
    }

    #[test]
    fn test_create_and_get_note() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let created = mcp
            .create_note("Hello", Some("# Hello"), Some("Inbox"), None)
            .unwrap();
        assert_eq!(created["path"], "Inbox/Hello.md");

        let fetched = mcp.get_note("Inbox/Hello.md").unwrap();
        assert_eq!(fetched["path"], "Inbox/Hello.md");
        assert!(fetched["content"].as_str().unwrap().contains("# Hello"));
        assert!(fetched["content"].as_str().unwrap().contains("created:"));
    }

    #[test]
    fn test_search_notes() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Inbox/Launch Plan.md",
            "---\ntype: note\n---\nDiscuss launch timeline",
        );
        write_note(
            temp_dir.path(),
            "Inbox/Other.md",
            "---\ntype: note\n---\nUnrelated",
        );
        let mcp = build_test_mcp(temp_dir.path());

        let results = mcp.search_notes("launch", Some(10)).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Inbox/Launch Plan.md");
    }

    #[test]
    fn test_query_sql() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Inbox/Query Me.md",
            "---\ntype: note\n---\nBody",
        );
        let mcp = build_test_mcp(temp_dir.path());

        let result = mcp
            .query_sql("SELECT path, title FROM v_notes ORDER BY path")
            .unwrap();
        assert_eq!(result["columns"], json!(["path", "title"]));
        assert_eq!(result["row_count"], 1);
    }

    #[test]
    fn test_list_notes_with_filter() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Customers/Acme.md",
            "---\ntype: customer\ncustomer: Acme\nstate: Active\n---\n# Acme",
        );
        write_note(
            temp_dir.path(),
            "Inbox/Scratch.md",
            "---\ntype: note\n---\n# Scratch",
        );
        let mcp = build_test_mcp(temp_dir.path());

        let results = mcp.list_notes(Some("customer"), None, None).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Customers/Acme.md");
    }

    #[test]
    fn test_inbox_add() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let created = mcp
            .inbox_add("Captured thought", Some("Quick Note"))
            .unwrap();
        let path = created["path"].as_str().unwrap();
        assert!(path.starts_with("Inbox/"));
        assert!(path.ends_with("Quick Note.md"));
        let stored = std::fs::read_to_string(temp_dir.path().join(path)).unwrap();
        assert_eq!(stored, "Captured thought");
    }

    #[test]
    fn test_list_tools_returns_all_tools() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let tools = mcp.registered_tools();
        assert_eq!(tools.len(), 13);
    }

    #[test]
    fn test_list_resources_returns_all_resources() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let resources = mcp.registered_resources();
        assert_eq!(resources.len(), 3);
        assert_eq!(resources[0].uri, "note:///{vault-path}");
        assert_eq!(resources[1].uri, "note:///daily/{date}");
        assert_eq!(resources[2].uri, "note:///vault/structure");
    }
}
