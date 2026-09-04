//! notesmith-mcp: MCP adapter exposing the shared [`notesmith_ops`] vault
//! operations to MCP clients over stdio.
//!
//! All operation logic lives in [`notesmith_ops::LocalOps`]; this crate only
//! maps MCP tool/resource requests onto that surface.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

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

/// HTTP request header carrying the job-run id an agent-job session tags its
/// vault writes with (job success criteria, ADR 0025 amendment 2026-09-04).
/// The job runner mints the id, the CLI stamps it on the daemon HTTP vault
/// binding, and the daemon attributes every write tool call under it to that
/// run so a run that wrote nothing can be recorded as `no_writes` rather than a
/// false `succeeded`. Header lookup is case-insensitive, so the CLI's
/// `X-Notesmith-Run-Id` matches this lower-case form.
pub const RUN_ID_HEADER: &str = "x-notesmith-run-id";

/// What a run wrote to the vault, attributed by run id.
///
/// `count` is the total number of successful write-tool calls; `sections` are
/// the distinct managed-section ids touched by `update_managed_section` calls
/// (sorted and deduped). A run that wrote only non-section tools (e.g.
/// `update_note`) has `count >= 1` and an empty `sections` list. Diagnostic
/// metadata — the briefing's verdict still keys off `count` alone (job success
/// criteria, ADR 0025 amendment 2026-09-04); per-section data is captured for
/// surfacing and a future partial-write option.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunWrites {
    pub count: u32,
    /// Managed-section ids touched, sorted and deduped.
    pub sections: Vec<String>,
}

/// Internal accumulator: a total count plus the set of section ids touched.
#[derive(Debug, Default)]
struct RunWriteTally {
    count: u32,
    sections: BTreeSet<String>,
}

/// Process-global per-run write tally, keyed by run id.
///
/// The daemon is a single process: the job runner and the HTTP MCP write
/// dispatch share it, and run ids are globally unique (UUIDs), so a process
/// global is the whole surface — no persistence, no per-vault scoping. Entries
/// are created on the first attributed write and removed by the runner
/// ([`take_run_writes`]) once the run has exited, so the map stays small.
fn run_write_counter() -> &'static Mutex<HashMap<String, RunWriteTally>> {
    static COUNTER: OnceLock<Mutex<HashMap<String, RunWriteTally>>> = OnceLock::new();
    COUNTER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record one successful vault write attributed to `run_id`. When the write was
/// an `update_managed_section` call, `section_id` names the section it touched,
/// which is recorded alongside the count for per-section attribution.
pub fn record_run_write(run_id: &str, section_id: Option<&str>) {
    let mut map = run_write_counter()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let tally = map.entry(run_id.to_string()).or_default();
    tally.count += 1;
    if let Some(section) = section_id {
        tally.sections.insert(section.to_string());
    }
}

/// Read and remove the write tally for `run_id`. `None` means no attributed
/// write was ever recorded for that run (the map entry is created lazily on the
/// first write). The runner treats `None` as zero writes. The returned
/// `sections` are sorted and deduped.
pub fn take_run_writes(run_id: &str) -> Option<RunWrites> {
    run_write_counter()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(run_id)
        .map(|tally| RunWrites {
            count: tally.count,
            sections: tally.sections.into_iter().collect(),
        })
}

/// Whether a tool call mutates the vault, for per-run write attribution.
///
/// The line is drawn at tools whose sole purpose is an unconditional vault
/// mutation, matching the ADR-amendment plan's enumerated set. Deliberately
/// EXCLUDED: the `memory_*` tools (preview unless `confirm_apply`/
/// `confirm_delete`, so a call is usually a no-write preview) and
/// `read_document` (writes only when `save: true`); counting either at the
/// dispatch level — without inspecting arguments — would over-count a run as
/// having written. Neither is used by the daily briefing this attribution
/// targets. Every listed tool goes through the write-gated `Ops` surface.
fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "create_note"
            | "update_note"
            | "update_managed_section"
            | "append_to_note"
            | "archive_note"
            | "update_task_status"
            | "inbox_add"
            | "create_daily_note"
            | "create_from_template"
    )
}

/// Extract the job-run id from the HTTP request parts rmcp injects into the
/// request extensions (streamable-HTTP transport only). `None` for the stdio
/// bridge or any request without the header.
fn run_id_from_context(context: &RequestContext<RoleServer>) -> Option<String> {
    run_id_from_extensions(&context.extensions)
}

