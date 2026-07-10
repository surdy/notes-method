//! notesmith-mcp: MCP adapter exposing the shared [`notesmith_ops`] vault
//! operations to MCP clients over stdio.
//!
//! All operation logic lives in [`notesmith_ops::LocalOps`]; this crate only
//! maps MCP tool/resource requests onto that surface.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use notesmith_config::VaultConfig;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_ops::{LocalOps, Ops};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, model::*, service::RequestContext};
use serde::Deserialize;
use serde_json::{Map, Value, json};

mod bridge;
pub use bridge::{run_bridge, run_stdio_bridge};

pub struct NotesmithMcp {
    ops: Arc<dyn Ops>,
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
struct MemoryRecallParams {
    query: String,
    scope: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MemoryListParams {
    scope: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MemorySaveParams {
    title: String,
    claim: String,
    description: Option<String>,
    scope: String,
    subject: Option<String>,
    certainty: String,
    source: Option<String>,
    confirmed: Option<String>,
    supersedes: Option<String>,
    tags: Option<Vec<String>>,
    acknowledge_inference: Option<bool>,
    confirm_apply: Option<bool>,
    preview_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryUpdateParams {
    path: String,
    expected_hash: String,
    title: Option<String>,
    claim: Option<String>,
    description: Option<String>,
    body: Option<String>,
    scope: Option<String>,
    subject: Option<String>,
    certainty: Option<String>,
    source: Option<String>,
    status: Option<String>,
    confirmed: Option<String>,
    tags: Option<Vec<String>>,
    acknowledge_inference: Option<bool>,
    confirm_apply: Option<bool>,
    preview_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemorySupersedeParams {
    path: String,
    expected_hash: String,
    new_title: String,
    new_claim: String,
    description: Option<String>,
    scope: String,
    subject: Option<String>,
    certainty: String,
    source: Option<String>,
    confirmed: Option<String>,
    tags: Option<Vec<String>>,
    acknowledge_inference: Option<bool>,
    confirm_apply: Option<bool>,
    preview_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryDeleteParams {
    path: String,
    expected_hash: String,
    confirm_delete: bool,
}

#[derive(Debug, Deserialize)]
struct QuerySqlParams {
    sql: String,
}

#[derive(Debug, Deserialize)]
struct TimeQueryParams {
    when: String,
    date_field: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
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
            ops: Arc::new(LocalOps::new(
                vault_name,
                vault_root,
                cache,
                search_index,
                vault_config,
            )),
        }
    }

    /// Construct from an existing [`Ops`] surface (e.g. backed by the daemon's
    /// live per-vault state, or wrapped in [`notesmith_ops::ReadOnlyOps`]).
    pub fn from_ops(ops: Arc<dyn Ops>) -> Self {
        Self { ops }
    }

    /// Borrow the underlying operation surface.
    pub fn ops(&self) -> &dyn Ops {
        self.ops.as_ref()
    }

    fn registered_tools(&self) -> Vec<Tool> {
        // Ground every tool in the active vault so an agent that also has other
        // MCP servers available (e.g. a different vault) can tell these tools
        // apply to *this* vault and prefer them (issue #259).
        let vault = self.ops.vault_name();
        let scoped = |base: &str| -> String {
            format!("{base} (operates on the `{vault}` Notesmith vault)")
        };
        vec![
            tool_definition(
                "create_note",
                scoped("Create a new note in the vault"),
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
                scoped("Read a note by vault-relative path"),
                json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "update_note",
                scoped("Replace a note's content"),
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
                scoped("Append content to an existing note"),
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
                scoped("Apply routing rules and archive a note"),
                json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "search_notes",
                scoped("Search notes by title and body content"),
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
                "vault_search",
                scoped(
                    "Hybrid search (lexical full-text + semantic embedding \
                     ranking via reciprocal rank fusion) over this vault's \
                     notes. Returns note references with a path and snippet for \
                     grounding/citation. Prefer this for open-ended questions \
                     about the vault's content; it degrades to lexical-only \
                     until embeddings are available",
                ),
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
                "memory_recall",
                scoped(
                    "Recall active fact-memory notes using the existing hybrid \
                     lexical + semantic retrieval stack, filtered to durable \
                     non-example `type: fact` notes. When `scope` is provided, \
                     it includes `scope: user` plus facts whose scope exactly \
                     matches the supplied value",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "scope": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1}
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "memory_list",
                scoped(
                    "List non-example fact-memory notes with stable structured \
                     fields. Defaults to active facts; when `scope` is \
                     supplied it includes `scope: user` plus the exact scope",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "scope": {"type": "string"},
                        "status": {
                            "type": "string",
                            "enum": ["active", "superseded", "retracted"]
                        },
                        "limit": {"type": "integer", "minimum": 1}
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "memory_save",
                scoped(
                    "Preview or apply creation of a new `facts/...` fact note. \
                     By default this is a no-write preview that returns the \
                     exact proposed path/content plus similar active fact \
                     candidates. Applying requires `confirm_apply: true`, a \
                     fresh `preview_token`, and valid provenance (`observed` \
                     needs `source`; `inferred` needs \
                     `acknowledge_inference: true`)",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "claim": {"type": "string"},
                        "description": {"type": "string"},
                        "scope": {"type": "string"},
                        "subject": {"type": "string"},
                        "certainty": {
                            "type": "string",
                            "enum": ["explicit", "observed", "inferred"]
                        },
                        "source": {"type": "string"},
                        "confirmed": {"type": "string"},
                        "supersedes": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "acknowledge_inference": {"type": "boolean"},
                        "confirm_apply": {"type": "boolean"},
                        "preview_token": {"type": "string"}
                    },
                    "required": ["title", "claim", "scope", "certainty"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "memory_update",
                scoped(
                    "Preview or apply an optimistic update to an existing \
                     `type: fact` note under `facts/`. Requires a fresh \
                     `expected_hash`; claim-changing updates preview similar \
                     active facts before writes. Applying requires \
                     `confirm_apply: true` plus the `preview_token` returned \
                     by the preview",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "expected_hash": {"type": "string"},
                        "title": {"type": "string"},
                        "claim": {"type": "string"},
                        "description": {"type": "string"},
                        "body": {"type": "string"},
                        "scope": {"type": "string"},
                        "subject": {"type": "string"},
                        "certainty": {
                            "type": "string",
                            "enum": ["explicit", "observed", "inferred"]
                        },
                        "source": {"type": "string"},
                        "status": {
                            "type": "string",
                            "enum": ["active", "superseded", "retracted"]
                        },
                        "confirmed": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "acknowledge_inference": {"type": "boolean"},
                        "confirm_apply": {"type": "boolean"},
                        "preview_token": {"type": "string"}
                    },
                    "required": ["path", "expected_hash"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "memory_supersede",
                scoped(
                    "Preview or apply replacement of an active fact with a new \
                     fact note. The preview returns the proposed replacement \
                     path/content and similar active-fact candidates; applying \
                     requires `confirm_apply: true`, a fresh `preview_token`, \
                     and the current fact `expected_hash`",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "expected_hash": {"type": "string"},
                        "new_title": {"type": "string"},
                        "new_claim": {"type": "string"},
                        "description": {"type": "string"},
                        "scope": {"type": "string"},
                        "subject": {"type": "string"},
                        "certainty": {
                            "type": "string",
                            "enum": ["explicit", "observed", "inferred"]
                        },
                        "source": {"type": "string"},
                        "confirmed": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "acknowledge_inference": {"type": "boolean"},
                        "confirm_apply": {"type": "boolean"},
                        "preview_token": {"type": "string"}
                    },
                    "required": ["path", "expected_hash", "new_title", "new_claim", "scope", "certainty"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "memory_delete",
                scoped(
                    "Hard-delete a fact note for mistakes or sensitive material. \
                     Requires `confirm_delete: true` and a fresh \
                     `expected_hash`; example facts are rejected",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "expected_hash": {"type": "string"},
                        "confirm_delete": {"type": "boolean"}
                    },
                    "required": ["path", "expected_hash", "confirm_delete"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "query_sql",
                scoped("Execute read-only SQL against the vault cache"),
                json!({
                    "type": "object",
                    "properties": {"sql": {"type": "string"}},
                    "required": ["sql"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "time_query",
                scoped(
                    "Resolve a natural-language time expression (e.g. 'last \
                     week', 'in May', 'yesterday', 'last 3 days', 'May 2021') \
                     into a date range and return note references dated within \
                     it. Pairs with vault_search so the agent can cite real, \
                     dated notes. Use `date_field` to choose which date to \
                     filter on (mtime [default], updated, created); pass an \
                     optional `query` to also keyword-filter the results",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "when": {
                            "type": "string",
                            "description": "Natural-language time expression, e.g. 'last week'"
                        },
                        "date_field": {
                            "type": "string",
                            "enum": ["mtime", "updated", "created"],
                            "description": "Which note date to filter on (default: mtime)"
                        },
                        "query": {
                            "type": "string",
                            "description": "Optional keyword to further filter matches"
                        },
                        "limit": {"type": "integer", "minimum": 1}
                    },
                    "required": ["when"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "list_notes",
                scoped("List notes with optional type, customer, and archive filters"),
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
                scoped("List tasks with optional status and customer filters"),
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
                scoped("Set the status of a task in a note"),
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
                scoped("Quick-capture content into the inbox folder"),
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
                scoped("Ensure a daily note exists for a date"),
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
                scoped("Instantiate a note from a configured template"),
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
            Ok(value) => CallToolResult::structured(ensure_structured_object(value)),
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
            "vault_search" => self
                .handle_tool_call::<SearchNotesParams, _>(request.arguments, |params| {
                    self.ops.vault_search(&params.query, params.limit)
                }),
            "memory_recall" => {
                self.handle_tool_call::<MemoryRecallParams, _>(request.arguments, |params| {
                    self.ops
                        .memory_recall(&params.query, params.scope.as_deref(), params.limit)
                })
            }
            "memory_list" => {
                self.handle_tool_call::<MemoryListParams, _>(request.arguments, |params| {
                    self.ops.memory_list(
                        params.scope.as_deref(),
                        params.status.as_deref(),
                        params.limit,
                    )
                })
            }
            "memory_save" => {
                self.handle_tool_call::<MemorySaveParams, _>(request.arguments, |params| {
                    self.ops.memory_save(
                        &params.title,
                        &params.claim,
                        params.description.as_deref(),
                        &params.scope,
                        params.subject.as_deref(),
                        &params.certainty,
                        params.source.as_deref(),
                        params.confirmed.as_deref(),
                        params.supersedes.as_deref(),
                        params.tags,
                        params.acknowledge_inference.unwrap_or(false),
                        params.confirm_apply.unwrap_or(false),
                        params.preview_token.as_deref(),
                    )
                })
            }
            "memory_update" => {
                self.handle_tool_call::<MemoryUpdateParams, _>(request.arguments, |params| {
                    self.ops.memory_update(
                        &params.path,
                        &params.expected_hash,
                        params.title.as_deref(),
                        params.claim.as_deref(),
                        params.description.as_deref(),
                        params.body.as_deref(),
                        params.scope.as_deref(),
                        params.subject.as_deref(),
                        params.certainty.as_deref(),
                        params.source.as_deref(),
                        params.status.as_deref(),
                        params.confirmed.as_deref(),
                        params.tags,
                        params.confirm_apply.unwrap_or(false),
                        params.preview_token.as_deref(),
                        params.acknowledge_inference.unwrap_or(false),
                    )
                })
            }
            "memory_supersede" => {
                self.handle_tool_call::<MemorySupersedeParams, _>(request.arguments, |params| {
                    self.ops.memory_supersede(
                        &params.path,
                        &params.expected_hash,
                        &params.new_title,
                        &params.new_claim,
                        params.description.as_deref(),
                        &params.scope,
                        params.subject.as_deref(),
                        &params.certainty,
                        params.source.as_deref(),
                        params.confirmed.as_deref(),
                        params.tags,
                        params.acknowledge_inference.unwrap_or(false),
                        params.confirm_apply.unwrap_or(false),
                        params.preview_token.as_deref(),
                    )
                })
            }
            "memory_delete" => {
                self.handle_tool_call::<MemoryDeleteParams, _>(request.arguments, |params| {
                    self.ops.memory_delete(
                        &params.path,
                        &params.expected_hash,
                        params.confirm_delete,
                    )
                })
            }
            "query_sql" => self
                .handle_tool_call::<QuerySqlParams, _>(request.arguments, |params| {
                    self.ops.query_sql(&params.sql)
                }),
            "time_query" => {
                self.handle_tool_call::<TimeQueryParams, _>(request.arguments, |params| {
                    self.ops.time_query(
                        &params.when,
                        params.date_field.as_deref(),
                        params.query.as_deref(),
                        params.limit,
                    )
                })
            }
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

/// The concrete streamable-HTTP MCP service type produced by
/// [`streamable_http_service`].
///
/// Exposed so callers (e.g. the daemon's HTTP server) can cache and reuse a
/// per-vault service instance. The service is cheap to [`Clone`] — clones share
/// the same session manager — but must be reused rather than rebuilt per request
/// so that MCP session state persists across requests.
pub type NotesmithHttpService = StreamableHttpService<NotesmithMcp, LocalSessionManager>;

/// Build a streamable-HTTP MCP service backed by the given [`Ops`] surface.
///
/// The returned service is an axum-compatible tower service. A fresh
/// [`NotesmithMcp`] handler is created per MCP session, all sharing the same
/// `ops` (and therefore the same live vault indexes when `ops` is backed by
/// [`LocalOps::from_shared`]).
pub fn streamable_http_service(ops: Arc<dyn Ops>) -> NotesmithHttpService {
    StreamableHttpService::new(
        move || Ok(NotesmithMcp::from_ops(ops.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}

fn parse_arguments<T>(arguments: Option<Map<String, Value>>) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(Into::into)
}

/// Coerce a tool result into a JSON object for `structuredContent`.
///
/// The MCP spec defines a tool result's `structuredContent` as "an optional
/// JSON *object*". Several ops legitimately return a bare array (search
/// results, note/task lists, SQL rows) or a scalar. Lenient clients accept
/// these, but strict clients (e.g. Copilot) reject a non-object and surface it
/// as a tool error. Wrap arrays under `results` and any other non-object under
/// `result` so the structured payload is always a valid object; the raw value
/// is still echoed in the result's text content.
fn ensure_structured_object(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        Value::Array(_) => json!({ "results": value }),
        other => json!({ "result": other }),
    }
}

fn tool_definition(name: &'static str, description: impl Into<String>, schema: Value) -> Tool {
    Tool::new(name, description.into(), schema_object(schema))
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
    fn test_vault_search_lexical_fallback() {
        // With no embeddings.db present, vault_search degrades to lexical-only
        // but still returns hybrid-shaped hits (path + snippet + ranks).
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

        let results = mcp.ops().vault_search("launch", Some(10)).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Inbox/Launch Plan.md");
        assert!(results[0].get("snippet").is_some());
        assert_eq!(results[0]["lexical_rank"], 1);
        assert!(results[0]["semantic_rank"].is_null());
    }

    #[test]
    fn test_memory_recall_lexical_fallback() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "facts/Coffee.md",
            "---\n\
             type: fact\n\
             description: Prefer coffee before launch reviews.\n\
             scope: user\n\
             certainty: explicit\n\
             source: User statement\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Prefer coffee before launch reviews.\n",
        );
        let mcp = build_test_mcp(temp_dir.path());

        let result = mcp.ops().memory_recall("launch", None, Some(10)).unwrap();
        assert_eq!(result["match_count"], 1);
        assert_eq!(result["embeddings_used"], false);
        let fact = &result["facts"].as_array().unwrap()[0];
        assert_eq!(fact["path"], "facts/Coffee.md");
        assert_eq!(fact["claim"], "Prefer coffee before launch reviews.");
    }

    #[test]
    fn ensure_structured_object_wraps_non_objects() {
        // MCP requires structuredContent to be a JSON object; arrays/scalars
        // are wrapped so strict clients (e.g. Copilot) don't reject the result.
        let array = ensure_structured_object(json!([{ "path": "a.md" }]));
        assert!(array.is_object());
        assert_eq!(array["results"], json!([{ "path": "a.md" }]));

        let scalar = ensure_structured_object(json!(42));
        assert!(scalar.is_object());
        assert_eq!(scalar["result"], json!(42));

        let object = ensure_structured_object(json!({ "path": "a.md" }));
        assert_eq!(object, json!({ "path": "a.md" }));
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
    fn test_time_query() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Journal/Old.md",
            "---\ntype: note\ncreated: 2019-03-02\n---\nArchived thought",
        );
        write_note(
            temp_dir.path(),
            "Journal/May.md",
            "---\ntype: note\ncreated: 2020-05-05\n---\nSpring thought",
        );
        let mcp = build_test_mcp(temp_dir.path());

        let result = mcp
            .ops()
            .time_query("May 2020", Some("created"), None, None)
            .unwrap();
        assert_eq!(result["match_count"], 1);
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["path"], "Journal/May.md");
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
        assert_eq!(tools.len(), 21);
        assert!(tools.iter().any(|t| t.name == "memory_recall"));
        assert!(tools.iter().any(|t| t.name == "memory_list"));
        assert!(tools.iter().any(|t| t.name == "memory_save"));
        assert!(tools.iter().any(|t| t.name == "memory_update"));
        assert!(tools.iter().any(|t| t.name == "memory_supersede"));
        assert!(tools.iter().any(|t| t.name == "memory_delete"));
        assert!(tools.iter().any(|t| t.name == "vault_search"));
        assert!(tools.iter().any(|t| t.name == "time_query"));
    }

    #[test]
    fn tool_descriptions_name_the_active_vault() {
        // Every tool description must ground the agent in the active vault so a
        // session that also has other MCP servers can prefer these tools for
        // this vault (issue #259).
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        for tool in mcp.registered_tools() {
            let desc = tool
                .description
                .as_ref()
                .map(|d| d.as_ref())
                .unwrap_or_default();
            assert!(
                desc.contains("test-vault"),
                "tool `{}` description does not name the vault: {desc:?}",
                tool.name
            );
        }
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
