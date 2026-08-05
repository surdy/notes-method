//! Tauri IPC bridge hosting the ACP agent client (ADR 0012, Decision 4).
//!
//! Per ADR 0012 the **desktop shell** — not the daemon — owns the ACP client:
//! it spawns the agent process, runs the protocol, and streams normalized
//! [`AgentEvent`]s to the Svelte chat panel over Tauri IPC. Transcripts live in
//! the daemon and are read/written by the frontend directly over HTTP; this
//! bridge is purely the live-session transport.
//!
//! One [`AcpSession`] is hosted per chat session inside a dedicated Tokio task
//! (the "pump"), which owns the session mutably and multiplexes outbound
//! commands (prompt / select-model / stop) against the inbound event stream.
//! Events are emitted on `notesmith://agent-event`; write-permission prompts
//! are emitted on `notesmith://agent-permission` and answered back through
//! [`agent_answer_permission`].
//!
//! MCP transport: the active vault is exposed to the agent as an MCP server.
//! Selection is capability-aware (ADR 0012, Decision 2): an HTTP binding to the
//! daemon's Streamable HTTP MCP endpoint (`/mcp/<vault>` read-write,
//! `/mcp-ro/<vault>` read-only) is preferred when the agent advertises
//! `mcpCapabilities.http` at `initialize` (e.g. Copilot is HTTP/SSE-only). For a
//! **local** daemon a `notesmith mcp start` stdio bridge (which itself forwards
//! to the daemon over HTTP) is supplied as a fallback for agents that do not
//! support HTTP MCP; a **remote** daemon only offers the HTTP binding.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, mpsc, oneshot};

use notesmith_agent::{
    AcpSession, AgentDescriptor, AgentDiagnosticsLog, AgentEvent, AgentSession, DiagEntry,
    DiffPreview, EditorContext, McpBinding, ModelPicker, PermissionDecider, PermissionDecision,
    PermissionRequest, extra_mcp_bindings, load_mcp_config,
};
use notesmith_config::{
    AgentEntry, AgentsConfig, CompanionMemoryConfig, McpConfig, McpServerEntry, expand_path_vars,
};

/// Event channel carrying normalized [`AgentEvent`]s to the chat panel.
pub const AGENT_EVENT: &str = "notesmith://agent-event";
/// Event channel carrying write-permission prompts to the chat panel.
pub const AGENT_PERMISSION: &str = "notesmith://agent-permission";

/// The process-global agent diagnostics log (issue #192). One bounded log is
/// shared by every [`AcpSession`] the bridge builds, so the Settings UI can read
/// recent errors and (when verbose) the "wire-ish" ACP message log across all
/// sessions. Lazily initialized; the verbose toggle defaults to off.
fn diagnostics_log() -> Arc<AgentDiagnosticsLog> {
    static LOG: OnceLock<Arc<AgentDiagnosticsLog>> = OnceLock::new();
    LOG.get_or_init(|| Arc::new(AgentDiagnosticsLog::new()))
        .clone()
}

// ---------------------------------------------------------------------------
// IPC payloads
// ---------------------------------------------------------------------------

/// One agent the user can pick. Mirrors the frontend `AgentInfo`.
#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    id: String,
    name: String,
    available: bool,
}

/// A single `[agents.<id>]` entry over the wire. Mirrors the frontend
/// `AgentEntryData` (camelCase JSON). `env` is an array of `[key, value]` pairs
/// so the Settings UI can edit it as ordered rows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentEntryDto {
    pub id: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub display_name: Option<String>,
    pub enabled: bool,
}

/// The `[agents]` section over the wire. Mirrors the frontend `AgentsConfigData`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentsConfigDto {
    pub debug: bool,
    pub entries: Vec<AgentEntryDto>,
}

/// Project the in-memory [`AgentsConfig`] to its wire DTO. Entries are emitted
/// in id order (the source `BTreeMap` is already sorted).
fn agents_config_to_dto(cfg: &AgentsConfig) -> AgentsConfigDto {
    let entries = cfg
        .entries
        .iter()
        .map(|(id, entry)| AgentEntryDto {
            id: id.clone(),
            command: entry.command.clone(),
            args: entry.args.clone(),
            env: entry
                .env
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            display_name: entry.display_name.clone(),
            enabled: entry.enabled,
        })
        .collect();
    AgentsConfigDto {
        debug: cfg.debug,
        entries,
    }
}

/// Fold the wire DTO back into an [`AgentsConfig`]. Entries with a blank id are
/// skipped (the UI may carry an empty "add agent" draft row); env pairs become a
/// `BTreeMap`, so a later duplicate key wins.
fn dto_to_agents_config(dto: AgentsConfigDto) -> AgentsConfig {
    let mut entries = BTreeMap::new();
    for entry in dto.entries {
        let id = entry.id.trim().to_string();
        if id.is_empty() {
            continue;
        }
        let env: BTreeMap<String, String> = entry.env.into_iter().collect();
        entries.insert(
            id,
            AgentEntry {
                command: entry.command,
                args: entry.args,
                env,
                display_name: entry.display_name,
                enabled: entry.enabled,
            },
        );
    }
    AgentsConfig {
        debug: dto.debug,
        entries,
    }
}

/// A single `[[mcp.servers]]` entry over the wire (ADR 0016 / #211). Mirrors
/// the frontend `McpServerData` (camelCase JSON). `env` is an array of
/// `[key, value]` pairs so the Settings UI can edit it as ordered rows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub url: Option<String>,
    /// HTTP request headers of an HTTP server, value-redacted on the way out
    /// (see [`McpHeaderDto`]). Defaults to empty so older payloads still parse.
    #[serde(default)]
    pub headers: Vec<McpHeaderDto>,
    pub display_name: Option<String>,
    pub enabled: bool,
}

/// One HTTP header row of an `[[mcp.servers]]` entry over the wire (#283).
///
/// Header values may carry bearer credentials, so — per the `ServerView`
/// precedent (ADR 0017) — they are **redacted outbound**: `mcp_servers_get`
/// always sends `value: None` with `has_value` flagging whether one is stored.
/// Inbound (`mcp_servers_set`), a non-empty `value` sets/overwrites the stored
/// value, while `None` (or an empty string) means "keep whatever is stored for
/// this server id + header name" so a Settings save never wipes a token the UI
/// never saw. Removing the row removes the header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpHeaderDto {
    /// Header name (e.g. `Authorization`). Sent in both directions.
    pub name: String,
    /// Outbound: always `None`. Inbound: `Some(value)` to set, `None`/empty to
    /// keep the stored value.
    #[serde(default)]
    pub value: Option<String>,
    /// Outbound: whether a value is stored for this header. Ignored inbound.
    #[serde(default)]
    pub has_value: bool,
}

/// The `[mcp]` section over the wire. Mirrors the frontend `McpConfigData`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompanionMemoryDto {
    pub enabled: bool,
    pub server_id: Option<String>,
    pub vault: Option<String>,
    pub read_only: bool,
}

impl Default for CompanionMemoryDto {
    fn default() -> Self {
        Self {
            enabled: false,
            server_id: None,
            vault: None,
            read_only: true,
        }
    }
}

fn companion_memory_to_dto(cfg: &CompanionMemoryConfig) -> CompanionMemoryDto {
    CompanionMemoryDto {
        enabled: cfg.enabled,
        server_id: cfg.server_id.clone(),
        vault: cfg.vault.clone(),
        read_only: cfg.read_only,
    }
}

