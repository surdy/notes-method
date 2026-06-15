//! Agent Client Protocol (ACP) transport (ADR 0012).
//!
//! ACP is a JSON-RPC 2.0 protocol spoken over a child process's stdio, framed
//! as newline-delimited JSON. This module drives any ACP-speaking agent through
//! the official Zed [`agent_client_protocol`] crate: the crate owns the wire
//! protocol (framing, request/response correlation, dispatch), and this module
//! only maps the protocol's typed messages onto Notesmith's normalized
//! [`AgentEvent`] stream and answers the agent's inbound callbacks.
//!
//! The session lifecycle is:
//!
//! 1. `initialize` — negotiate the protocol version and advertise client
//!    capabilities (filesystem / terminal are advertised only when the opt-in
//!    local-I/O break-glass is on; Phase 4).
//! 2. `session/new` — open a session carrying the absolute working directory
//!    (`cwd`) and the per-vault MCP servers (`mcpServers`); the agent replies
//!    with a `sessionId`.
//! 3. `session/prompt` — one per user turn. The agent streams `session/update`
//!    notifications (assistant text chunks, tool calls/updates) and finally
//!    answers the request with a `stopReason`.
//!
//! While a prompt is in flight the agent may call back with
//! `session/request_permission`; the session answers it from the read-only /
//! read-write scope (read-only rejects, read-write approves) — the full
//! per-write prompt flow lands in Phase 4.
//!
//! Per ADR 0009 the mapping is tolerant: unrecognized updates are ignored and a
//! malformed turn becomes an [`AgentEvent::Error`] on the stream rather than a
//! panic.
//!
//! The public [`AcpSession`] keeps a pull-based shape (`send` / `next_event`):
//! the crate's connection lifecycle is owned by a background driver task, and
//! user messages and normalized events are bridged in and out over channels.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::{
    ClientCapabilities, ContentBlock, CreateTerminalRequest, EnvVariable, FileSystemCapabilities,
    Implementation, InitializeRequest, KillTerminalRequest, McpServer, McpServerStdio,
    NewSessionRequest, PermissionOption, PermissionOptionKind, PromptRequest, ProtocolVersion,
    ReadTextFileRequest, ReleaseTerminalRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest,
    TerminalOutputRequest, TextContent, ToolCallContent, ToolCallStatus,
    WaitForTerminalExitRequest, WaitForTerminalExitResponse, WriteTextFileRequest,
};
use agent_client_protocol::{AcpAgent, Client};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::acp_client::{LocalIoHandler, await_exit};
use crate::context::{EditorContext, VaultSummary, assemble_preamble};
use crate::error::AgentError;
use crate::event::{AgentEvent, ToolCall, ToolResult};
use crate::mcp::McpBinding;
use crate::model::{ModelPicker, parse_model_picker};
use crate::permission::{
    DenyAll, PermissionDecider, PermissionRequest, PermissionState, resolve_permission,
};
use crate::session::AgentSession;

/// Client name advertised to the agent during `initialize`.
const CLIENT_NAME: &str = "notesmith";

/// Name surfaced to the agent's process table for the spawned transport.
const AGENT_SERVER_NAME: &str = "notesmith-agent";

/// Default binary that speaks ACP natively (GitHub Copilot CLI).
pub const DEFAULT_COPILOT_BIN: &str = "copilot";

/// npm package providing the Claude Code ACP adapter (run via `npx`).
pub const CLAUDE_ACP_PACKAGE: &str = "@zed-industries/claude-code-acp";

/// Default binary providing the Codex ACP adapter.
pub const DEFAULT_CODEX_ACP_BIN: &str = "codex-acp";

/// Default binary for the Gemini CLI, which speaks ACP behind
/// `--experimental-acp`.
pub const DEFAULT_GEMINI_BIN: &str = "gemini";

/// The result the driver reports back to the first [`AcpSession::send`] so a
/// spawn/handshake failure surfaces as a synchronous error rather than only on
/// the event stream.
type HandshakeResult = Result<(), String>;

/// A one-shot slot used to report the handshake outcome exactly once, whether
/// it completes inside the connection's `main_fn` or the transport fails to
/// spawn before `main_fn` ever runs.
type ReadySlot = Mutex<Option<oneshot::Sender<HandshakeResult>>>;

/// Build a one-time context preamble steering the agent to the Notesmith MCP
/// tools. It is prepended to the first prompt of a session so the agent prefers
/// vault-aware tools over guessing at the filesystem.
///
/// The wording adapts to the session: whether the vault MCP endpoint is wired,
/// and whether the agent also has scoped local filesystem/terminal access.
pub(crate) fn session_preamble(has_mcp: bool, local_io: bool) -> String {
    let mut text = String::from(
        "You are an assistant operating inside a Notesmith vault (a directory of \
         Markdown notes).",
    );
    if has_mcp {
        text.push_str(
            " To read, search, or modify notes, use the Notesmith MCP tools \
             (for example `search_notes`, `get_note`, `list_notes`, `query_sql`, \
             and `list_tasks`); they are vault-aware and respect the vault's \
             indexes and read-only scope.",
        );
    }
    if local_io {
        text.push_str(
            " You also have scoped filesystem and terminal access to this vault \
             directory for anything the Notesmith tools do not cover; prefer the \
             Notesmith tools when they apply.",
        );
    } else {
        text.push_str(
            " You do NOT have shell or direct filesystem access in this session, \
             so do not attempt to run shell commands or read files from disk \
             directly — use the Notesmith tools instead.",
        );
    }
    text
}

/// Advertise the filesystem/terminal client capabilities for the break-glass
/// matrix (ADR 0012, Decisions 7–8):
///
/// - break-glass **off** (`local_io = false`, the default): advertise neither —
///   the agent reaches the vault through the Notesmith MCP tools only;
/// - break-glass **on**, read-write: advertise `fs/read` + `fs/write` +
///   `terminal`, all scoped to the vault directory;
/// - break-glass **on**, read-only: advertise `fs/read` only — writes and the
///   terminal stay blocked, matching the read-only scope.
fn initialize_request(local_io: bool, read_only: bool) -> InitializeRequest {
    let read = local_io;
    let write = local_io && !read_only;
    let terminal = local_io && !read_only;
    InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(
            ClientCapabilities::new()
                .fs(FileSystemCapabilities::new()
                    .read_text_file(read)
                    .write_text_file(write))
                .terminal(terminal),
        )
        .client_info(Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")))
}

/// Build the `session/new` request for `cwd`, wiring the active vault's MCP
/// server (when present) into the `mcpServers` array. The binding selects the
/// transport: a stdio bridge subprocess for local sessions, or an HTTP(S)
/// endpoint for remote daemons.
fn new_session_request(cwd: &str, mcp: Option<&McpBinding>) -> NewSessionRequest {
    let servers: Vec<McpServer> = mcp.map(|m| vec![m.to_mcp_server()]).unwrap_or_default();
    NewSessionRequest::new(PathBuf::from(cwd)).mcp_servers(servers)
}

/// Choose the MCP binding to advertise based on the agent's declared transport
/// support. The `primary` binding (typically the daemon's HTTP endpoint) is
/// used whenever the agent supports HTTP MCP; otherwise the stdio `fallback`
/// (a local-daemon bridge) is used. When neither applies the primary is kept so
/// behaviour degrades to "advertise the binding we have".
fn select_mcp_binding<'a>(
    agent_supports_http: bool,
    primary: Option<&'a McpBinding>,
    fallback: Option<&'a McpBinding>,
) -> Option<&'a McpBinding> {
    if agent_supports_http {
        primary.or(fallback)
    } else {
        fallback.or(primary)
    }
}

