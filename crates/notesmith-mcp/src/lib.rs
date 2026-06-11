//! notesmith-mcp: MCP adapter exposing the shared [`notesmith_ops`] vault
//! operations to MCP clients over stdio.
//!
//! All operation logic lives in [`notesmith_ops::LocalOps`]; this crate only
//! maps MCP tool/resource requests onto that surface.

use std::collections::HashMap;
use std::path::PathBuf;

use notesmith_config::VaultConfig;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_ops::{LocalOps, Ops};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

pub struct NotesmithMcp {
    ops: LocalOps,
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
        Self {
            ops: LocalOps::new(vault_name, vault_root, cache, search_index, vault_config),
        }
    }

    /// Construct from an existing [`LocalOps`] (e.g. backed by the daemon's
    /// live per-vault state).
    pub fn from_ops(ops: LocalOps) -> Self {
        Self { ops }
    }

    /// Borrow the underlying operation surface.
    pub fn ops(&self) -> &LocalOps {
        &self.ops
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
                    self.ops.create_note(
                        &params.title,
                        params.content.as_deref(),
                        params.folder.as_deref(),
                        params.frontmatter.as_ref(),
                    )
                })
            }
            "get_note" => self.handle_tool_call::<GetNoteParams, _>(request.arguments, |params| {
                self.ops.get_note(&params.path)
            }),
            "update_note" => self
                .handle_tool_call::<UpdateNoteParams, _>(request.arguments, |params| {
                    self.ops.update_note(&params.path, &params.content)
                }),
            "append_to_note" => self
                .handle_tool_call::<AppendToNoteParams, _>(request.arguments, |params| {
                    self.ops.append_to_note(&params.path, &params.content)
                }),
            "archive_note" => self
                .handle_tool_call::<ArchiveNoteParams, _>(request.arguments, |params| {
                    self.ops.archive_note(&params.path)
                }),
            "search_notes" => self
                .handle_tool_call::<SearchNotesParams, _>(request.arguments, |params| {
                    self.ops.search_notes(&params.query, params.limit)
                }),
            "query_sql" => self
                .handle_tool_call::<QuerySqlParams, _>(request.arguments, |params| {
                    self.ops.query_sql(&params.sql)
                }),
            "list_notes" => {
                self.handle_tool_call::<ListNotesParams, _>(request.arguments, |params| {
                    self.ops.list_notes(
                        params.note_type.as_deref(),
                        params.customer.as_deref(),
                        params.archived,
                    )
                })
            }
            "list_tasks" => {
                self.handle_tool_call::<ListTasksParams, _>(request.arguments, |params| {
                    self.ops
                        .list_tasks(params.status.as_deref(), params.customer.as_deref())
                })
            }
            "update_task_status" => {
                self.handle_tool_call::<UpdateTaskStatusParams, _>(request.arguments, |params| {
                    self.ops.update_task_status(
                        &params.note_path,
                        &params.task_hash,
                        &params.status,
                    )
                })
            }
            "inbox_add" => self
                .handle_tool_call::<InboxAddParams, _>(request.arguments, |params| {
                    self.ops.inbox_add(&params.content, params.title.as_deref())
                }),
            "create_daily_note" => self
                .handle_tool_call::<CreateDailyNoteParams, _>(request.arguments, |params| {
                    self.ops.create_daily_note(params.date.as_deref())
                }),
            "create_from_template" => {
                self.handle_tool_call::<CreateFromTemplateParams, _>(request.arguments, |params| {
                    self.ops
                        .create_from_template(&params.template_name, params.prompts)
                })
            }
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
        match self.ops.read_resource(&request.uri) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_core::{VaultEngine, VaultPath};
    use notesmith_vault::{NativeVaultEngine, apply_save_pipeline};
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
        cache
            .reindex_with_periodic("test-vault", &notes, &vault_config().periodic)
            .unwrap();
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
            .ops()
            .create_note("Hello", Some("# Hello"), Some("Inbox"), None)
            .unwrap();
        assert_eq!(created["path"], "Inbox/Hello.md");

        let fetched = mcp.ops().get_note("Inbox/Hello.md").unwrap();
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

        let results = mcp.ops().search_notes("launch", Some(10)).unwrap();
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
            .ops()
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

        let results = mcp.ops().list_notes(Some("customer"), None, None).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Customers/Acme.md");
    }

    #[test]
    fn test_inbox_add() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let created = mcp
            .ops()
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