fn dto_to_companion_memory(dto: CompanionMemoryDto) -> CompanionMemoryConfig {
    CompanionMemoryConfig {
        enabled: dto.enabled,
        server_id: dto.server_id,
        vault: dto.vault,
        read_only: dto.read_only,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigDto {
    pub servers: Vec<McpServerDto>,
    #[serde(default)]
    pub companion_memory: CompanionMemoryDto,
}

/// Project the in-memory [`McpConfig`] to its wire DTO, preserving order.
/// Header **values** are redacted (`value: None`, `has_value` set) — they may
/// carry bearer credentials and are never shown to the UI (see
/// [`McpHeaderDto`]).
fn mcp_config_to_dto(cfg: &McpConfig) -> McpConfigDto {
    let servers = cfg
        .servers
        .iter()
        .map(|entry| McpServerDto {
            id: entry.id.clone(),
            command: entry.command.clone(),
            args: entry.args.clone(),
            env: entry
                .env
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            url: entry.url.clone(),
            headers: entry
                .headers
                .iter()
                .map(|(name, value)| McpHeaderDto {
                    name: name.clone(),
                    value: None,
                    has_value: !value.is_empty(),
                })
                .collect(),
            display_name: entry.display_name.clone(),
            enabled: entry.enabled,
        })
        .collect();
    McpConfigDto {
        servers,
        companion_memory: companion_memory_to_dto(&cfg.companion_memory),
    }
}

/// Fold the wire DTO back into an [`McpConfig`]. Servers with a blank id are
/// skipped (the UI may carry an empty "add server" draft row); env pairs become
/// a `BTreeMap`, so a later duplicate key wins. Order is preserved.
///
/// `previous` is the currently persisted config: header rows arriving without a
/// value (the redacted view the UI round-trips, see [`McpHeaderDto`]) keep the
/// value stored there under the same server id + header name, so a Settings
/// save never wipes a token. A redacted row whose header no longer exists in
/// `previous` has no value at all and is dropped.
fn dto_to_mcp_config(dto: McpConfigDto, previous: &McpConfig) -> McpConfig {
    let McpConfigDto {
        servers: dto_servers,
        companion_memory,
    } = dto;
    let mut servers = Vec::new();
    for server in dto_servers {
        let id = server.id.trim().to_string();
        if id.is_empty() {
            continue;
        }
        let stored_headers = previous
            .servers
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.headers);
        let mut headers = BTreeMap::new();
        for row in server.headers {
            let name = row.name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            match row.value.filter(|value| !value.is_empty()) {
                Some(value) => {
                    headers.insert(name, value);
                }
                None => {
                    // Redacted row: preserve the stored value, if any.
                    if let Some(value) = stored_headers.and_then(|stored| stored.get(&name)) {
                        headers.insert(name, value.clone());
                    }
                }
            }
        }
        let env: BTreeMap<String, String> = server.env.into_iter().collect();
        servers.push(McpServerEntry {
            id,
            command: server.command,
            args: server.args,
            env,
            url: server.url,
            headers,
            display_name: server.display_name,
            enabled: server.enabled,
        });
    }
    McpConfig {
        servers,
        companion_memory: dto_to_companion_memory(companion_memory),
    }
}

fn companion_http_binding(
    daemon_url: &str,
    server_id: &str,
    vault: &str,
    read_only: bool,
) -> McpBinding {
    let base = daemon_url.trim_end_matches('/');
    let scope = if read_only { "mcp-ro" } else { "mcp" };
    let url = format!("{base}/{scope}/{vault}");
    let namespaced = notesmith_agent::server_name_for_namespaced_vault(server_id, vault);
    let companion_suffix = namespaced
        .strip_prefix("notesmith")
        .unwrap_or(namespaced.as_str())
        .trim_start_matches('-');
    McpBinding::http(
        if companion_suffix.is_empty() {
            "notesmith--companion".to_string()
        } else {
            format!("notesmith--companion-{companion_suffix}")
        },
        url,
    )
}

fn dedupe_key(binding: &McpBinding) -> String {
    match binding {
        McpBinding::Http { url, .. } => url.replace("/mcp-ro/", "/mcp/"),
        McpBinding::Stdio { command, args, .. } => {
            let filtered: Vec<&str> = args
                .iter()
                .map(String::as_str)
                .filter(|arg| *arg != "--read-only")
                .collect();
            format!("{command}::{}", filtered.join("\u{1f}"))
        }
    }
}

fn dedupe_companion_binding(
    active: Option<&McpBinding>,
    companion: Option<McpBinding>,
) -> Option<McpBinding> {
    let companion = companion?;
    if active.is_some_and(|binding| dedupe_key(binding) == dedupe_key(&companion)) {
        None
    } else {
        Some(ensure_unique_companion_name(active, companion))
    }
}

fn ensure_unique_companion_name(
    active: Option<&McpBinding>,
    mut companion: McpBinding,
) -> McpBinding {
    if active.is_some_and(|binding| binding.name() == companion.name()) {
        let unique_name = format!("{}-companion", companion.name());
        match &mut companion {
            McpBinding::Http { name, .. } | McpBinding::Stdio { name, .. } => {
                *name = unique_name;
            }
        }
    }
    companion
}

fn unavailable_companion_vault_message(server_name: &str, vault: &str) -> String {
    format!(
        "Companion memory vault '{vault}' on '{}' is unavailable. Refresh saved server vaults in Settings → Connection and try again.",
        server_name
    )
}

fn missing_companion_vault_message(server_name: &str, vault: &str) -> String {
    format!(
        "Companion memory vault '{vault}' was not found on '{}'. Refresh saved server vaults in Settings → Connection and try again.",
        server_name
    )
}

fn should_refresh_companion_cache(
    entry: Option<&notesmith_tauri::vault_cache::ServerVaults>,
) -> bool {
    match entry {
        None => true,
        Some(entry) => {
            entry.last_seen.is_none()
                && matches!(
                    entry.status,
                    notesmith_tauri::vault_cache::VaultListStatus::Fresh
                        | notesmith_tauri::vault_cache::VaultListStatus::Stale
                )
        }
    }
}

#[cfg(test)]
fn resolve_companion_cache_entry<F>(
    cache_entry: Option<notesmith_tauri::vault_cache::ServerVaults>,
    server_name: &str,
    vault: &str,
    refresh: F,
) -> Result<notesmith_tauri::vault_cache::ServerVaults, String>
where
    F: FnOnce() -> Option<notesmith_tauri::vault_cache::ServerVaults>,
{
    let cache_entry = if should_refresh_companion_cache(cache_entry.as_ref()) {
        refresh()
    } else {
        cache_entry
    };

    validate_companion_cache_entry(cache_entry, server_name, vault)
}

fn validate_companion_cache_entry(
    cache_entry: Option<notesmith_tauri::vault_cache::ServerVaults>,
    server_name: &str,
    vault: &str,
) -> Result<notesmith_tauri::vault_cache::ServerVaults, String> {
    let cache_entry =
        cache_entry.ok_or_else(|| unavailable_companion_vault_message(server_name, vault))?;

    use notesmith_tauri::vault_cache::VaultListStatus;
    match cache_entry.status {
        VaultListStatus::AuthError => {
            return Err(format!(
                "Companion memory server '{server_name}' rejected the saved credentials. Update the connection token in Settings → Connection and try again."
            ));
        }
        VaultListStatus::Unreachable => {
            return Err(format!(
                "Companion memory server '{server_name}' is unreachable. Refresh saved server vaults in Settings → Connection and try again."
            ));
        }
        VaultListStatus::Fresh | VaultListStatus::Stale => {}
    }

    if !cache_entry
        .vaults
        .iter()
        .any(|candidate| candidate == vault)
    {
        return Err(missing_companion_vault_message(server_name, vault));
    }

    Ok(cache_entry)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionOptions {
    vault: String,
    agent: String,
    read_only: bool,
    #[serde(default)]
    break_glass: bool,
    /// Tools the user has already granted "Always Allow" for this vault, fetched
    /// by the frontend from the daemon grant store and passed in to pre-seed the
    /// session permission state so they never re-prompt (issue #189).
    #[serde(default)]
    persisted_grants: Vec<String>,
    /// One-time session preamble assembled by the frontend from always-on
    /// discovered instructions and the active persona's body (issues #210/#212,
    /// ADR 0016). Injected via [`AcpSession::with_skill`]; `None` = no preamble.
    /// Bounded when the preamble is assembled, keeping it small.
    #[serde(default)]
    preamble: Option<String>,
    /// The agent's ACP `sessionId` from a prior run of this chat thread, to
    /// resume via `session/load` instead of starting fresh (#262). `None` (the
    /// default) starts a new session. A stale/unknown id degrades to a fresh
    /// session agent-side, so this is always safe to pass.
    #[serde(default)]
    resume_acp_session_id: Option<String>,
}

/// Editor context handed in with a turn. Mirrors the frontend `EditorContext`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorContextDto {
    #[serde(default)]
    active_note: Option<String>,
    #[serde(default)]
    active_title: Option<String>,
    #[serde(default)]
    selection: Option<String>,
    #[serde(default)]
    open_tabs: Vec<String>,
}

impl From<EditorContextDto> for EditorContext {
    fn from(dto: EditorContextDto) -> Self {
        EditorContext {
            active_path: dto.active_note,
            active_title: dto.active_title,
            selection: dto.selection,
            open_tabs: dto.open_tabs,
        }
    }
}

/// A selectable model option. Mirrors the frontend `ModelOption`.
#[derive(Debug, Clone, Serialize)]
pub struct ModelOptionDto {
    id: String,
    name: String,
    description: Option<String>,
}

/// The model picker advertised by the agent. Mirrors the frontend `ModelPicker`.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPickerDto {
    current: String,
    options: Vec<ModelOptionDto>,
}

impl From<&ModelPicker> for ModelPickerDto {
    fn from(picker: &ModelPicker) -> Self {
        ModelPickerDto {
            current: picker.current().to_string(),
            options: picker
                .options()
                .iter()
                .map(|o| ModelOptionDto {
                    id: o.id.clone(),
                    name: o.name.clone(),
                    description: o.description.clone(),
                })
                .collect(),
        }
    }
}

/// Result of starting a session. Mirrors the frontend `StartSessionResult`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResult {
    session_id: String,
    models: Option<ModelPickerDto>,
    /// The agent's resolved ACP `sessionId` for this session, to persist per
    /// thread so it can be resumed later (#262). `None` if the agent returned
    /// no session id.
    acp_session_id: Option<String>,
}