/// Build the `session/prompt` content blocks for a single user turn. Blocks are
/// ordered context-first so the agent reads steering before the user's words:
///
/// 1. the one-time session `preamble` (skill.md + vault summary), on the first
///    turn only;
/// 2. the per-turn `editor` context block (active note, selection, tabs), on
///    every turn it is present; then
/// 3. the user's message.
fn prompt_blocks(preamble: Option<&str>, editor: Option<&str>, text: &str) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    if let Some(preamble) = preamble {
        blocks.push(ContentBlock::Text(TextContent::new(preamble)));
    }
    if let Some(editor) = editor {
        blocks.push(ContentBlock::Text(TextContent::new(editor)));
    }
    blocks.push(ContentBlock::Text(TextContent::new(text)));
    blocks
}

/// Extract the text of a content block, if it is a text block.
fn text_of(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

/// Concatenate the text of a tool-call update's content blocks.
fn tool_update_text(blocks: &[ToolCallContent]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ToolCallContent::Content(content) => text_of(&content.content),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Map a `session/update` notification to normalized events.
///
/// Recognized updates:
/// - `agent_message_chunk` — a text block becomes an
///   [`AgentEvent::AgentMessageDelta`].
/// - `tool_call` — becomes an [`AgentEvent::ToolCall`].
/// - `tool_call_update` — a terminal status (`completed`/`failed`) becomes an
///   [`AgentEvent::ToolResult`]; non-terminal updates are ignored.
///
/// Any other update (thoughts, plans, commands, config, usage) yields no event.
fn map_session_update(update: SessionUpdate) -> Vec<AgentEvent> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => text_of(&chunk.content)
            .map(|text| vec![AgentEvent::AgentMessageDelta { text }])
            .unwrap_or_default(),
        SessionUpdate::ToolCall(call) => {
            let name = if call.title.is_empty() {
                "tool".to_string()
            } else {
                call.title
            };
            vec![AgentEvent::ToolCall(ToolCall {
                id: Some(call.tool_call_id.to_string()),
                name,
                args: call.raw_input.unwrap_or_else(|| json!({})),
            })]
        }
        SessionUpdate::ToolCallUpdate(update) => match update.fields.status {
            Some(ToolCallStatus::Completed) | Some(ToolCallStatus::Failed) => {
                let is_error = update.fields.status == Some(ToolCallStatus::Failed);
                let content = update
                    .fields
                    .content
                    .as_deref()
                    .map(tool_update_text)
                    .unwrap_or_default();
                vec![AgentEvent::ToolResult(ToolResult {
                    id: Some(update.tool_call_id.to_string()),
                    content,
                    is_error,
                })]
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Decide how to answer a `session/request_permission` callback given the
/// resolved allow/deny outcome. `allow` selects an allow option (preferring the
/// "always" polarity when `prefer_always`, otherwise "once"); a deny selects a
/// reject option, preferring "once". Falls back to cancelling when the agent
/// offered no matching option.
pub(crate) fn select_permission_option(
    options: &[PermissionOption],
    allow: bool,
    prefer_always: bool,
) -> RequestPermissionOutcome {
    let order: [PermissionOptionKind; 2] = match (allow, prefer_always) {
        (true, true) => [
            PermissionOptionKind::AllowAlways,
            PermissionOptionKind::AllowOnce,
        ],
        (true, false) => [
            PermissionOptionKind::AllowOnce,
            PermissionOptionKind::AllowAlways,
        ],
        (false, _) => [
            PermissionOptionKind::RejectOnce,
            PermissionOptionKind::RejectAlways,
        ],
    };
    let pick = order
        .iter()
        .find_map(|kind| options.iter().find(|option| option.kind == *kind));
    match pick {
        Some(option) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
            option.option_id.clone(),
        )),
        None => RequestPermissionOutcome::Cancelled,
    }
}

/// Extract the prompt context (tool name + kind) from an inbound
/// `session/request_permission` request. The tool name is the per-tool "allow
/// always" key, so a missing/empty title falls back to the tool kind and then a
/// stable placeholder.
fn permission_request_info(req: &RequestPermissionRequest) -> PermissionRequest {
    let kind = req.tool_call.fields.kind.as_ref().map(|kind| {
        serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "other".to_string())
    });
    let tool = req
        .tool_call
        .fields
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or_else(|| kind.clone())
        .unwrap_or_else(|| "tool".to_string());
    PermissionRequest { tool, kind }
}

/// Answer a `session/request_permission` callback using the session permission
/// policy: read-only hard-denies; an already-granted tool is allowed silently;
/// anything else is delegated to the `decider` (allow once / allow always /
/// deny), with "allow always" remembered on `state` for the rest of the
/// session.
async fn answer_permission(
    req: &RequestPermissionRequest,
    read_only: bool,
    state: &PermissionState,
    decider: &Arc<dyn PermissionDecider>,
) -> RequestPermissionOutcome {
    let info = permission_request_info(req);
    let already_always = !read_only && state.is_always(&info.tool);
    let decision = if read_only || already_always {
        None
    } else {
        Some(decider.decide(info.clone()).await)
    };
    let resolution = resolve_permission(read_only, already_always, decision);
    if resolution.remember {
        state.remember(&info.tool);
    }
    select_permission_option(&req.options, resolution.allow, resolution.remember)
}

/// Whether an MCP endpoint URL denotes the read-only scope (`/mcp-ro/`).
///
/// The daemon encodes scope in the path; an absent endpoint defaults to
/// read-only so the permission gate stays safe by default.
pub fn mcp_url_is_read_only(mcp_url: Option<&str>) -> bool {
    match mcp_url {
        Some(url) if !url.is_empty() => url.contains("/mcp-ro/"),
        _ => true,
    }
}

/// Report the handshake outcome through the ready slot, at most once.
fn signal_ready(slot: &ReadySlot, result: HandshakeResult) {
    if let Ok(mut guard) = slot.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(result);
        }
    }
}

/// One queued user turn: the message plus the editor context captured for it.
/// The context is carried per turn (rather than stashed on the session) so each
/// prompt reflects the editor state at the moment it was sent.
struct PromptTurn {
    message: String,
    editor: Option<EditorContext>,
}