/// The run-id header value carried by the `http::request::Parts` rmcp stores in
/// a request's extensions. Split out from [`run_id_from_context`] so it can be
/// unit-tested without constructing a full [`RequestContext`].
fn run_id_from_extensions(extensions: &Extensions) -> Option<String> {
    extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.headers.get(RUN_ID_HEADER))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

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
struct UpdateManagedSectionParams {
    path: String,
    section_id: String,
    content: String,
    #[serde(default = "default_append_if_missing")]
    append_if_missing: bool,
    expected_hash: Option<String>,
}

fn default_append_if_missing() -> bool {
    true
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
struct VaultSearchParams {
    query: String,
    limit: Option<usize>,
    filters: Option<notesmith_ops::SearchFilters>,
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
    fields: Option<HashMap<String, String>>,
    archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListTasksParams {
    status: Option<String>,
    fields: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct VaultStatsParams {
    top: Option<usize>,
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

#[derive(Debug, Deserialize)]
struct YoutubeTranscriptParams {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ReadDocumentParams {
    path: String,
    #[serde(default)]
    save: bool,
    folder: Option<String>,
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
                "update_managed_section",
                scoped(
                    "Refresh one managed section of a note in place. Replaces only the bytes \
                     between that section's `<!-- notesmith:section:begin <id> -->` and \
                     `<!-- notesmith:section:end <id> -->` marker lines; every other byte of the \
                     note — human prose, other managed sections, and the frontmatter (no `updated:` \
                     restamp) — is preserved exactly. Prefer this over `update_note` for any note \
                     that carries markers: it is idempotent and cannot disturb human content",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Vault-relative note path"},
                        "section_id": {
                            "type": "string",
                            "description": "Section id inside the markers, e.g. `briefing/meetings`"
                        },
                        "content": {
                            "type": "string",
                            "description": "New interior content, without the marker lines"
                        },
                        "append_if_missing": {
                            "type": "boolean",
                            "default": true,
                            "description": "When the marker pair is absent, append one complete marked block at the end of the note instead of erroring"
                        },
                        "expected_hash": {
                            "type": "string",
                            "description": "Optional hash of the note as last read; a mismatch is a write conflict rather than a silent overwrite"
                        }
                    },
                    "required": ["path", "section_id", "content"],
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
                     until embeddings are available. Optional `filters` scope \
                     the search by metadata: exact field values (list fields \
                     match by membership; an array value is OR within its \
                     key), tags, and a path prefix — all AND together. E.g. \
                     {\"fields\": {\"customers\": \"[[Acme]]\"}} restricts to \
                     notes involving Acme",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1},
                        "filters": {
                            "type": "object",
                            "properties": {
                                "fields": {
                                    "type": "object",
                                    "additionalProperties": {
                                        "oneOf": [
                                            {"type": "string"},
                                            {"type": "array", "items": {"type": "string"}}
                                        ]
                                    },
                                    "description": "Field key -> exact value or any-of array; list fields match by membership"
                                },
                                "tags": {"type": "array", "items": {"type": "string"}},
                                "path_prefix": {"type": "string"}
                            },
                            "additionalProperties": false
                        }
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
                     fresh process-bound `preview_token` from the current \
                     daemon session, and valid provenance (`observed` needs \
                     `source`; `inferred` needs \
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
                     `confirm_apply: true` plus the fresh process-bound \
                     `preview_token` returned by the preview; regenerate it \
                     after daemon restart",
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
                     requires `confirm_apply: true`, a fresh process-bound \
                     `preview_token`, and the current fact `expected_hash`; \
                     regenerate the token after daemon restart",
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
                scoped(
                    "List notes with optional type/kind, exact field, and \
                     archive filters. `fields` maps field keys to exact \
                     values; a list-valued field (e.g. customers) matches \
                     when any member equals the value, so \
                     {\"customers\": \"[[Acme]]\"} finds every note \
                     involving Acme",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "type": {"type": "string"},
                        "fields": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Field key -> exact value; list fields match by membership; multiple keys AND together"
                        },
                        "archived": {"type": "boolean"}
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "list_tasks",
                scoped(
                    "List tasks with optional status and exact field filters. \
                     Tasks inherit their containing note's frontmatter \
                     (task-level inline fields override per key), so \
                     {\"customers\": \"[[Acme]]\"} finds tasks from Acme \
                     meetings too",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "fields": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Effective field key -> exact value; list fields match by membership; multiple keys AND together"
                        }
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "vault_stats",
                scoped(
                    "Summarise this vault's structure from the note index: \
                     totals (notes, distinct tags, resolved links, tasks, \
                     words, orphans), the most-used tags, the most-linked-to \
                     notes, and orphan notes (no resolved incoming or outgoing \
                     links). Use it to reason about the vault's shape for \
                     PKM/cleanup. `top` caps each ranked list (default 20). \
                     Embedding-independent",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "top": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Max rows per ranked list (default 20)"
                        }
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
            tool_definition(
                "youtube_transcript",
                scoped(
                    "Fetch the published caption transcript for a YouTube URL via the captions \
                     API; returns transcript text with timestamps. Videos without published \
                     captions return a clear non-fatal result (no audio is transcribed)",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"}
                    },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "read_document",
                scoped(
                    "Extract text from a PDF or EPUB file stored in the vault (by \
                     vault-relative path) into plain text plus fixed-size chunks and \
                     provenance metadata (title, author, page/chapter count). Pure local \
                     parsing, no OCR: scanned/image-only PDFs yield little or no text. Set \
                     `save: true` to also write a normalized note (into `folder`, default \
                     `attachments`)",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "save": {"type": "boolean"},
                        "folder": {"type": "string"}
                    },
                    "required": ["path"],
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

    /// Extract a vault document and, when `save` is set, persist the normalized
    /// note through the gated `create_note` path (so a read-only surface refuses
    /// the write). Returns the extraction result, augmented with `saved` /
    /// `saved_path` when a note was written.
    fn read_document_op(
        &self,
        path: &str,
        save: bool,
        folder: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut result = self.ops.read_document(path)?;
        if save {
            let title = result
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| document_title_from_path(path));
            let body = result
                .get("body")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let frontmatter = result
                .get("frontmatter")
                .and_then(|value| value.as_object())
                .cloned();
            let folder = folder.unwrap_or("attachments");
            let created =
                self.ops
                    .create_note(&title, Some(&body), Some(folder), frontmatter.as_ref())?;
            if let Value::Object(map) = &mut result {
                map.insert("saved".to_string(), Value::Bool(true));
                if let Some(path) = created.get("path") {
                    map.insert("saved_path".to_string(), path.clone());
                }
            }
        }
        Ok(result)
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
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let is_write = is_write_tool(request.name.as_ref());
        // Capture the managed-section id before `request.arguments` is moved into
        // the dispatch, so a successful `update_managed_section` write can be
        // attributed to the section it touched (per-run write attribution).
        let section_id = if request.name.as_ref() == "update_managed_section" {
            request
                .arguments
                .as_ref()
                .and_then(|args| args.get("section_id"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        } else {
            None
        };
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
            "update_managed_section" => self.handle_tool_call::<UpdateManagedSectionParams, _>(
                request.arguments,
                |params| {
                    self.ops.update_managed_section(
                        &params.path,
                        &params.section_id,
                        &params.content,
                        params.append_if_missing,
                        params.expected_hash.as_deref(),
                    )
                },
            ),
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
            "vault_search" => {
                self.handle_tool_call::<VaultSearchParams, _>(request.arguments, |params| {
                    self.ops
                        .vault_search(&params.query, params.limit, params.filters.as_ref())
                })
            }
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
                        params.fields.as_ref(),
                        params.archived,
                    )
                })
            }
            "list_tasks" => {
                self.handle_tool_call::<ListTasksParams, _>(request.arguments, |params| {
                    self.ops
                        .list_tasks(params.status.as_deref(), params.fields.as_ref())
                })
            }
            "vault_stats" => self
                .handle_tool_call::<VaultStatsParams, _>(request.arguments, |params| {
                    self.ops.vault_stats(params.top)
                }),
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
            "youtube_transcript" => {
                // `youtube_transcript` is async (bounded network fetch via the
                // shared clip module), so it cannot flow through the sync
                // `handle_tool_call` closure. Await the free async op directly.
                match parse_arguments::<YoutubeTranscriptParams>(request.arguments) {
                    Ok(params) => match notesmith_ops::youtube_transcript(&params.url).await {
                        Ok(value) => CallToolResult::structured(ensure_structured_object(value)),
                        Err(error) => CallToolResult::error(vec![Content::text(error.to_string())]),
                    },
                    Err(error) => CallToolResult::error(vec![Content::text(error.to_string())]),
                }
            }
            "read_document" => self
                .handle_tool_call::<ReadDocumentParams, _>(request.arguments, |params| {
                    self.read_document_op(&params.path, params.save, params.folder.as_deref())
                }),
            other => {
                return Err(McpError::new(
                    ErrorCode::METHOD_NOT_FOUND,
                    format!("unknown tool: {other}"),
                    None,
                ));
            }
        };
        // Attribute a successful write to the tagging run (if any). Counting
        // only non-error results means a rejected write (e.g. a read-only
        // surface, a write conflict) does not falsely mark the run as having
        // written — `no_writes` must reflect what actually landed in the vault.
        if is_write && result.is_error != Some(true) {
            if let Some(run_id) = run_id_from_context(&context) {
                record_run_write(&run_id, section_id.as_deref());
            }
        }
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

/// Derive a fallback note title from a document's vault-relative path when the
/// document has no embedded title (file stem, minus extension).
fn document_title_from_path(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.rsplit_once('.').map(|(name, _)| name).unwrap_or(file);
    if stem.is_empty() {
        "Document".to_string()
    } else {
        stem.to_string()
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

        let results = mcp.ops().vault_search("launch", Some(10), None).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Inbox/Launch Plan.md");
        assert!(results[0].get("snippet").is_some());
        assert_eq!(results[0]["lexical_rank"], 1);
        assert!(results[0]["semantic_rank"].is_null());
    }

    #[test]
    fn test_vault_search_filters_parse_from_tool_arguments() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Meetings/acme.md",
            "---\nkind: meeting\ncustomers:\n  - \"[[Acme]]\"\n---\nDiscuss launch timeline",
        );
        write_note(
            temp_dir.path(),
            "Meetings/globex.md",
            "---\nkind: meeting\ncustomers:\n  - \"[[Globex]]\"\n---\nAnother launch discussion",
        );
        let mcp = build_test_mcp(temp_dir.path());

        let params: VaultSearchParams = serde_json::from_value(serde_json::json!({
            "query": "launch",
            "filters": {"fields": {"customers": "[[Acme]]"}}
        }))
        .unwrap();
        let results = mcp
            .ops()
            .vault_search(&params.query, params.limit, params.filters.as_ref())
            .unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Meetings/acme.md");
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
    fn test_memory_tool_params_do_not_percent_decode_paths() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());
        let encoded = "facts%2F..%2Foutside.md";
        let params: MemoryDeleteParams = parse_arguments(
            json!({
                "path": encoded,
                "expected_hash": "hash",
                "confirm_delete": true,
            })
            .as_object()
            .cloned(),
        )
        .unwrap();

        assert_eq!(params.path, encoded);
        let err = mcp
            .ops()
            .memory_delete(&params.path, &params.expected_hash, params.confirm_delete)
            .unwrap_err();
        assert!(err.to_string().contains("facts/ note paths"));
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
        assert_eq!(tools.len(), 25);
        assert!(tools.iter().any(|t| t.name == "update_managed_section"));
        assert!(tools.iter().any(|t| t.name == "memory_recall"));
        assert!(tools.iter().any(|t| t.name == "memory_list"));
        assert!(tools.iter().any(|t| t.name == "memory_save"));
        assert!(tools.iter().any(|t| t.name == "memory_update"));
        assert!(tools.iter().any(|t| t.name == "memory_supersede"));
        assert!(tools.iter().any(|t| t.name == "memory_delete"));
        assert!(tools.iter().any(|t| t.name == "vault_search"));
        assert!(tools.iter().any(|t| t.name == "time_query"));
        assert!(tools.iter().any(|t| t.name == "vault_stats"));
        assert!(tools.iter().any(|t| t.name == "youtube_transcript"));
        assert!(tools.iter().any(|t| t.name == "read_document"));
    }

    #[test]
    fn update_managed_section_tool_has_expected_schema() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let tools = mcp.registered_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "update_managed_section")
            .expect("update_managed_section tool must be registered");

        let schema = tool.input_schema.as_ref();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["section_id"]["type"], "string");
        assert_eq!(schema["properties"]["content"]["type"], "string");
        assert_eq!(schema["properties"]["append_if_missing"]["type"], "boolean");
        assert_eq!(schema["properties"]["expected_hash"]["type"], "string");
        assert_eq!(schema["required"], json!(["path", "section_id", "content"]));
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn update_managed_section_tool_defaults_to_appending_a_missing_pair() {
        // `append_if_missing` is optional in the schema; omitting it must mean
        // "append", matching the guidance the vault prompts give agents.
        let params: UpdateManagedSectionParams = serde_json::from_value(json!({
            "path": "Daily/Today.md",
            "section_id": "briefing/meetings",
            "content": "- x",
        }))
        .unwrap();
        assert!(params.append_if_missing);
        assert!(params.expected_hash.is_none());
    }

    #[test]
    fn update_managed_section_is_rejected_on_a_read_only_surface() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Daily/Today.md",
            "<!-- notesmith:section:begin s -->\nold\n<!-- notesmith:section:end s -->",
        );
        let mcp = build_test_mcp(temp_dir.path());
        let read_only =
            NotesmithMcp::from_ops(Arc::new(notesmith_ops::ReadOnlyOps::new(LocalOps::new(
                "test-vault".to_string(),
                temp_dir.path().to_path_buf(),
                VaultCache::open_in_memory().unwrap(),
                SearchIndex::open_in_memory().unwrap(),
                vault_config(),
            ))));

        // The write surface performs it...
        assert!(
            mcp.ops()
                .update_managed_section("Daily/Today.md", "s", "new", true, None)
                .is_ok()
        );
        // ...the read-only surface refuses, like every other write tool.
        let error = read_only
            .ops()
            .update_managed_section("Daily/Today.md", "s", "newer", true, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("update_managed_section"), "{error}");
        assert!(error.contains("read-only"), "{error}");
    }

    #[test]
    fn youtube_transcript_tool_has_expected_schema() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let tools = mcp.registered_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "youtube_transcript")
            .expect("youtube_transcript tool must be registered");

        let schema = tool.input_schema.as_ref();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["additionalProperties"], json!(false));
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

    #[test]
    fn read_document_tool_has_expected_schema() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let tools = mcp.registered_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "read_document")
            .expect("read_document tool must be registered");

        let schema = tool.input_schema.as_ref();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["save"]["type"], "boolean");
        assert_eq!(schema["properties"]["folder"]["type"], "string");
        assert_eq!(schema["required"], json!(["path"]));
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    fn write_sample_pdf(path: &Path, lines: &[&str]) {
        use printpdf::*;
        let (doc, page1, layer1) = PdfDocument::new("Fixture", Mm(210.0), Mm(297.0), "Layer 1");
        let font = doc.add_builtin_font(BuiltinFont::Helvetica).unwrap();
        let layer = doc.get_page(page1).get_layer(layer1);
        let mut y = 280.0;
        for line in lines {
            layer.use_text(*line, 14.0, Mm(20.0), Mm(y), &font);
            y -= 10.0;
        }
        let bytes = doc.save_to_bytes().unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn read_document_extracts_pdf_text_and_chunks() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("attachments")).unwrap();
        write_sample_pdf(
            &temp_dir.path().join("attachments/paper.pdf"),
            &["Ingested from a PDF via MCP.", "Second extractable line."],
        );
        let mcp = build_test_mcp(temp_dir.path());

        let value = mcp
            .read_document_op("attachments/paper.pdf", false, None)
            .unwrap();

        assert_eq!(value["source_type"], "pdf");
        assert_eq!(value["source_path"], "attachments/paper.pdf");
        assert!(
            value["text"]
                .as_str()
                .unwrap()
                .contains("Ingested from a PDF")
        );
        assert!(value["chunk_count"].as_u64().unwrap() >= 1);
        assert!(value.get("saved").is_none());
    }

    #[test]
    fn read_document_with_save_writes_a_normalized_note() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("attachments")).unwrap();
        write_sample_pdf(
            &temp_dir.path().join("attachments/paper.pdf"),
            &["Persisted document body."],
        );
        let mcp = build_test_mcp(temp_dir.path());

        let value = mcp
            .read_document_op("attachments/paper.pdf", true, Some("Documents"))
            .unwrap();

        assert_eq!(value["saved"], true);
        let saved_path = value["saved_path"].as_str().unwrap();
        assert_eq!(saved_path, "Documents/paper.md");

        let note = mcp.ops().get_note(saved_path).unwrap();
        let content = note["content"].as_str().unwrap();
        assert!(content.contains("source_type: pdf"));
        assert!(content.contains("source_path: attachments/paper.pdf"));
        assert!(content.contains("Persisted document body."));
    }

    #[test]
    fn read_document_missing_file_errors_without_panic() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let err = mcp
            .read_document_op("attachments/nope.pdf", false, None)
            .unwrap_err();
        assert!(err.to_string().contains("cannot read document"));
    }

    fn extensions_with_run_id(run_id: &str) -> Extensions {
        let parts = http::Request::builder()
            .header(RUN_ID_HEADER, run_id)
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let mut extensions = Extensions::new();
        extensions.insert(parts);
        extensions
    }

    #[test]
    fn write_tool_set_covers_mutations_not_reads() {
        for write in [
            "create_note",
            "update_note",
            "update_managed_section",
            "append_to_note",
            "archive_note",
            "update_task_status",
            "inbox_add",
            "create_daily_note",
            "create_from_template",
        ] {
            assert!(is_write_tool(write), "{write} should count as a write");
        }
        // Reads and the deliberately-excluded conditional/preview tools.
        for read in [
            "get_note",
            "search_notes",
            "vault_search",
            "query_sql",
            "list_notes",
            "list_tasks",
            "read_document",
            "memory_save",
            "memory_update",
            "memory_supersede",
            "memory_delete",
        ] {
            assert!(!is_write_tool(read), "{read} must not count as a write");
        }
    }

    #[test]
    fn run_id_is_read_from_the_request_parts_header() {
        let extensions = extensions_with_run_id("run-abc");
        assert_eq!(
            run_id_from_extensions(&extensions).as_deref(),
            Some("run-abc")
        );
        // No parts at all → no run id (the stdio bridge case).
        assert_eq!(run_id_from_extensions(&Extensions::new()), None);
    }

    #[test]
    fn write_counter_is_per_run_id_and_isolated() {
        // Unique ids so parallel tests never collide on the process-global map.
        let a = "counter-test-run-a";
        let b = "counter-test-run-b";
        record_run_write(a, None);
        record_run_write(a, None);
        record_run_write(b, None);
        // take() reads and removes; two ids stay isolated.
        assert_eq!(
            take_run_writes(a),
            Some(RunWrites {
                count: 2,
                sections: vec![]
            })
        );
        assert_eq!(
            take_run_writes(b),
            Some(RunWrites {
                count: 1,
                sections: vec![]
            })
        );
        // Draining leaves nothing behind.
        assert_eq!(take_run_writes(a), None);
        assert_eq!(take_run_writes(b), None);
        // A run that never wrote has no entry.
        assert_eq!(take_run_writes("counter-test-run-never"), None);
    }

    #[test]
    fn write_counter_records_touched_managed_sections_sorted_and_deduped() {
        let run = "counter-test-run-sections";
        // update_managed_section calls contribute their section id; a repeat is
        // deduped, and the ids come back sorted.
        record_run_write(run, Some("briefing/tasks"));
        record_run_write(run, Some("briefing/meetings"));
        record_run_write(run, Some("briefing/tasks"));
        // A non-section write (e.g. update_note) bumps the count only.
        record_run_write(run, None);

        assert_eq!(
            take_run_writes(run),
            Some(RunWrites {
                count: 4,
                sections: vec![
                    "briefing/meetings".to_string(),
                    "briefing/tasks".to_string(),
                ],
            })
        );
    }

    #[test]
    fn write_counter_with_only_non_section_writes_has_empty_sections() {
        // A run that wrote update_note but touched no managed section: count >= 1
        // with an empty sections list (not None — the run did write).
        let run = "counter-test-run-nosections";
        record_run_write(run, None);
        record_run_write(run, None);
        assert_eq!(
            take_run_writes(run),
            Some(RunWrites {
                count: 2,
                sections: vec![],
            })
        );
    }

    #[test]
    fn read_document_rejects_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let mcp = build_test_mcp(temp_dir.path());

        let err = mcp
            .read_document_op("../outside.pdf", false, None)
            .unwrap_err();
        assert!(err.to_string().contains("invalid document path"));
    }
}