/// `notesmith://agent-event` payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentEventPayload {
    session_id: String,
    event: AgentEvent,
}

/// `notesmith://agent-permission` payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionPayload {
    session_id: String,
    request_id: String,
    request: PermissionRequestDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionRequestDto {
    tool: String,
    kind: Option<String>,
    /// The proposed change to preview before deciding (issue #189). `None` for
    /// non-file actions (e.g. command runs).
    diff: Option<DiffPreviewDto>,
}

/// A proposed file change carried with a permission prompt so the UI can show a
/// diff/preview before the user decides (issue #189). Mirrors the frontend
/// `PermissionRequest.diff`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffPreviewDto {
    path: String,
    old_text: Option<String>,
    new_text: String,
}

impl From<DiffPreview> for DiffPreviewDto {
    fn from(diff: DiffPreview) -> Self {
        DiffPreviewDto {
            path: diff.path,
            old_text: diff.old_text,
            new_text: diff.new_text,
        }
    }
}

// ---------------------------------------------------------------------------
// Bridge state
// ---------------------------------------------------------------------------

/// Outbound commands sent from IPC handlers to a session's pump task.
enum SessionCommand {
    Prompt {
        text: String,
        editor: Option<EditorContext>,
    },
    SelectModel {
        value: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Stop,
}

struct SessionEntry {
    commands: mpsc::UnboundedSender<SessionCommand>,
    opts: StartSessionOptions,
}

/// Shared, managed bridge state. Holds the live sessions and the pending
/// permission prompts awaiting a user answer.
pub struct AgentBridge {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    pending: Arc<PendingPermissions>,
    next_session: AtomicU64,
}

impl AgentBridge {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            pending: Arc::new(PendingPermissions::new()),
            next_session: AtomicU64::new(1),
        }
    }
}

impl Default for AgentBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry of in-flight permission prompts keyed by request id.
struct PendingPermissions {
    inner: std::sync::Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>,
    next_id: AtomicU64,
}

impl PendingPermissions {
    fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    fn register(&self, tx: oneshot::Sender<PermissionDecision>) -> String {
        let id = format!("perm-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        if let Ok(mut map) = self.inner.lock() {
            map.insert(id.clone(), tx);
        }
        id
    }

    fn answer(&self, request_id: &str, decision: PermissionDecision) {
        let tx = self
            .inner
            .lock()
            .ok()
            .and_then(|mut m| m.remove(request_id));
        if let Some(tx) = tx {
            let _ = tx.send(decision);
        }
    }
}

// ---------------------------------------------------------------------------
// Permission decider that prompts the UI
// ---------------------------------------------------------------------------

/// A [`PermissionDecider`] that surfaces each prompt to the chat panel and
/// blocks on the user's answer. If the UI is gone (window closed, channel
/// dropped) it falls back to a safe `Deny`.
struct BridgeDecider {
    app: AppHandle,
    session_id: String,
    pending: Arc<PendingPermissions>,
}

impl PermissionDecider for BridgeDecider {
    fn decide(&self, request: PermissionRequest) -> BoxFuture<'static, PermissionDecision> {
        let app = self.app.clone();
        let session_id = self.session_id.clone();
        let pending = self.pending.clone();
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            let request_id = pending.register(tx);
            let payload = PermissionPayload {
                session_id,
                request_id,
                request: PermissionRequestDto {
                    tool: request.tool,
                    kind: request.kind,
                    diff: request.diff.map(DiffPreviewDto::from),
                },
            };
            if app.emit(AGENT_PERMISSION, payload).is_err() {
                return PermissionDecision::Deny;
            }
            rx.await.unwrap_or(PermissionDecision::Deny)
        })
    }
}

// ---------------------------------------------------------------------------
// Session construction + pump
// ---------------------------------------------------------------------------

fn binary_on_path(program: &str) -> bool {
    if program.contains('/') {
        return PathBuf::from(program).exists();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(program).exists())
}

/// Whether the additional npm-package gate (if any) is satisfied.
///
/// Agents launched directly from a binary on PATH have no gate (`None` →
/// always satisfied). Agents launched through a package runner (`npx`) are only
/// truly available when their adapter package resolves locally — otherwise the
/// "available" badge would be true on any machine with Node.js even though the
/// adapter is not installed (issue #241).
fn package_available(pkg: Option<&str>) -> bool {
    match pkg {
        None => true,
        Some(pkg) => npm_package_available(pkg),
    }
}

/// The npm package that must additionally resolve for `id` to count as
/// available — but only while `program` is still the registry's default
/// launcher. A user `[agents].<id>.command` override points at a real binary and
/// needs no package gate, so the gate is dropped when the program differs from
/// the descriptor's default availability program.
fn availability_package_for(id: &str, program: &str) -> Option<String> {
    notesmith_agent::descriptor(id)
        .filter(|descriptor| descriptor.availability_program() == program)
        .and_then(|descriptor| descriptor.availability_package())
        .map(|pkg| pkg.to_string())
}

/// Whether an npm `pkg` resolves locally without a network round-trip and
/// without ever spawning the package itself (the Claude adapter is an stdio ACP
/// server that would hang). Checks the two places npx/npm actually place it:
/// the npx on-demand cache (`npx --yes <pkg>`) and the global modules
/// (`npm i -g <pkg>`).
pub(crate) fn npm_package_available(pkg: &str) -> bool {
    if let Some(cache) = npm_cache_root()
        && npx_cache_has_package(&cache, pkg)
    {
        return true;
    }
    if let Some(root) = npm_global_root()
        && package_in_node_modules(&root, pkg)
    {
        return true;
    }
    false
}

/// The npm cache directory: `$npm_config_cache` / `$NPM_CONFIG_CACHE` when set
/// (as npm itself honors), otherwise `~/.npm`.
fn npm_cache_root() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("npm_config_cache")
        .or_else(|| std::env::var_os("NPM_CONFIG_CACHE"))
        .filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(value));
    }
    home_dir().map(|home| home.join(".npm"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Whether any npx on-demand cache entry (`<cache>/_npx/<hash>/node_modules`)
/// contains `pkg`. Resilient to a missing/unreadable cache dir (ADR 0009).
fn npx_cache_has_package(cache_root: &std::path::Path, pkg: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(cache_root.join("_npx")) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| package_in_node_modules(&entry.path().join("node_modules"), pkg))
}

/// Whether `node_modules/<pkg>/package.json` exists (handles scoped packages
/// like `@scope/name`, whose `/` nests correctly).
fn package_in_node_modules(node_modules: &std::path::Path, pkg: &str) -> bool {
    node_modules.join(pkg).join("package.json").exists()
}

/// The global npm modules directory via `npm root -g`, bounded so a slow or
/// misconfigured npm cannot stall the picker, and never spawning anything that
/// reads stdin. Returns `None` when npm is absent or the probe fails.
fn npm_global_root() -> Option<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let output = std::process::Command::new("npm")
            .args(["root", "-g"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();
        let _ = tx.send(output);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(output)) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (!path.is_empty()).then(|| PathBuf::from(path))
        }
        _ => None,
    }
}

/// Load the `[agents]` config section, degrading to defaults (never panicking)
/// when the global config is missing or unreadable (ADR 0009).
fn load_agents_config() -> AgentsConfig {
    notesmith_config::GlobalConfig::load()
        .map(|config| config.agents)
        .unwrap_or_default()
}

/// Merge the built-in registry with the user's `[agents]` config to produce the
/// effective agent list (ADR 0013, decision 4): `(id, display_name,
/// availability_program)` per agent. A user override of a built-in replaces its
/// availability program; `enabled = false` hides a built-in; custom ids not in
/// the registry are appended. Tilde / `$VAR` in commands are expanded.
fn effective_agents(
    registry: &[AgentDescriptor],
    cfg: &AgentsConfig,
) -> Vec<(String, String, String)> {
    let mut agents = Vec::new();

    // Built-ins, honoring overrides and omitting disabled ones.
    for descriptor in registry {
        match cfg.entries.get(descriptor.id) {
            Some(entry) if !entry.enabled => continue,
            Some(entry) => {
                let program = match entry.command.as_deref().filter(|c| !c.is_empty()) {
                    Some(command) => expand_path_vars(command),
                    None => descriptor.availability_program().to_string(),
                };
                agents.push((
                    descriptor.id.to_string(),
                    descriptor.display_name.to_string(),
                    program,
                ));
            }
            None => agents.push((
                descriptor.id.to_string(),
                descriptor.display_name.to_string(),
                descriptor.availability_program().to_string(),
            )),
        }
    }

    // Custom agents: configured ids that are not built-ins (enabled, with a
    // command). A command-less custom entry is not launchable, so it is omitted.
    for (id, entry) in &cfg.entries {
        if registry.iter().any(|descriptor| descriptor.id == *id) || !entry.enabled {
            continue;
        }
        let Some(command) = entry.command.as_deref().filter(|c| !c.is_empty()) else {
            continue;
        };
        let display_name = entry.display_name.clone().unwrap_or_else(|| id.clone());
        agents.push((id.clone(), display_name, expand_path_vars(command)));
    }

    agents
}