/// A command sent from the public [`AcpSession`] handle to the driver task that
/// owns the ACP connection. Prompts and model changes share one ordered channel
/// so they are applied in the order the caller issued them.
enum DriverCommand {
    /// Send a user turn (`session/prompt`).
    Prompt(PromptTurn),
    /// Apply a model selection, replying once the agent confirms (or rejects).
    SetModel {
        value: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

/// Shared slot holding the model picker parsed from the `session/new` result,
/// populated by the driver after the handshake and read by the public handle.
type ModelSlot = Arc<Mutex<Option<ModelPicker>>>;

/// An [`AgentSession`] driven over the Agent Client Protocol.
///
/// The child process is spawned lazily on the first [`send`](AgentSession::send),
/// which runs the `initialize` + `session/new` handshake and then issues the
/// first `session/prompt`. Subsequent sends reuse the same session id, so ACP
/// sessions are **multi-turn**.
pub struct AcpSession {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    working_dir: Option<PathBuf>,
    mcp: Option<McpBinding>,
    /// Stdio bridge used only when the agent does not advertise HTTP MCP
    /// support (a local-daemon fallback for HTTP-incapable agents).
    mcp_stdio_fallback: Option<McpBinding>,
    read_only: bool,
    local_io: bool,
    pub(crate) setup_hint: Option<String>,
    skill: Option<String>,
    vault_summary: Option<VaultSummary>,
    decider: Arc<dyn PermissionDecider>,
    permission_state: PermissionState,
    model_picker: ModelSlot,
    started: bool,
    user_tx: Option<mpsc::UnboundedSender<DriverCommand>>,
    event_rx: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    driver: Option<JoinHandle<()>>,
}

impl AcpSession {
    /// Build an ACP session that launches `program` with `args`.
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            env: Vec::new(),
            working_dir: None,
            mcp: None,
            mcp_stdio_fallback: None,
            read_only: true,
            local_io: false,
            setup_hint: None,
            skill: None,
            vault_summary: None,
            decider: Arc::new(DenyAll),
            permission_state: PermissionState::new(),
            model_picker: Arc::new(Mutex::new(None)),
            started: false,
            user_tx: None,
            event_rx: None,
            driver: None,
        }
    }

    /// Build an ACP session for the GitHub Copilot CLI (`copilot --acp`).
    ///
    /// `bin` overrides the binary (path or name); `None` uses
    /// [`DEFAULT_COPILOT_BIN`].
    pub fn copilot(bin: Option<&str>) -> Self {
        let program = bin.filter(|b| !b.is_empty()).unwrap_or(DEFAULT_COPILOT_BIN);
        Self::new(program, vec!["--acp".to_string()])
    }

    /// Build an ACP session for Claude Code via its ACP adapter.
    ///
    /// Claude Code does not speak ACP natively; the adapter
    /// [`CLAUDE_ACP_PACKAGE`] is run through `npx`. `bin` overrides the launcher
    /// with a direct path to an adapter executable (run with no extra args).
    pub fn claude_code(bin: Option<&str>) -> Self {
        let session = match bin.filter(|b| !b.is_empty()) {
            Some(bin) => Self::new(bin, Vec::new()),
            None => Self::new(
                "npx",
                vec!["--yes".to_string(), CLAUDE_ACP_PACKAGE.to_string()],
            ),
        };
        session.with_setup_hint(format!(
            "Claude Code over ACP needs its adapter. Install Node.js and run \
             `npx --yes {CLAUDE_ACP_PACKAGE}` once, or set the agent binary to a \
             `claude-code-acp` executable."
        ))
    }

    /// Build an ACP session for Codex via its ACP adapter binary.
    ///
    /// Codex's native protocol is app-server/proto; the [`DEFAULT_CODEX_ACP_BIN`]
    /// adapter exposes it over ACP. `bin` overrides the adapter binary.
    pub fn codex(bin: Option<&str>) -> Self {
        let program = bin
            .filter(|b| !b.is_empty())
            .unwrap_or(DEFAULT_CODEX_ACP_BIN);
        Self::new(program, Vec::new()).with_setup_hint(format!(
            "Codex over ACP needs the `{DEFAULT_CODEX_ACP_BIN}` adapter on your \
             PATH (see https://github.com/zed-industries/codex-acp)."
        ))
    }

    /// Build an ACP session for the Gemini CLI (`gemini --experimental-acp`).
    ///
    /// The Gemini CLI exposes an ACP server behind its `--experimental-acp`
    /// flag. `bin` overrides the binary (path or name); `None` uses
    /// [`DEFAULT_GEMINI_BIN`].
    pub fn gemini(bin: Option<&str>) -> Self {
        let program = bin.filter(|b| !b.is_empty()).unwrap_or(DEFAULT_GEMINI_BIN);
        Self::new(program, vec!["--experimental-acp".to_string()]).with_setup_hint(
            "Gemini over ACP needs the Gemini CLI on your PATH; launch it with \
             `gemini --experimental-acp` (see https://github.com/google-gemini/gemini-cli).",
        )
    }

    /// Run the agent in `working_dir` (the active vault's directory).
    pub fn in_dir(mut self, working_dir: Option<PathBuf>) -> Self {
        self.working_dir = working_dir;
        self
    }

    /// Grant the agent scoped filesystem and terminal access to the vault
    /// directory (the opt-in local-I/O break-glass, Phase 4). Off by default;
    /// when off the agent reaches the vault through MCP tools only.
    pub fn with_local_io(mut self, enabled: bool) -> Self {
        self.local_io = enabled;
        self
    }

    /// Attach a human-readable setup hint, appended to start/handshake failures
    /// so a missing ACP adapter binary surfaces actionable guidance.
    pub fn with_setup_hint(mut self, hint: impl Into<String>) -> Self {
        self.setup_hint = Some(hint.into());
        self
    }

    /// Inject the vault's `.notesmith/skill.md` contents into the one-time
    /// session preamble (ADR 0012, Decision 11). The caller reads the file
    /// (e.g. via `notesmith skill print`) and passes `None` when it is absent
    /// or unreadable, so a missing skill degrades to a summary-only preamble.
    /// The body is bounded when assembled, keeping the preamble small.
    pub fn with_skill(mut self, skill: Option<String>) -> Self {
        self.skill = skill;
        self
    }

    /// Inject a compact, caller-computed [`VaultSummary`] (name, note count, top
    /// tags/folders) into the one-time session preamble (ADR 0012, Decision 11).
    pub fn with_vault_summary(mut self, summary: VaultSummary) -> Self {
        self.vault_summary = Some(summary);
        self
    }