fn agent_catalog() -> Vec<(String, String, String)> {
    effective_agents(notesmith_agent::builtin_registry(), &load_agents_config())
}

/// Build an [`AcpSession`] for a custom or overridden agent command: expand
/// `~`/`$VAR` in the command and each arg, and apply the configured env.
fn custom_session(entry: &AgentEntry, command: &str) -> AcpSession {
    let program = expand_path_vars(command);
    let args: Vec<String> = entry.args.iter().map(|arg| expand_path_vars(arg)).collect();
    let env: Vec<(String, String)> = entry
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    AcpSession::new(program, args).with_env(env)
}

/// Resolve the base [`AcpSession`] for `agent`, merging the built-in registry
/// with the user's `[agents]` config (ADR 0013, decision 4): a user entry with a
/// `command` wins (custom command/args/env, keeping a built-in's setup hint); a
/// built-in with no override uses its declarative defaults; a disabled built-in
/// or an unknown id is rejected.
fn resolve_session(agent: &str, cfg: &AgentsConfig) -> Result<AcpSession, String> {
    let descriptor = notesmith_agent::descriptor(agent);
    let entry = cfg.entries.get(agent);

    match (descriptor, entry) {
        (Some(descriptor), Some(entry)) => {
            if !entry.enabled {
                return Err(format!("agent '{agent}' is disabled"));
            }
            match entry.command.as_deref().filter(|c| !c.is_empty()) {
                Some(command) => {
                    let mut session = custom_session(entry, command);
                    if !descriptor.setup_hint.is_empty() {
                        session = session.with_setup_hint(descriptor.setup_hint);
                    }
                    Ok(session)
                }
                None => Ok(descriptor.session(None)),
            }
        }
        (Some(descriptor), None) => Ok(descriptor.session(None)),
        (None, Some(entry)) => {
            if !entry.enabled {
                return Err(format!("agent '{agent}' is disabled"));
            }
            let command = entry
                .command
                .as_deref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| format!("agent '{agent}' has no command configured"))?;
            Ok(custom_session(entry, command))
        }
        (None, None) => Err(format!("unknown agent '{agent}'")),
    }
}

/// Build (but do not start) an [`AcpSession`] for `opts`, wired to the MCP
/// endpoint of the daemon the calling window (`window_label`) is connected to,
/// plus a UI permission decider.
async fn resolve_companion_memory_binding(
    app: &AppHandle,
    window_label: &str,
    opts: &StartSessionOptions,
    config: &McpConfig,
) -> Result<Option<McpBinding>, String> {
    let companion = &config.companion_memory;
    if !companion.enabled {
        return Ok(None);
    }

    let server_id = companion
        .server_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Companion memory is enabled but no saved server is selected.".to_string()
        })?;
    let vault = companion
        .vault
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Companion memory is enabled but no companion vault is selected.".to_string()
        })?;

    if crate::window_server_id(app, window_label) == server_id && opts.vault == vault {
        return Ok(None);
    }

    let servers = app.state::<crate::ServersState>().snapshot();
    let server = servers.get(server_id).ok_or_else(|| {
        format!("Companion memory server '{server_id}' is no longer in saved connections.")
    })?;

    let cache_entry = app.state::<crate::VaultCacheState>().get(server_id);
    let cache_entry = if should_refresh_companion_cache(cache_entry.as_ref()) {
        crate::refresh_server_vault_cache(app.clone(), server_id.to_string()).await
    } else {
        cache_entry
    };
    validate_companion_cache_entry(cache_entry, &server.name, vault)?;

    let effective_read_only = opts.read_only || companion.read_only;
    let binding = companion_http_binding(&server.url, server_id, vault, effective_read_only);
    let active = session_primary_binding(app, window_label, opts);
    Ok(dedupe_companion_binding(Some(&active), Some(binding)))
}

fn session_primary_binding(
    app: &AppHandle,
    window_label: &str,
    opts: &StartSessionOptions,
) -> McpBinding {
    let (daemon_url, _) = crate::window_daemon_target(app, window_label);
    http_binding(&daemon_url, opts)
}

async fn build_session(
    app: &AppHandle,
    opts: &StartSessionOptions,
    session_id: &str,
    window_label: &str,
    pending: Arc<PendingPermissions>,
) -> Result<AcpSession, String> {
    let mut session = resolve_session(opts.agent.as_str(), &load_agents_config())?;
    let mcp_config = load_mcp_config();

    // Scope the working directory (and any break-glass fs access) to the vault.
    if let Some(path) = vault_root(&opts.vault) {
        session = session.in_dir(Some(path));
    }

    // Expose the active vault to the agent as an MCP server. The transport is
    // chosen by the agent's advertised `mcpCapabilities` during the ACP
    // handshake (see `AcpSession`), not assumed up front:
    //   - **HTTP** (preferred) points at the daemon's Streamable HTTP endpoint
    //     (`/mcp/<vault>` read-write, `/mcp-ro/<vault>` read-only). Every
    //     HTTP-capable agent uses this — including GitHub Copilot, whose ACP
    //     client supports *only* HTTP/SSE MCP and silently ignores stdio.
    //   - **stdio** (`notesmith mcp start` bridge) is supplied only for a local
    //     daemon, as a fallback for agents that do not advertise HTTP MCP. The
    //     bridge forwards to the same daemon over HTTP so indexes stay shared
    //     (ADR 0010 Phase 3).
    // Read-only vs read-write scope is carried on each binding.
    //
    // Resolve the daemon from the *calling window's* connection (ADR 0017), not
    // the global active one: the vault the window shows may live only on a
    // remote daemon, so binding its MCP server to the local sidecar would point
    // at the wrong daemon and fail to connect (issue #259 follow-up).
    let (daemon_url, local) = crate::window_daemon_target(app, window_label);
    let http = http_binding(&daemon_url, opts);
    session = session.with_mcp(http);
    if local {
        let bin = crate::resolve_sidecar_path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "notesmith".to_string());
        session = session.with_mcp_stdio_fallback(McpBinding::local_bridge(
            bin,
            &opts.vault,
            opts.read_only,
        ));
    }

    let mut extra = extra_mcp_bindings(&mcp_config);
    if let Some(companion) =
        resolve_companion_memory_binding(app, window_label, opts, &mcp_config).await?
    {
        extra.insert(0, companion);
        session = session.with_companion_memory(true);
    }
    session = session.with_extra_mcp(extra);

    // Companion memory and user-configured external MCP servers (ADR 0016 /
    // #211) ride alongside the built-in vault binding. Disabled or malformed
    // entries are skipped without erroring (ADR 0009).

    let decider = Arc::new(BridgeDecider {
        app: app.clone(),
        session_id: session_id.to_string(),
        pending,
    });

    Ok(session
        .read_only(opts.read_only)
        .with_vault_name(opts.vault.clone())
        .with_skill(opts.preamble.clone())
        .with_local_io(opts.break_glass)
        .with_granted_tools(opts.persisted_grants.clone())
        .with_diagnostics(diagnostics_log())
        .resume_from(opts.resume_acp_session_id.clone())
        .with_permission_decider(decider))
}

/// Build the daemon's Streamable HTTP MCP binding for the requested vault and
/// scope (`/mcp-ro/<vault>` read-only, `/mcp/<vault>` read-write).
fn http_binding(daemon_url: &str, opts: &StartSessionOptions) -> McpBinding {
    let base = daemon_url.trim_end_matches('/');
    let scope = if opts.read_only { "mcp-ro" } else { "mcp" };
    let url = format!("{base}/{scope}/{}", opts.vault);
    McpBinding::http(notesmith_agent::server_name_for_vault(&opts.vault), url)
}

fn vault_root(vault: &str) -> Option<PathBuf> {
    notesmith_config::GlobalConfig::load()
        .ok()
        .and_then(|config| config.vault(vault).map(|reg| reg.path.clone()))
}