    /// Auto-wire the active vault's MCP server into `session/new` and derive
    /// the permission scope (read-only vs read-write) from the binding.
    ///
    /// This is the **preferred** binding, used whenever the agent advertises
    /// HTTP MCP support during `initialize`. For a local daemon this is the
    /// daemon's Streamable HTTP endpoint, which every HTTP-capable agent (e.g.
    /// GitHub Copilot, which supports *only* HTTP/SSE) can reach. Pair it with
    /// [`with_mcp_stdio_fallback`](Self::with_mcp_stdio_fallback) so
    /// HTTP-incapable agents still get the vault over a stdio bridge.
    pub fn with_mcp(mut self, binding: McpBinding) -> Self {
        self.read_only = binding.read_only();
        self.mcp = Some(binding);
        self
    }

    /// Provide a stdio MCP bridge used only when the agent does **not**
    /// advertise HTTP MCP support (`mcpCapabilities.http == false`). This is a
    /// local-daemon fallback for agents that speak only stdio MCP; remote
    /// sessions leave it unset and always use the HTTP binding.
    pub fn with_mcp_stdio_fallback(mut self, binding: McpBinding) -> Self {
        self.mcp_stdio_fallback = Some(binding);
        self
    }

    /// Explicitly set the read-only permission scope (overrides the value
    /// derived from the MCP endpoint).
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Inject the decider consulted for write-permission prompts the session
    /// cannot resolve from its own state (read-write writes that are not yet
    /// "allow always"). Defaults to [`DenyAll`] so writes can never slip
    /// through unprompted; the desktop chat UI injects a real prompt (Phase 8).
    pub fn with_permission_decider(mut self, decider: Arc<dyn PermissionDecider>) -> Self {
        self.decider = decider;
        self
    }

    /// Set environment variables applied to the spawned agent process. Used by
    /// the desktop bridge to honor a custom agent's `[agents.<id>].env` block
    /// (ADR 0013, decision 4). Replaces any previously-set env.
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// The program this session launches (path or PATH-resolved name).
    pub fn program(&self) -> &str {
        &self.program
    }

    /// The base launch arguments this session spawns the program with.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// The environment variables applied to the spawned agent process.
    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    /// The setup hint appended to start/handshake failures, when present.
    pub fn setup_hint(&self) -> Option<&str> {
        self.setup_hint.as_deref()
    }