/// Drive one session: own the [`AcpSession`] mutably and multiplex outbound
/// commands against the inbound event stream until the session ends or is
/// stopped.
async fn run_session(
    mut session: AcpSession,
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
    app: AppHandle,
    session_id: String,
) {
    loop {
        tokio::select! {
            maybe_event = session.next_event() => {
                match maybe_event {
                    Some(event) => {
                        let _ = app.emit(
                            AGENT_EVENT,
                            AgentEventPayload { session_id: session_id.clone(), event },
                        );
                    }
                    None => break,
                }
            }
            maybe_cmd = commands.recv() => {
                match maybe_cmd {
                    Some(SessionCommand::Prompt { text, editor }) => {
                        let result = match editor {
                            Some(editor) => session.send_with_context(&text, editor).await,
                            None => session.send(&text).await,
                        };
                        if let Err(error) = result {
                            let _ = app.emit(
                                AGENT_EVENT,
                                AgentEventPayload {
                                    session_id: session_id.clone(),
                                    event: AgentEvent::Error { message: error.to_string() },
                                },
                            );
                        }
                    }
                    Some(SessionCommand::SelectModel { value, reply }) => {
                        let _ = reply.send(
                            session.select_model(&value).await.map_err(|e| e.to_string()),
                        );
                    }
                    Some(SessionCommand::Stop) | None => break,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// IPC commands
// ---------------------------------------------------------------------------

/// Build, eagerly start, and spawn the pump for a session described by `opts`.
/// Returns the live entry plus the model picker captured at handshake.
async fn spawn_session(
    app: &AppHandle,
    bridge: &AgentBridge,
    session_id: &str,
    window_label: &str,
    opts: StartSessionOptions,
) -> Result<(SessionEntry, Option<ModelPickerDto>, Option<String>), String> {
    let mut session =
        build_session(app, &opts, session_id, window_label, bridge.pending.clone()).await?;

    // Eagerly run the handshake so the model picker is available immediately.
    session.start().await.map_err(|e| e.to_string())?;
    let models = session.model_picker().as_ref().map(ModelPickerDto::from);
    // Capture the agent's resolved sessionId (fresh or resumed) so the caller
    // can persist it per thread for later resume (#262).
    let acp_session_id = session.agent_session_id();

    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(run_session(
        session,
        rx,
        app.clone(),
        session_id.to_string(),
    ));

    Ok((SessionEntry { commands: tx, opts }, models, acp_session_id))
}

#[tauri::command]
pub async fn agent_list() -> Result<Vec<AgentInfo>, String> {
    Ok(agent_catalog()
        .into_iter()
        .map(|(id, name, program)| AgentInfo {
            id: id.to_string(),
            name: name.to_string(),
            available: binary_on_path(&program)
                && package_available(availability_package_for(&id, &program).as_deref()),
        })
        .collect())
}

/// Read the `[agents]` section of the global config for the Settings surface
/// (ADR 0013, decision 7). Degrades to the default (empty) config when the file
/// is absent or unreadable (ADR 0009) rather than erroring.
#[tauri::command]
pub async fn agent_config_get() -> Result<AgentsConfigDto, String> {
    let config = notesmith_config::GlobalConfig::load().unwrap_or_default();
    Ok(agents_config_to_dto(&config.agents))
}

/// Replace the `[agents]` section of the global config from the Settings surface
/// (ADR 0013, decision 7). Loads the current global config (defaulting when
/// absent), swaps in the edited agents section, and persists it. A write failure
/// surfaces to the UI as an `Err` — this is an explicit user action, not a hot
/// path, so an error is the correct outcome (not a panic, per ADR 0009).
#[tauri::command]
pub async fn agent_config_set(config: AgentsConfigDto) -> Result<(), String> {
    let path = notesmith_config::GlobalConfig::default_path()
        .ok_or_else(|| "could not determine the config directory".to_string())?;
    let mut global = notesmith_config::GlobalConfig::load().unwrap_or_default();
    global.agents = dto_to_agents_config(config);
    global.save_to(&path).map_err(|error| error.to_string())
}

/// Read the `[mcp]` section of the global config for the MCP server management
/// surface (ADR 0016 / #211). Degrades to the default (empty) config when the
/// file is absent or unreadable (ADR 0009) rather than erroring. The built-in
/// per-vault daemon tools are not included here — the UI renders them as a
/// static, non-removable entry.
#[tauri::command]
pub async fn mcp_servers_get() -> Result<McpConfigDto, String> {
    let config = notesmith_config::GlobalConfig::load().unwrap_or_default();
    Ok(mcp_config_to_dto(&config.mcp))
}

/// Replace the `[mcp]` section of the global config from the management surface
/// (ADR 0016 / #211). Loads the current global config (defaulting when absent),
/// swaps in the edited MCP section, and persists it. Header values arrive
/// redacted from the UI, so the fold merge-preserves them against the config
/// being replaced (#283). A write failure surfaces to the UI as an `Err` — an
/// explicit user action, not a hot path (ADR 0009).
#[tauri::command]
pub async fn mcp_servers_set(config: McpConfigDto) -> Result<(), String> {
    let path = notesmith_config::GlobalConfig::default_path()
        .ok_or_else(|| "could not determine the config directory".to_string())?;
    let mut global = notesmith_config::GlobalConfig::load().unwrap_or_default();
    global.mcp = dto_to_mcp_config(config, &global.mcp);
    global.save_to(&path).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn agent_start(
    app: AppHandle,
    window: tauri::Window,
    bridge: tauri::State<'_, AgentBridge>,
    opts: StartSessionOptions,
) -> Result<StartSessionResult, String> {
    let session_id = format!(
        "sess-{}",
        bridge.next_session.fetch_add(1, Ordering::Relaxed)
    );

    let (entry, models, acp_session_id) =
        spawn_session(&app, &bridge, &session_id, window.label(), opts).await?;

    bridge
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), entry);

    Ok(StartSessionResult {
        session_id,
        models,
        acp_session_id,
    })
}

#[tauri::command]
pub async fn agent_prompt(
    bridge: tauri::State<'_, AgentBridge>,
    session_id: String,
    text: String,
    editor: Option<EditorContextDto>,
) -> Result<(), String> {
    let sessions = bridge.sessions.lock().await;
    let entry = sessions
        .get(&session_id)
        .ok_or_else(|| format!("no such session '{session_id}'"))?;
    entry
        .commands
        .send(SessionCommand::Prompt {
            text,
            editor: editor.map(EditorContext::from),
        })
        .map_err(|_| "agent session has ended".to_string())
}

#[tauri::command]
pub async fn agent_select_model(
    bridge: tauri::State<'_, AgentBridge>,
    session_id: String,
    value: String,
) -> Result<(), String> {
    let reply_rx = {
        let sessions = bridge.sessions.lock().await;
        let entry = sessions
            .get(&session_id)
            .ok_or_else(|| format!("no such session '{session_id}'"))?;
        let (reply, reply_rx) = oneshot::channel();
        entry
            .commands
            .send(SessionCommand::SelectModel { value, reply })
            .map_err(|_| "agent session has ended".to_string())?;
        reply_rx
    };
    reply_rx
        .await
        .map_err(|_| "agent session has ended".to_string())?
}

#[tauri::command]
pub async fn agent_set_read_only(
    app: AppHandle,
    window: tauri::Window,
    bridge: tauri::State<'_, AgentBridge>,
    session_id: String,
    read_only: bool,
) -> Result<Option<String>, String> {
    // AcpSession fixes its scope at construction (the MCP endpoint + permission
    // policy are chosen up front), so a runtime mode switch rebuilds the session
    // in place under the same id. Conversation history is preserved client-side
    // (and in the daemon transcript); only the agent-side context resets, which
    // is the correct behaviour when tightening or loosening write access.
    let mut opts = {
        let sessions = bridge.sessions.lock().await;
        let entry = sessions
            .get(&session_id)
            .ok_or_else(|| format!("no such session '{session_id}'"))?;
        if entry.opts.read_only == read_only {
            return Ok(None);
        }
        let _ = entry.commands.send(SessionCommand::Stop);
        entry.opts.clone()
    };
    opts.read_only = read_only;
    // The rebuild intentionally starts a fresh agent context (see above), so do
    // not attempt to resume the prior ACP session here — resuming would fight the
    // reset and, if the stale id no longer loads, silently fork a new session
    // whose id the caller never learns. The new session's id is returned below so
    // the frontend can re-bind the thread to it for future resume (#262).
    opts.resume_acp_session_id = None;

    let (entry, _models, acp_session_id) =
        spawn_session(&app, &bridge, &session_id, window.label(), opts).await?;
    bridge.sessions.lock().await.insert(session_id, entry);
    Ok(acp_session_id)
}

#[tauri::command]
pub async fn agent_answer_permission(
    bridge: tauri::State<'_, AgentBridge>,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    // Map the frontend's decision string to the session policy. "Always Allow"
    // persistence is frontend-orchestrated (the chat store POSTs the grant to
    // the daemon store), so here both "allow_session" and "allow_always" resolve
    // to the same in-session `AllowAlways` (allow + remember this session). The
    // legacy "allow_always" string is accepted for forward/backward tolerance.
    let decision = match decision.as_str() {
        "allow_once" => PermissionDecision::AllowOnce,
        "allow_session" | "allow_always" => PermissionDecision::AllowAlways,
        "deny" => PermissionDecision::Deny,
        other => return Err(format!("unknown decision '{other}'")),
    };
    bridge.pending.answer(&request_id, decision);
    Ok(())
}

#[tauri::command]
pub async fn agent_stop(
    bridge: tauri::State<'_, AgentBridge>,
    session_id: String,
) -> Result<(), String> {
    if let Some(entry) = bridge.sessions.lock().await.remove(&session_id) {
        let _ = entry.commands.send(SessionCommand::Stop);
    }
    Ok(())
}

/// Snapshot the process-global agent diagnostics log: recent agent errors and,
/// when verbose capture is on, the mediated ACP "wire" messages (issue #192).
/// Newest entries are last.
#[tauri::command]
pub async fn agent_diagnostics_log() -> Result<Vec<DiagEntry>, String> {
    Ok(diagnostics_log().snapshot())
}

/// Toggle verbose ACP wire capture on the diagnostics log. Off by default;
/// errors are recorded regardless (issue #192).
#[tauri::command]
pub async fn agent_diagnostics_set_verbose(verbose: bool) -> Result<(), String> {
    diagnostics_log().set_verbose(verbose);
    Ok(())
}

/// Clear all retained diagnostics entries (issue #192).
#[tauri::command]
pub async fn agent_diagnostics_clear() -> Result<(), String> {
    diagnostics_log().clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_config::AgentEntry;
    use std::collections::BTreeMap;

    fn registry() -> &'static [AgentDescriptor] {
        notesmith_agent::builtin_registry()
    }

    fn find<'a>(
        agents: &'a [(String, String, String)],
        id: &str,
    ) -> Option<&'a (String, String, String)> {
        agents.iter().find(|(agent_id, _, _)| agent_id == id)
    }

    #[test]
    fn no_config_matches_registry_derived_list() {
        let agents = effective_agents(registry(), &AgentsConfig::default());

        let expected: Vec<(String, String, String)> = registry()
            .iter()
            .map(|descriptor| {
                (
                    descriptor.id.to_string(),
                    descriptor.display_name.to_string(),
                    descriptor.availability_program().to_string(),
                )
            })
            .collect();

        assert_eq!(agents, expected);
    }

    #[test]
    fn override_changes_a_builtin_availability_program() {
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "copilot".to_string(),
            AgentEntry {
                command: Some("/opt/copilot/bin/copilot".to_string()),
                args: vec!["--acp".to_string()],
                ..AgentEntry::default()
            },
        );

        let agents = effective_agents(registry(), &cfg);
        let copilot = find(&agents, "copilot").expect("copilot present");
        assert_eq!(copilot.2, "/opt/copilot/bin/copilot");
        // The display name is still the registry's.
        assert_eq!(copilot.1, "GitHub Copilot");
        // The count is unchanged (override, not addition).
        assert_eq!(agents.len(), registry().len());
    }

    #[test]
    fn disabled_builtin_is_removed() {
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "claude".to_string(),
            AgentEntry {
                enabled: false,
                ..AgentEntry::default()
            },
        );

        let agents = effective_agents(registry(), &cfg);
        assert!(find(&agents, "claude").is_none());
        assert_eq!(agents.len(), registry().len() - 1);
    }

    #[test]
    fn custom_entry_is_appended_with_display_name_and_command() {
        let mut env = BTreeMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "my-agent".to_string(),
            AgentEntry {
                command: Some("/usr/local/bin/my-agent".to_string()),
                args: vec!["--acp".to_string()],
                env,
                display_name: Some("My Agent".to_string()),
                enabled: true,
            },
        );

        let agents = effective_agents(registry(), &cfg);
        assert_eq!(agents.len(), registry().len() + 1);
        let custom = find(&agents, "my-agent").expect("custom agent appended");
        assert_eq!(custom.1, "My Agent");
        assert_eq!(custom.2, "/usr/local/bin/my-agent");
    }

    #[test]
    fn custom_entry_falls_back_to_id_for_display_name() {
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "bespoke".to_string(),
            AgentEntry {
                command: Some("bespoke-acp".to_string()),
                ..AgentEntry::default()
            },
        );

        let agents = effective_agents(registry(), &cfg);
        let custom = find(&agents, "bespoke").expect("custom agent appended");
        assert_eq!(custom.1, "bespoke");
        assert_eq!(custom.2, "bespoke-acp");
    }

    #[test]
    fn disabled_custom_entry_is_not_appended() {
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "off-agent".to_string(),
            AgentEntry {
                command: Some("off-agent".to_string()),
                enabled: false,
                ..AgentEntry::default()
            },
        );

        let agents = effective_agents(registry(), &cfg);
        assert!(find(&agents, "off-agent").is_none());
        assert_eq!(agents.len(), registry().len());
    }

    #[test]
    fn command_less_custom_entry_is_omitted() {
        let mut cfg = AgentsConfig::default();
        cfg.entries
            .insert("no-command".to_string(), AgentEntry::default());

        let agents = effective_agents(registry(), &cfg);
        assert!(find(&agents, "no-command").is_none());
        assert_eq!(agents.len(), registry().len());
    }

    #[test]
    fn resolve_session_errors_for_unknown_agent() {
        let cfg = AgentsConfig::default();
        let error = resolve_session("nope", &cfg)
            .err()
            .expect("expected an error");
        assert!(error.contains("unknown agent"));
    }

    #[test]
    fn resolve_session_errors_for_disabled_builtin() {
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "copilot".to_string(),
            AgentEntry {
                enabled: false,
                ..AgentEntry::default()
            },
        );
        let error = resolve_session("copilot", &cfg)
            .err()
            .expect("expected an error");
        assert!(error.contains("disabled"));
    }

    #[test]
    fn resolve_session_uses_builtin_default_without_override() {
        let cfg = AgentsConfig::default();
        let session = resolve_session("copilot", &cfg).expect("copilot resolves");
        assert_eq!(session.program(), "copilot");
        assert_eq!(session.args(), vec!["--acp".to_string()]);
    }

    #[test]
    fn resolve_session_applies_override_command_args_and_env() {
        let mut env = BTreeMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "copilot".to_string(),
            AgentEntry {
                command: Some("/opt/copilot".to_string()),
                args: vec!["--acp".to_string(), "--verbose".to_string()],
                env,
                ..AgentEntry::default()
            },
        );
        let session = resolve_session("copilot", &cfg).expect("override resolves");
        assert_eq!(session.program(), "/opt/copilot");
        assert_eq!(
            session.args(),
            vec!["--acp".to_string(), "--verbose".to_string()]
        );
        assert_eq!(session.env(), vec![("FOO".to_string(), "bar".to_string())]);
        // The descriptor's setup hint is preserved on an override.
        assert!(session.setup_hint().is_some());
    }

    #[test]
    fn resolve_session_builds_custom_agent_verbatim() {
        let mut cfg = AgentsConfig::default();
        cfg.entries.insert(
            "my-agent".to_string(),
            AgentEntry {
                command: Some("node".to_string()),
                args: vec!["index.js".to_string(), "--acp".to_string()],
                ..AgentEntry::default()
            },
        );
        let session = resolve_session("my-agent", &cfg).expect("custom resolves");
        assert_eq!(session.program(), "node");
        assert_eq!(
            session.args(),
            vec!["index.js".to_string(), "--acp".to_string()]
        );
    }

    #[test]
    fn resolve_session_errors_for_custom_agent_without_command() {
        let mut cfg = AgentsConfig::default();
        cfg.entries
            .insert("my-agent".to_string(), AgentEntry::default());
        let error = resolve_session("my-agent", &cfg)
            .err()
            .expect("expected an error");
        assert!(error.contains("no command"));
    }

    fn sample_agents_config() -> AgentsConfig {
        let mut env = BTreeMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "qux".to_string());
        let mut cfg = AgentsConfig {
            debug: true,
            ..AgentsConfig::default()
        };
        cfg.entries.insert(
            "copilot".to_string(),
            AgentEntry {
                command: Some("/opt/copilot/bin/copilot".to_string()),
                args: vec!["--acp".to_string()],
                ..AgentEntry::default()
            },
        );
        cfg.entries.insert(
            "my-agent".to_string(),
            AgentEntry {
                command: Some("node".to_string()),
                args: vec!["index.js".to_string(), "--acp".to_string()],
                env,
                display_name: Some("My Agent".to_string()),
                enabled: false,
            },
        );
        cfg
    }

    #[test]
    fn agents_config_dto_round_trips_without_a_filesystem() {
        let cfg = sample_agents_config();
        let dto = agents_config_to_dto(&cfg);

        // Entries are projected in id order (the BTreeMap is sorted).
        assert!(dto.debug);
        let ids: Vec<&str> = dto.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["copilot", "my-agent"]);

        // The disabled custom agent survives the projection verbatim.
        let custom = dto
            .entries
            .iter()
            .find(|e| e.id == "my-agent")
            .expect("custom entry present");
        assert_eq!(custom.display_name.as_deref(), Some("My Agent"));
        assert!(!custom.enabled);
        assert_eq!(
            custom.env,
            vec![
                ("BAZ".to_string(), "qux".to_string()),
                ("FOO".to_string(), "bar".to_string()),
            ]
        );

        let back = dto_to_agents_config(dto);
        assert_eq!(back, cfg);
    }

    #[test]
    fn dto_to_agents_config_skips_blank_ids() {
        let dto = AgentsConfigDto {
            debug: false,
            entries: vec![
                AgentEntryDto {
                    id: "   ".to_string(),
                    command: Some("ignored".to_string()),
                    args: vec![],
                    env: vec![],
                    display_name: None,
                    enabled: true,
                },
                AgentEntryDto {
                    id: "real".to_string(),
                    command: Some("real-acp".to_string()),
                    args: vec!["--acp".to_string()],
                    env: vec![],
                    display_name: None,
                    enabled: true,
                },
            ],
        };

        let cfg = dto_to_agents_config(dto);
        assert_eq!(cfg.entries.len(), 1);
        assert!(cfg.entries.contains_key("real"));
    }

    #[test]
    fn mcp_config_dto_round_trips_preserving_order_and_fields() {
        let cfg = McpConfig {
            servers: vec![
                McpServerEntry {
                    id: "filesystem".to_string(),
                    command: Some("npx".to_string()),
                    args: vec!["-y".to_string(), "server-fs".to_string()],
                    env: BTreeMap::from([("TOKEN".to_string(), "secret".to_string())]),
                    display_name: Some("Files".to_string()),
                    ..Default::default()
                },
                McpServerEntry {
                    id: "remote".to_string(),
                    url: Some("https://tools.example.com/mcp".to_string()),
                    enabled: false,
                    ..Default::default()
                },
            ],
            companion_memory: CompanionMemoryConfig::default(),
        };

        let dto = mcp_config_to_dto(&cfg);
        let ids: Vec<&str> = dto.servers.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["filesystem", "remote"]);
        assert_eq!(
            dto.servers[0].env,
            vec![("TOKEN".to_string(), "secret".to_string())]
        );
        assert!(!dto.servers[1].enabled);

        let back = dto_to_mcp_config(dto, &cfg);
        assert_eq!(back, cfg);
    }

    #[test]
    fn dto_to_mcp_config_skips_blank_ids() {
        let dto = McpConfigDto {
            servers: vec![
                McpServerDto {
                    id: "  ".to_string(),
                    command: Some("ignored".to_string()),
                    args: vec![],
                    env: vec![],
                    url: None,
                    headers: vec![],
                    display_name: None,
                    enabled: true,
                },
                McpServerDto {
                    id: "keep".to_string(),
                    command: Some("server".to_string()),
                    args: vec![],
                    env: vec![],
                    url: None,
                    headers: vec![],
                    display_name: None,
                    enabled: true,
                },
            ],
            companion_memory: CompanionMemoryDto::default(),
        };

        let cfg = dto_to_mcp_config(dto, &McpConfig::default());
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].id, "keep");
    }

    #[test]
    fn mcp_config_to_dto_redacts_header_values() {
        let cfg = McpConfig {
            servers: vec![McpServerEntry {
                id: "workiq".to_string(),
                url: Some("https://workiq.example.com/mcp".to_string()),
                headers: BTreeMap::from([(
                    "Authorization".to_string(),
                    "Bearer super-secret".to_string(),
                )]),
                ..Default::default()
            }],
            companion_memory: CompanionMemoryConfig::default(),
        };

        let dto = mcp_config_to_dto(&cfg);
        let headers = &dto.servers[0].headers;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "Authorization");
        assert_eq!(headers[0].value, None);
        assert!(headers[0].has_value);

        // Non-leakage: the serialized wire payload never carries the value.
        let json = serde_json::to_string(&dto).unwrap();
        assert!(
            !json.contains("super-secret"),
            "header value leaked: {json}"
        );
    }

    #[test]
    fn dto_to_mcp_config_merge_preserves_redacted_header_values() {
        let previous = McpConfig {
            servers: vec![McpServerEntry {
                id: "workiq".to_string(),
                url: Some("https://workiq.example.com/mcp".to_string()),
                headers: BTreeMap::from([(
                    "Authorization".to_string(),
                    "Bearer stored".to_string(),
                )]),
                ..Default::default()
            }],
            companion_memory: CompanionMemoryConfig::default(),
        };

        // A UI save round-trips the redacted view (value: None) untouched and
        // adds a new header with an explicit value.
        let mut dto = mcp_config_to_dto(&previous);
        dto.servers[0].headers.push(McpHeaderDto {
            name: "X-New".to_string(),
            value: Some("fresh".to_string()),
            has_value: false,
        });

        let merged = dto_to_mcp_config(dto, &previous);
        let headers = &merged.servers[0].headers;
        // The redacted Authorization value is preserved from the stored config,
        // not wiped by the save.
        assert_eq!(headers["Authorization"], "Bearer stored");
        assert_eq!(headers["X-New"], "fresh");
    }

    #[test]
    fn dto_to_mcp_config_overwrites_a_header_when_a_value_is_supplied() {
        let previous = McpConfig {
            servers: vec![McpServerEntry {
                id: "workiq".to_string(),
                url: Some("https://workiq.example.com/mcp".to_string()),
                headers: BTreeMap::from([("Authorization".to_string(), "Bearer old".to_string())]),
                ..Default::default()
            }],
            companion_memory: CompanionMemoryConfig::default(),
        };

        let mut dto = mcp_config_to_dto(&previous);
        dto.servers[0].headers[0].value = Some("Bearer new".to_string());

        let merged = dto_to_mcp_config(dto, &previous);
        assert_eq!(merged.servers[0].headers["Authorization"], "Bearer new");
    }

    #[test]
    fn dto_to_mcp_config_drops_headers_with_no_value_anywhere() {
        let dto = McpConfigDto {
            servers: vec![McpServerDto {
                id: "workiq".to_string(),
                command: None,
                args: vec![],
                env: vec![],
                url: Some("https://workiq.example.com/mcp".to_string()),
                headers: vec![
                    // A draft row the user never filled in: no inbound value and
                    // nothing stored to preserve.
                    McpHeaderDto {
                        name: "Authorization".to_string(),
                        value: None,
                        has_value: false,
                    },
                    // Blank names are skipped outright.
                    McpHeaderDto {
                        name: "   ".to_string(),
                        value: Some("x".to_string()),
                        has_value: false,
                    },
                ],
                display_name: None,
                enabled: true,
            }],
            companion_memory: CompanionMemoryDto::default(),
        };

        let merged = dto_to_mcp_config(dto, &McpConfig::default());
        assert!(merged.servers[0].headers.is_empty());
    }

    #[test]
    fn mcp_config_dto_round_trips_companion_memory() {
        let dto = McpConfigDto {
            servers: vec![],
            companion_memory: CompanionMemoryDto {
                enabled: true,
                server_id: Some("memory-host".to_string()),
                vault: Some("memory".to_string()),
                read_only: false,
            },
        };

        let cfg = dto_to_mcp_config(dto.clone(), &McpConfig::default());
        assert!(cfg.companion_memory.enabled);
        assert_eq!(
            cfg.companion_memory.server_id.as_deref(),
            Some("memory-host")
        );
        assert_eq!(cfg.companion_memory.vault.as_deref(), Some("memory"));
        assert!(!cfg.companion_memory.read_only);
        assert_eq!(mcp_config_to_dto(&cfg), dto);
    }

    fn session_opts(vault: &str, read_only: bool) -> StartSessionOptions {
        StartSessionOptions {
            vault: vault.to_string(),
            agent: "copilot".to_string(),
            read_only,
            break_glass: false,
            persisted_grants: Vec::new(),
            preamble: None,
            resume_acp_session_id: None,
        }
    }

    #[test]
    fn http_binding_uses_the_read_write_endpoint_for_a_read_write_scope() {
        let binding = http_binding("http://127.0.0.1:27183", &session_opts("work", false));
        assert_eq!(binding.name(), "notesmith-work");
        match binding {
            McpBinding::Http { url, read_only, .. } => {
                assert_eq!(url, "http://127.0.0.1:27183/mcp/work");
                assert!(!read_only);
            }
            other => panic!("expected an http binding, got {other:?}"),
        }
    }

    #[test]
    fn http_binding_uses_the_read_only_endpoint_and_trims_a_trailing_slash() {
        let binding = http_binding("https://notes.example.com/", &session_opts("journal", true));
        assert_eq!(binding.name(), "notesmith-journal");
        match binding {
            McpBinding::Http { url, read_only, .. } => {
                assert_eq!(url, "https://notes.example.com/mcp-ro/journal");
                assert!(read_only);
            }
            other => panic!("expected an http binding, got {other:?}"),
        }
    }

    #[test]
    fn companion_binding_uses_its_own_server_and_access_mode() {
        let binding = companion_http_binding(
            "https://memory.example.com/",
            "memory-host",
            "memory",
            false,
        );
        assert_eq!(binding.name(), "notesmith--companion-memory-memory-host");
        match binding {
            McpBinding::Http { url, read_only, .. } => {
                assert_eq!(url, "https://memory.example.com/mcp/memory");
                assert!(!read_only);
            }
            other => panic!("expected an http binding, got {other:?}"),
        }
    }

    #[test]
    fn companion_binding_uses_read_only_scope_when_requested() {
        let binding =
            companion_http_binding("https://memory.example.com/", "memory-host", "memory", true);
        match binding {
            McpBinding::Http { url, read_only, .. } => {
                assert_eq!(url, "https://memory.example.com/mcp-ro/memory");
                assert!(read_only);
            }
            other => panic!("expected an http binding, got {other:?}"),
        }
    }

    #[test]
    fn companion_binding_names_cannot_collide_with_active_vault_bindings() {
        let active = McpBinding::http(
            notesmith_agent::server_name_for_vault("work-memory-host"),
            "https://active.example.com/mcp/work-memory-host",
        );
        let companion =
            companion_http_binding("https://memory.example.com/", "memory-host", "work", false);

        assert_ne!(active.name(), companion.name());
        assert_eq!(companion.name(), "notesmith--companion-work-memory-host");
    }

    #[test]
    fn dedupe_skips_companion_when_it_matches_the_active_binding() {
        let active = McpBinding::http("notesmith-work", "https://active.example.com/mcp/work");
        assert!(dedupe_companion_binding(Some(&active), Some(active.clone())).is_none());
    }

    #[test]
    fn dedupe_skips_url_equivalent_companion_even_with_a_distinct_name() {
        let active = McpBinding::http("notesmith-work", "https://active.example.com/mcp/work");
        let companion =
            companion_http_binding("https://active.example.com/", "memory", "work", true);

        assert!(dedupe_companion_binding(Some(&active), Some(companion)).is_none());
    }

    #[test]
    fn dedupe_keeps_distinct_companion_bindings() {
        let active = McpBinding::http("notesmith-work", "https://active.example.com/mcp/work");
        let companion = McpBinding::http(
            "notesmith--companion-memory-memory-host",
            "https://memory.example.com/mcp-ro/memory",
        );
        assert_eq!(
            dedupe_companion_binding(Some(&active), Some(companion.clone())),
            Some(companion)
        );
    }

    #[test]
    fn dedupe_renames_companion_when_a_name_collision_slips_through() {
        let active = McpBinding::http("notesmith-work", "https://active.example.com/mcp/work");
        let companion = McpBinding::http("notesmith-work", "https://memory.example.com/mcp/memory");

        let companion = dedupe_companion_binding(Some(&active), Some(companion))
            .expect("distinct URLs must keep the companion binding");
        assert_ne!(companion.name(), active.name());
        assert_eq!(companion.name(), "notesmith-work-companion");
    }

    fn cached_server_vaults(
        status: notesmith_tauri::vault_cache::VaultListStatus,
        vaults: &[&str],
        last_seen: Option<u64>,
    ) -> notesmith_tauri::vault_cache::ServerVaults {
        notesmith_tauri::vault_cache::ServerVaults {
            vaults: vaults.iter().map(|vault| (*vault).to_string()).collect(),
            last_seen: last_seen.map(|secs| {
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
            }),
            status,
        }
    }

    #[test]
    fn companion_cache_refreshes_selected_server_on_cold_start() {
        use std::cell::Cell;

        let refresh_calls = Cell::new(0);
        let entry = resolve_companion_cache_entry(None, "Memory Server", "memory", || {
            refresh_calls.set(refresh_calls.get() + 1);
            Some(cached_server_vaults(
                notesmith_tauri::vault_cache::VaultListStatus::Fresh,
                &["memory", "work"],
                Some(10),
            ))
        })
        .expect("refresh should load the companion vault");

        assert_eq!(refresh_calls.get(), 1);
        assert_eq!(entry.vaults, vec!["memory".to_string(), "work".to_string()]);
    }

    #[test]
    fn companion_cache_uses_populated_fast_path_without_refresh() {
        use std::cell::Cell;

        let refresh_calls = Cell::new(0);
        let entry = resolve_companion_cache_entry(
            Some(cached_server_vaults(
                notesmith_tauri::vault_cache::VaultListStatus::Fresh,
                &["memory"],
                Some(10),
            )),
            "Memory Server",
            "memory",
            || {
                refresh_calls.set(refresh_calls.get() + 1);
                Some(cached_server_vaults(
                    notesmith_tauri::vault_cache::VaultListStatus::Fresh,
                    &["other"],
                    Some(20),
                ))
            },
        )
        .expect("warm cache should validate directly");

        assert_eq!(refresh_calls.get(), 0);
        assert_eq!(entry.vaults, vec!["memory".to_string()]);
    }

    #[test]
    fn companion_cache_refreshes_stale_uninitialized_entries() {
        use std::cell::Cell;

        let refresh_calls = Cell::new(0);
        let entry = resolve_companion_cache_entry(
            Some(cached_server_vaults(
                notesmith_tauri::vault_cache::VaultListStatus::Stale,
                &[],
                None,
            )),
            "Memory Server",
            "memory",
            || {
                refresh_calls.set(refresh_calls.get() + 1);
                Some(cached_server_vaults(
                    notesmith_tauri::vault_cache::VaultListStatus::Fresh,
                    &["memory"],
                    Some(20),
                ))
            },
        )
        .expect("stale uninitialized cache should refresh");

        assert_eq!(refresh_calls.get(), 1);
        assert_eq!(entry.vaults, vec!["memory".to_string()]);
    }

    #[test]
    fn companion_cache_reports_auth_failures_after_refresh() {
        let error = resolve_companion_cache_entry(None, "Memory Server", "memory", || {
            Some(cached_server_vaults(
                notesmith_tauri::vault_cache::VaultListStatus::AuthError,
                &[],
                None,
            ))
        })
        .expect_err("auth failures should surface");

        assert_eq!(
            error,
            "Companion memory server 'Memory Server' rejected the saved credentials. Update the connection token in Settings → Connection and try again."
        );
    }

    #[test]
    fn companion_cache_reports_unreachable_servers_after_refresh() {
        let error = resolve_companion_cache_entry(None, "Memory Server", "memory", || {
            Some(cached_server_vaults(
                notesmith_tauri::vault_cache::VaultListStatus::Unreachable,
                &[],
                None,
            ))
        })
        .expect_err("transport failures should surface");

        assert_eq!(
            error,
            "Companion memory server 'Memory Server' is unreachable. Refresh saved server vaults in Settings → Connection and try again."
        );
    }

    #[test]
    fn companion_cache_reports_missing_vaults_after_refresh() {
        let error = resolve_companion_cache_entry(None, "Memory Server", "memory", || {
            Some(cached_server_vaults(
                notesmith_tauri::vault_cache::VaultListStatus::Fresh,
                &["other"],
                Some(10),
            ))
        })
        .expect_err("missing vaults should surface");

        assert_eq!(
            error,
            "Companion memory vault 'memory' was not found on 'Memory Server'. Refresh saved server vaults in Settings → Connection and try again."
        );
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "notesmith-agent-pkg-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_package(node_modules: &std::path::Path, pkg: &str) {
        let dir = node_modules.join(pkg);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
    }

    #[test]
    fn package_in_node_modules_detects_a_scoped_package() {
        let root = unique_temp_dir("scoped");
        let node_modules = root.join("node_modules");
        write_package(&node_modules, "@zed-industries/claude-code-acp");

        assert!(package_in_node_modules(
            &node_modules,
            "@zed-industries/claude-code-acp"
        ));
        assert!(!package_in_node_modules(
            &node_modules,
            "@zed-industries/other"
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn npx_cache_finds_an_on_demand_installed_package() {
        let cache = unique_temp_dir("npx-cache");
        let node_modules = cache.join("_npx").join("abc123hash").join("node_modules");
        write_package(&node_modules, "@zed-industries/claude-code-acp");

        assert!(npx_cache_has_package(
            &cache,
            "@zed-industries/claude-code-acp"
        ));
        assert!(!npx_cache_has_package(&cache, "@zed-industries/missing"));
        std::fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn npx_cache_missing_directory_is_not_an_error() {
        let cache = unique_temp_dir("npx-empty");
        assert!(!npx_cache_has_package(
            &cache,
            "@zed-industries/claude-code-acp"
        ));
        std::fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn package_available_is_always_true_without_a_gate() {
        assert!(package_available(None));
    }

    #[test]
    fn availability_gate_applies_only_to_the_default_npx_launcher() {
        // The npx-wrapped Claude adapter carries a package gate at its default
        // launcher (issue #241).
        assert_eq!(
            availability_package_for("claude", "npx").as_deref(),
            Some(notesmith_agent::CLAUDE_ACP_PACKAGE)
        );
        // A user command override points at a real binary — no package gate.
        assert_eq!(
            availability_package_for("claude", "/usr/local/bin/claude-code-acp"),
            None
        );
        // Agents that launch directly from a binary never carry a gate.
        assert_eq!(availability_package_for("copilot", "copilot"), None);
        assert_eq!(availability_package_for("codex", "codex"), None);
        // Unknown / custom ids have no descriptor and therefore no gate.
        assert_eq!(availability_package_for("my-agent", "my-agent"), None);
    }
}