    /// Resolve the session working directory as an **absolute** path. ACP
    /// agents reject relative `cwd` values, so a missing or relative working
    /// directory is resolved against the process's current directory.
    fn absolute_cwd(&self) -> String {
        let base = self
            .working_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let absolute = if base.is_absolute() {
            base
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&base))
                .unwrap_or(base)
        };
        let resolved = std::fs::canonicalize(&absolute).unwrap_or(absolute);
        resolved.to_string_lossy().into_owned()
    }

    /// Build the ACP subprocess transport from the launch table entry.
    fn build_agent(&self) -> AcpAgent {
        let mut stdio =
            McpServerStdio::new(AGENT_SERVER_NAME, self.program.clone()).args(self.args.clone());
        if !self.env.is_empty() {
            stdio = stdio.env(
                self.env
                    .iter()
                    .map(|(name, value)| EnvVariable::new(name.clone(), value.clone()))
                    .collect(),
            );
        }
        AcpAgent::new(McpServer::Stdio(stdio))
    }

    /// Spawn the driver task that owns the ACP connection lifecycle, returning a
    /// receiver that resolves once the `initialize` + `session/new` handshake
    /// has completed (or failed). The driver streams normalized events through
    /// `self.event_rx` and consumes user messages from `self.user_tx`.
    fn start_driver(&mut self) -> oneshot::Receiver<HandshakeResult> {
        let (user_tx, mut user_rx) = mpsc::unbounded_channel::<DriverCommand>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (ready_tx, ready_rx) = oneshot::channel::<HandshakeResult>();

        self.user_tx = Some(user_tx);
        self.event_rx = Some(event_rx);

        let ready: Arc<ReadySlot> = Arc::new(Mutex::new(Some(ready_tx)));
        let ready_main = ready.clone();

        let agent = self.build_agent();
        let cwd = self.absolute_cwd();
        let mcp = self.mcp.clone();
        let mcp_stdio_fallback = self.mcp_stdio_fallback.clone();
        let local_io = self.local_io;
        let read_only = self.read_only;
        let decider = self.decider.clone();
        let permission_state = self.permission_state.clone();
        let model_slot = self.model_picker.clone();
        let io_handler = Arc::new(LocalIoHandler::new(
            local_io,
            read_only,
            PathBuf::from(&cwd),
        ));
        let preamble = assemble_preamble(
            &session_preamble(
                self.mcp.is_some() || self.mcp_stdio_fallback.is_some(),
                self.local_io,
            ),
            self.vault_summary.as_ref(),
            self.skill.as_deref(),
        );

        let notif_event_tx = event_tx.clone();
        let main_event_tx = event_tx.clone();
        let outer_event_tx = event_tx;

        let driver = tokio::spawn(async move {
            // Per-handler clones of the vault-scoped local-I/O handler. The
            // handlers are always registered but report "method not found" when
            // break-glass is off, so the advertised capabilities are the real
            // gate (with these as defense in depth).
            let h_read = io_handler.clone();
            let h_write = io_handler.clone();
            let h_term_create = io_handler.clone();
            let h_term_output = io_handler.clone();
            let h_term_kill = io_handler.clone();
            let h_term_release = io_handler.clone();
            let h_term_wait = io_handler;

            let result = Client
                .builder()
                .name(CLIENT_NAME)
                .on_receive_notification(
                    async move |notif: SessionNotification, _cx| {
                        for event in map_session_update(notif.update) {
                            let _ = notif_event_tx.send(event);
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |req: RequestPermissionRequest, responder, _cx| {
                        let outcome =
                            answer_permission(&req, read_only, &permission_state, &decider).await;
                        responder.respond(RequestPermissionResponse::new(outcome))
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: ReadTextFileRequest, responder, _cx| {
                        responder.respond_with_result(h_read.fs_read(&req))
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: WriteTextFileRequest, responder, _cx| {
                        responder.respond_with_result(h_write.fs_write(&req))
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: CreateTerminalRequest, responder, _cx| {
                        responder.respond_with_result(h_term_create.terminal_create(&req).await)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: TerminalOutputRequest, responder, _cx| {
                        responder.respond_with_result(h_term_output.terminal_output(&req).await)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: KillTerminalRequest, responder, _cx| {
                        responder.respond_with_result(h_term_kill.terminal_kill(&req).await)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: ReleaseTerminalRequest, responder, _cx| {
                        responder.respond_with_result(h_term_release.terminal_release(&req).await)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |req: WaitForTerminalExitRequest, responder, cx| {
                        // The wait runs off the dispatch loop so a long-running
                        // command never blocks the connection.
                        match h_term_wait.terminal_wait_handles(&req).await {
                            Ok((exit, done)) => cx.spawn(async move {
                                let exit = await_exit(exit, done).await;
                                responder.respond(WaitForTerminalExitResponse::new(
                                    exit.to_exit_status(),
                                ))
                            }),
                            Err(error) => responder.respond_with_error(error),
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(agent, async move |cx| {
                    // Handshake. block_task() is safe here: main_fn runs
                    // alongside (not inside) the inbound dispatch loop.
                    let init = match cx
                        .send_request(initialize_request(local_io, read_only))
                        .block_task()
                        .await
                    {
                        Ok(response) => response,
                        Err(error) => {
                            signal_ready(&ready_main, Err(error.to_string()));
                            return Err(error);
                        }
                    };
                    // Pick the MCP transport the agent can actually use. Some
                    // agents (e.g. GitHub Copilot) support only HTTP/SSE MCP and
                    // silently ignore stdio servers, so honour the advertised
                    // `mcpCapabilities` rather than assuming a transport.
                    let agent_supports_http = init.agent_capabilities.mcp_capabilities.http;
                    let chosen_mcp = select_mcp_binding(
                        agent_supports_http,
                        mcp.as_ref(),
                        mcp_stdio_fallback.as_ref(),
                    );
                    let session_id: SessionId = match cx
                        .send_request(new_session_request(&cwd, chosen_mcp))
                        .block_task()
                        .await
                    {
                        Ok(response) => {
                            // Capture the agent's advertised model selector (a
                            // `model` config option, else the deprecated modes)
                            // so the caller can render a picker and apply a
                            // choice. Absent → no picker, no error.
                            if let Some(picker) = parse_model_picker(
                                response.config_options.as_deref(),
                                response.modes.as_ref(),
                            ) {
                                if let Ok(mut slot) = model_slot.lock() {
                                    *slot = Some(picker);
                                }
                            }
                            response.session_id
                        }
                        Err(error) => {
                            signal_ready(&ready_main, Err(error.to_string()));
                            return Err(error);
                        }
                    };
                    signal_ready(&ready_main, Ok(()));

                    // Command loop. Prompts and model changes share one ordered
                    // channel. The Notesmith context preamble is sent once,
                    // ahead of the first user message; each turn also carries
                    // its editor-context block (when present). Assistant deltas
                    // arrive concurrently via the notification handler while
                    // each prompt request awaits its terminal stopReason.
                    let mut preamble = Some(preamble);
                    while let Some(command) = user_rx.recv().await {
                        match command {
                            DriverCommand::Prompt(turn) => {
                                let editor = turn.editor.as_ref().and_then(EditorContext::render);
                                let blocks = prompt_blocks(
                                    preamble.take().as_deref(),
                                    editor.as_deref(),
                                    &turn.message,
                                );
                                match cx
                                    .send_request(PromptRequest::new(session_id.clone(), blocks))
                                    .block_task()
                                    .await
                                {
                                    Ok(_response) => {
                                        let _ =
                                            main_event_tx.send(AgentEvent::Done { result: None });
                                    }
                                    Err(error) => {
                                        let _ = main_event_tx.send(AgentEvent::Error {
                                            message: error.to_string(),
                                        });
                                        break;
                                    }
                                }
                            }
                            DriverCommand::SetModel { value, reply } => {
                                // Validate against the advertised picker and
                                // route to the matching setter. The mutex guard
                                // is dropped before any await (it is not Send).
                                let routing: Result<Option<String>, String> =
                                    match model_slot.lock() {
                                        Ok(slot) => match slot.as_ref() {
                                            None => Err("the agent advertises no model selector"
                                                .to_string()),
                                            Some(picker) if !picker.contains(&value) => {
                                                Err(format!("unknown model `{value}`"))
                                            }
                                            Some(picker) => {
                                                Ok(picker.config_id().map(str::to_string))
                                            }
                                        },
                                        Err(_) => Err("model selector unavailable".to_string()),
                                    };
                                let outcome = match routing {
                                    Err(message) => Err(message),
                                    // `configOptions` selector → set_config_option.
                                    Ok(Some(config_id)) => cx
                                        .send_request(SetSessionConfigOptionRequest::new(
                                            session_id.clone(),
                                            config_id,
                                            value,
                                        ))
                                        .block_task()
                                        .await
                                        .map(|_| ())
                                        .map_err(|error| error.to_string()),
                                    // deprecated `modes` fallback → set_mode.
                                    Ok(None) => cx
                                        .send_request(SetSessionModeRequest::new(
                                            session_id.clone(),
                                            value,
                                        ))
                                        .block_task()
                                        .await
                                        .map(|_| ())
                                        .map_err(|error| error.to_string()),
                                };
                                let _ = reply.send(outcome);
                            }
                        }
                    }
                    Ok(())
                })
                .await;

            // A transport spawn failure surfaces here without ever running
            // main_fn; report it through the (still-armed) ready slot so the
            // first `send` returns a clean error, and onto the event stream for
            // any in-flight `next_event`.
            if let Err(error) = result {
                signal_ready(&ready, Err(error.to_string()));
                let _ = outer_event_tx.send(AgentEvent::Error {
                    message: error.to_string(),
                });
            }
        });

        self.driver = Some(driver);
        ready_rx
    }

    /// Append the setup hint (when set) to a startup/handshake failure so a
    /// missing ACP adapter binary surfaces actionable guidance.
    fn explain(&self, error: AgentError) -> AgentError {
        match &self.setup_hint {
            Some(hint) => AgentError::Protocol(format!("{error} — {hint}")),
            None => error,
        }
    }

    /// Eagerly run the `initialize` + `session/new` handshake without sending a
    /// prompt, so callers can read [`model_picker`](AcpSession::model_picker)
    /// up front (e.g. to populate a picker the moment a chat session opens).
    /// Idempotent: a no-op once the session has started.
    pub async fn start(&mut self) -> Result<(), AgentError> {
        self.ensure_started().await
    }

    /// Lazily start the driver and run the `initialize` + `session/new`
    /// handshake on the first call; subsequent calls are no-ops. After this
    /// resolves `Ok`, the model picker (if any) has been captured.
    async fn ensure_started(&mut self) -> Result<(), AgentError> {
        if self.started {
            return Ok(());
        }
        self.started = true;
        let ready_rx = self.start_driver();
        match ready_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(self.explain(AgentError::Protocol(message))),
            Err(_) => Err(self.explain(AgentError::Protocol(
                "agent connection closed during handshake".to_string(),
            ))),
        }
    }

    /// Send a turn (message + optional editor context), lazily starting the
    /// driver and running the handshake on the first call. Both the plain
    /// [`AgentSession::send`] and [`AcpSession::send_with_context`] funnel
    /// through here so the start-up path is shared.
    async fn deliver(&mut self, turn: PromptTurn) -> Result<(), AgentError> {
        self.ensure_started().await?;
        let user_tx = self
            .user_tx
            .as_ref()
            .ok_or_else(|| AgentError::Protocol("session is not initialized".to_string()))?;
        user_tx
            .send(DriverCommand::Prompt(turn))
            .map_err(|_| AgentError::Protocol("agent connection closed".to_string()))
    }

    /// The model selector the agent advertised for this session, or `None` when
    /// it advertised neither a `model` config option nor session `modes` (ADR
    /// 0012, Decision 12). Available once the session has started; callers
    /// render a picker from it and apply a choice via [`select_model`].
    ///
    /// [`select_model`]: AcpSession::select_model
    pub fn model_picker(&self) -> Option<ModelPicker> {
        self.model_picker.lock().ok().and_then(|slot| slot.clone())
    }

    /// Apply a model selection to the running session, routing to
    /// `session/set_config_option` or the deprecated `session/set_mode`
    /// depending on how the agent advertised its models. Starts the session if
    /// needed, then resolves once the agent confirms (or rejects) the change.
    /// `value` must be one of the advertised option ids.
    pub async fn select_model(&mut self, value: &str) -> Result<(), AgentError> {
        self.ensure_started().await?;
        let (reply_tx, reply_rx) = oneshot::channel();
        let user_tx = self
            .user_tx
            .as_ref()
            .ok_or_else(|| AgentError::Protocol("session is not initialized".to_string()))?;
        user_tx
            .send(DriverCommand::SetModel {
                value: value.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| AgentError::Protocol("agent connection closed".to_string()))?;
        match reply_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(AgentError::Protocol(message)),
            Err(_) => Err(AgentError::Protocol("agent connection closed".to_string())),
        }
    }

    /// Send a turn carrying the desktop editor's current state (active note,
    /// selection, open tabs), injected ahead of the message as a context block
    /// (ADR 0012, Decision 10). An empty context degrades to no block.
    pub async fn send_with_context(
        &mut self,
        message: &str,
        editor: EditorContext,
    ) -> Result<(), AgentError> {
        self.deliver(PromptTurn {
            message: message.to_string(),
            editor: Some(editor),
        })
        .await
    }
}

impl AgentSession for AcpSession {
    async fn send(&mut self, message: &str) -> Result<(), AgentError> {
        self.deliver(PromptTurn {
            message: message.to_string(),
            editor: None,
        })
        .await
    }

    async fn next_event(&mut self) -> Option<AgentEvent> {
        match self.event_rx.as_mut() {
            Some(rx) => rx.recv().await,
            // Before the first send no process exists yet; pend so the runner's
            // `select!` waits for the first user message instead of ending.
            None => std::future::pending().await,
        }
    }
}

impl Drop for AcpSession {
    fn drop(&mut self) {
        if let Some(driver) = self.driver.take() {
            // Aborting the driver drops the ACP connection, which kills the
            // spawned agent process.
            driver.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionDecision;
    use agent_client_protocol::schema::PermissionOptionId;
    use futures::future::BoxFuture;
    use serde_json::Value;

    /// A decider that always returns a fixed decision, for exercising the
    /// permission policy without a UI prompt.
    struct FixedDecider(PermissionDecision);
    impl PermissionDecider for FixedDecider {
        fn decide(&self, _request: PermissionRequest) -> BoxFuture<'static, PermissionDecision> {
            let decision = self.0;
            Box::pin(async move { decision })
        }
    }

    fn update_from(json: Value) -> SessionUpdate {
        serde_json::from_value(json).expect("valid session update")
    }

    #[test]
    fn initialize_request_capability_matrix() {
        // Break-glass off: advertise neither fs nor terminal.
        let off = serde_json::to_value(initialize_request(false, false)).unwrap();
        assert_eq!(
            off["clientCapabilities"]["fs"]["readTextFile"],
            json!(false)
        );
        assert_eq!(
            off["clientCapabilities"]["fs"]["writeTextFile"],
            json!(false)
        );
        assert_eq!(off["clientCapabilities"]["terminal"], json!(false));

        // Break-glass on, read-write: advertise read + write + terminal.
        let rw = serde_json::to_value(initialize_request(true, false)).unwrap();
        assert_eq!(rw["clientCapabilities"]["fs"]["readTextFile"], json!(true));
        assert_eq!(rw["clientCapabilities"]["fs"]["writeTextFile"], json!(true));
        assert_eq!(rw["clientCapabilities"]["terminal"], json!(true));

        // Break-glass on, read-only: read only — writes and terminal blocked.
        let ro = serde_json::to_value(initialize_request(true, true)).unwrap();
        assert_eq!(ro["clientCapabilities"]["fs"]["readTextFile"], json!(true));
        assert_eq!(
            ro["clientCapabilities"]["fs"]["writeTextFile"],
            json!(false)
        );
        assert_eq!(ro["clientCapabilities"]["terminal"], json!(false));
    }

    #[test]
    fn select_mcp_prefers_http_primary_when_agent_supports_http() {
        let http = McpBinding::http("notesmith", "http://127.0.0.1:27183/mcp/work");
        let stdio = McpBinding::local_bridge("notesmith", "work", false);
        let chosen = select_mcp_binding(true, Some(&http), Some(&stdio));
        assert_eq!(chosen, Some(&http));
    }

    #[test]
    fn select_mcp_falls_back_to_stdio_when_agent_lacks_http() {
        let http = McpBinding::http("notesmith", "http://127.0.0.1:27183/mcp/work");
        let stdio = McpBinding::local_bridge("notesmith", "work", false);
        let chosen = select_mcp_binding(false, Some(&http), Some(&stdio));
        assert_eq!(chosen, Some(&stdio));
    }

    #[test]
    fn select_mcp_keeps_http_when_no_stdio_fallback_exists() {
        let http = McpBinding::http("notesmith", "http://127.0.0.1:27183/mcp/work");
        // Remote daemon: no stdio fallback, agent lacks http → keep what we have.
        assert_eq!(select_mcp_binding(false, Some(&http), None), Some(&http));
    }

    #[test]
    fn select_mcp_returns_none_without_any_binding() {
        assert_eq!(select_mcp_binding(true, None, None), None);
    }

    #[test]
    fn new_session_request_includes_mcp_server_when_bound() {
        let binding = McpBinding::http("notesmith", "http://127.0.0.1:27183/mcp/work");
        let value = serde_json::to_value(new_session_request("/vault", Some(&binding))).unwrap();
        let servers = value["mcpServers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["type"], json!("http"));
        assert_eq!(servers[0]["url"], json!("http://127.0.0.1:27183/mcp/work"));
        assert_eq!(value["cwd"], json!("/vault"));
    }

    #[test]
    fn new_session_request_uses_a_stdio_transport_for_local_bridges() {
        let binding = McpBinding::local_bridge("notesmith", "work", false);
        let value = serde_json::to_value(new_session_request("/vault", Some(&binding))).unwrap();
        let servers = value["mcpServers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["command"], json!("notesmith"));
        assert_eq!(
            servers[0]["args"],
            json!(["--vault", "work", "mcp", "start"])
        );
    }

    #[test]
    fn new_session_request_has_empty_servers_without_mcp() {
        let value = serde_json::to_value(new_session_request("/vault", None)).unwrap();
        assert_eq!(value["mcpServers"], json!([]));
    }

    #[test]
    fn prompt_blocks_wrap_text_in_a_content_block() {
        let blocks = prompt_blocks(None, None, "hello");
        assert_eq!(blocks.len(), 1);
        let value = serde_json::to_value(&blocks[0]).unwrap();
        assert_eq!(value, json!({ "type": "text", "text": "hello" }));
    }

    #[test]
    fn prompt_blocks_prepend_preamble_block_when_present() {
        let blocks = prompt_blocks(Some("context"), None, "hello");
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            serde_json::to_value(&blocks[0]).unwrap(),
            json!({ "type": "text", "text": "context" })
        );
        assert_eq!(
            serde_json::to_value(&blocks[1]).unwrap(),
            json!({ "type": "text", "text": "hello" })
        );
    }

    #[test]
    fn prompt_blocks_order_preamble_then_editor_then_message() {
        let blocks = prompt_blocks(Some("PRE"), Some("EDITOR"), "hello");
        let texts: Vec<String> = blocks
            .iter()
            .filter_map(|b| {
                serde_json::to_value(b).unwrap()["text"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(texts, vec!["PRE", "EDITOR", "hello"]);
    }

    #[test]
    fn prompt_blocks_carry_editor_context_without_a_preamble() {
        // Later turns: the preamble is gone but editor context still rides along.
        let blocks = prompt_blocks(None, Some("EDITOR"), "hello");
        let texts: Vec<String> = blocks
            .iter()
            .filter_map(|b| {
                serde_json::to_value(b).unwrap()["text"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(texts, vec!["EDITOR", "hello"]);
    }

    #[test]
    fn session_preamble_steers_to_mcp_and_reflects_local_io() {
        let with_mcp = session_preamble(true, false);
        assert!(with_mcp.contains("Notesmith MCP tools"));
        assert!(with_mcp.contains("do not attempt to run shell commands"));

        let with_io = session_preamble(true, true);
        assert!(with_io.contains("scoped filesystem and terminal access"));
    }

    #[test]
    fn agent_message_chunk_maps_to_a_delta() {
        let update = update_from(json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" },
        }));
        assert_eq!(
            map_session_update(update),
            vec![AgentEvent::AgentMessageDelta {
                text: "hello".to_string()
            }]
        );
    }

    #[test]
    fn non_text_content_chunk_is_ignored() {
        let update = update_from(json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "image", "data": "AAAA", "mimeType": "image/png" },
        }));
        assert!(map_session_update(update).is_empty());
    }

    #[test]
    fn tool_call_maps_to_a_tool_call_event() {
        let update = update_from(json!({
            "sessionUpdate": "tool_call",
            "toolCallId": "toolu_1",
            "title": "Read",
            "rawInput": { "path": "note.md" },
        }));
        assert_eq!(
            map_session_update(update),
            vec![AgentEvent::ToolCall(ToolCall {
                id: Some("toolu_1".to_string()),
                name: "Read".to_string(),
                args: json!({ "path": "note.md" }),
            })]
        );
    }

    #[test]
    fn completed_tool_update_maps_to_a_tool_result() {
        let update = update_from(json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "toolu_1",
            "status": "completed",
            "content": [
                { "type": "content", "content": { "type": "text", "text": "done" } }
            ],
        }));
        assert_eq!(
            map_session_update(update),
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("toolu_1".to_string()),
                content: "done".to_string(),
                is_error: false,
            })]
        );
    }

    #[test]
    fn failed_tool_update_sets_the_error_flag() {
        let update = update_from(json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "toolu_1",
            "status": "failed",
        }));
        let events = map_session_update(update);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::ToolResult(result) => assert!(result.is_error),
            other => panic!("expected a tool result, got {other:?}"),
        }
    }

    #[test]
    fn in_progress_tool_update_yields_no_event() {
        let update = update_from(json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "toolu_1",
            "status": "in_progress",
        }));
        assert!(map_session_update(update).is_empty());
    }

    #[test]
    fn unknown_update_kinds_are_ignored() {
        let update = update_from(json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": "thinking" },
        }));
        assert!(map_session_update(update).is_empty());
    }

    fn option(id: &str, kind: &str) -> PermissionOption {
        serde_json::from_value(json!({ "optionId": id, "name": id, "kind": kind }))
            .expect("valid option")
    }

    fn selected_id(outcome: &RequestPermissionOutcome) -> Option<PermissionOptionId> {
        match outcome {
            RequestPermissionOutcome::Selected(selected) => Some(selected.option_id.clone()),
            _ => None,
        }
    }

    #[test]
    fn allow_outcome_selects_an_allow_option() {
        let options = vec![option("yes", "allow_once"), option("no", "reject_once")];
        let outcome = select_permission_option(&options, true, false);
        assert_eq!(
            selected_id(&outcome).map(|id| id.0.to_string()),
            Some("yes".to_string())
        );
    }

    #[test]
    fn deny_outcome_selects_a_reject_option() {
        let options = vec![option("yes", "allow_once"), option("no", "reject_once")];
        let outcome = select_permission_option(&options, false, false);
        assert_eq!(
            selected_id(&outcome).map(|id| id.0.to_string()),
            Some("no".to_string())
        );
    }

    #[test]
    fn option_selection_prefers_once_then_always() {
        let options = vec![option("a", "allow_always"), option("b", "allow_once")];
        let outcome = select_permission_option(&options, true, false);
        assert_eq!(
            selected_id(&outcome).map(|id| id.0.to_string()),
            Some("b".to_string())
        );
    }

    #[test]
    fn option_selection_prefers_always_when_remembering() {
        let options = vec![option("b", "allow_once"), option("a", "allow_always")];
        let outcome = select_permission_option(&options, true, true);
        assert_eq!(
            selected_id(&outcome).map(|id| id.0.to_string()),
            Some("a".to_string())
        );
    }

    #[test]
    fn option_selection_with_no_matching_option_is_cancelled() {
        let options = vec![option("yes", "allow_once")];
        let outcome = select_permission_option(&options, false, false);
        assert!(matches!(outcome, RequestPermissionOutcome::Cancelled));
    }

    fn permission_req(json: Value) -> RequestPermissionRequest {
        serde_json::from_value(json).expect("valid permission request")
    }

    #[test]
    fn permission_request_info_keys_on_the_tool_title() {
        let req = permission_req(json!({
            "sessionId": "s",
            "toolCall": { "toolCallId": "t1", "title": "create_note", "kind": "edit" },
            "options": [],
        }));
        let info = permission_request_info(&req);
        assert_eq!(info.tool, "create_note");
        assert_eq!(info.kind.as_deref(), Some("edit"));
    }

    #[test]
    fn permission_request_info_falls_back_to_kind_then_placeholder() {
        let by_kind = permission_req(json!({
            "sessionId": "s",
            "toolCall": { "toolCallId": "t1", "kind": "execute" },
            "options": [],
        }));
        assert_eq!(permission_request_info(&by_kind).tool, "execute");

        let placeholder = permission_req(json!({
            "sessionId": "s",
            "toolCall": { "toolCallId": "t1" },
            "options": [],
        }));
        assert_eq!(permission_request_info(&placeholder).tool, "tool");
    }

    #[tokio::test]
    async fn answer_permission_hard_denies_in_read_only() {
        let req = permission_req(json!({
            "sessionId": "s",
            "toolCall": { "toolCallId": "t1", "title": "create_note" },
            "options": [
                { "optionId": "yes", "name": "Allow", "kind": "allow_once" },
                { "optionId": "no", "name": "Reject", "kind": "reject_once" },
            ],
        }));
        let state = PermissionState::new();
        // A decider that would allow must NOT be consulted in read-only mode.
        let decider: Arc<dyn PermissionDecider> =
            Arc::new(FixedDecider(PermissionDecision::AllowAlways));
        let outcome = answer_permission(&req, true, &state, &decider).await;
        assert_eq!(
            selected_id(&outcome).map(|id| id.0.to_string()),
            Some("no".to_string())
        );
        assert!(!state.is_always("create_note"));
    }

    #[tokio::test]
    async fn answer_permission_remembers_allow_always_per_tool() {
        let req = permission_req(json!({
            "sessionId": "s",
            "toolCall": { "toolCallId": "t1", "title": "create_note" },
            "options": [
                { "optionId": "yes", "name": "Allow", "kind": "allow_once" },
                { "optionId": "always", "name": "Always", "kind": "allow_always" },
                { "optionId": "no", "name": "Reject", "kind": "reject_once" },
            ],
        }));
        let state = PermissionState::new();
        let decider: Arc<dyn PermissionDecider> =
            Arc::new(FixedDecider(PermissionDecision::AllowAlways));

        let first = answer_permission(&req, false, &state, &decider).await;
        assert_eq!(
            selected_id(&first).map(|id| id.0.to_string()),
            Some("always".to_string())
        );
        assert!(state.is_always("create_note"));

        // A later prompt for the same tool is allowed silently — the decider
        // (which here would still allow) is bypassed, and "once" is selected
        // because the grant is already recorded.
        let deny: Arc<dyn PermissionDecider> = Arc::new(FixedDecider(PermissionDecision::Deny));
        let second = answer_permission(&req, false, &state, &deny).await;
        assert_eq!(
            selected_id(&second).map(|id| id.0.to_string()),
            Some("yes".to_string())
        );
    }

    #[test]
    fn mcp_url_scope_detection() {
        assert!(mcp_url_is_read_only(Some("http://x/mcp-ro/work")));
        assert!(!mcp_url_is_read_only(Some("http://x/mcp/work")));
        assert!(mcp_url_is_read_only(None));
        assert!(mcp_url_is_read_only(Some("")));
    }

    #[test]
    fn copilot_builds_acp_launch() {
        let session = AcpSession::copilot(None);
        assert_eq!(session.program, "copilot");
        assert_eq!(session.args, vec!["--acp".to_string()]);
    }

    #[test]
    fn copilot_honors_binary_override() {
        let session = AcpSession::copilot(Some("/opt/copilot"));
        assert_eq!(session.program, "/opt/copilot");
    }

    #[test]
    fn claude_code_launches_the_adapter_via_npx() {
        let session = AcpSession::claude_code(None);
        assert_eq!(session.program, "npx");
        assert_eq!(
            session.args,
            vec!["--yes".to_string(), CLAUDE_ACP_PACKAGE.to_string()]
        );
        assert!(session.setup_hint.is_some());
    }

    #[test]
    fn claude_code_binary_override_runs_directly() {
        let session = AcpSession::claude_code(Some("claude-code-acp"));
        assert_eq!(session.program, "claude-code-acp");
        assert!(session.args.is_empty());
    }

    #[test]
    fn codex_launches_the_default_adapter_binary() {
        let session = AcpSession::codex(None);
        assert_eq!(session.program, DEFAULT_CODEX_ACP_BIN);
        assert!(session.setup_hint.is_some());
    }

    #[test]
    fn codex_honors_binary_override() {
        let session = AcpSession::codex(Some("/opt/codex-acp"));
        assert_eq!(session.program, "/opt/codex-acp");
    }

    #[test]
    fn gemini_launches_with_experimental_acp() {
        let session = AcpSession::gemini(None);
        assert_eq!(session.program, "gemini");
        assert_eq!(session.args, vec!["--experimental-acp".to_string()]);
        assert!(session.setup_hint.is_some());
    }

    #[test]
    fn gemini_honors_binary_override() {
        let session = AcpSession::gemini(Some("/opt/gemini"));
        assert_eq!(session.program, "/opt/gemini");
        assert_eq!(session.args, vec!["--experimental-acp".to_string()]);
    }

    #[test]
    fn explain_appends_setup_hint_when_present() {
        let session = AcpSession::codex(None);
        let explained = session.explain(AgentError::Protocol("boom".to_string()));
        assert!(explained.to_string().contains("codex-acp"));
        assert!(explained.to_string().contains("boom"));
    }

    #[test]
    fn explain_is_a_noop_without_a_hint() {
        let session = AcpSession::new("x", Vec::new());
        let explained = session.explain(AgentError::Protocol("boom".to_string()));
        assert_eq!(explained.to_string(), "agent protocol error: boom");
    }

    #[test]
    fn with_mcp_derives_scope_from_binding() {
        let read_only = AcpSession::new("x", Vec::new())
            .with_mcp(McpBinding::http("notesmith", "http://x/mcp-ro/work"));
        assert!(read_only.read_only);

        let read_write = AcpSession::new("x", Vec::new())
            .with_mcp(McpBinding::http("notesmith", "http://x/mcp/work"));
        assert!(!read_write.read_only);

        let local = AcpSession::new("x", Vec::new()).with_mcp(McpBinding::local_bridge(
            "notesmith",
            "work",
            true,
        ));
        assert!(local.read_only);
    }

    #[test]
    fn absolute_cwd_is_always_absolute() {
        let session = AcpSession::new("x", Vec::new()).in_dir(Some(PathBuf::from(".")));
        assert!(PathBuf::from(session.absolute_cwd()).is_absolute());
    }

    #[test]
    fn with_env_stores_environment_variables() {
        let session = AcpSession::new("x", Vec::new()).with_env(vec![
            ("FOO".to_string(), "bar".to_string()),
            ("BAZ".to_string(), "qux".to_string()),
        ]);
        assert_eq!(
            session.env,
            vec![
                ("FOO".to_string(), "bar".to_string()),
                ("BAZ".to_string(), "qux".to_string()),
            ]
        );
    }

    #[test]
    fn with_env_defaults_to_empty() {
        let session = AcpSession::new("x", Vec::new());
        assert!(session.env.is_empty());
    }
}
