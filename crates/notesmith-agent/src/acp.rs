//! Agent Client Protocol (ACP) transport (ADR 0011 Phase E).
//!
//! ACP is a JSON-RPC 2.0 protocol spoken over a child process's stdio, framed
//! as **newline-delimited JSON** (one message per line). It is the single
//! convergence transport that replaces the per-agent line adapters: one client
//! plus one [`AcpSession`] drives any ACP-speaking agent. For Phase E1 the only
//! wired agent is the GitHub Copilot CLI, which speaks ACP natively
//! (`copilot --acp`).
//!
//! The session lifecycle is:
//!
//! 1. `initialize` — negotiate `protocolVersion` and client capabilities.
//! 2. `session/new` — open a session carrying the working directory (`cwd`) and
//!    the per-vault MCP servers (`mcpServers`); the agent replies with a
//!    `sessionId`.
//! 3. `session/prompt` — one per user turn. The agent streams `session/update`
//!    notifications (assistant text chunks, tool calls/updates) and finally
//!    answers the request with a `stopReason`.
//!
//! While a prompt is in flight the agent may call back with
//! `session/request_permission`; the runner answers it from the read-only /
//! read-write scope (read-only rejects, read-write approves), which is the ACP
//! analogue of the Phase C scope toggle.
//!
//! Per ADR 0009 the reader is tolerant: a malformed line becomes an
//! [`AgentEvent::Error`] on the stream and never panics or ends the session.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::acp_client::ClientHandler;
use crate::error::AgentError;
use crate::event::{AgentEvent, ToolCall, ToolResult};
use crate::mcp::McpBinding;
use crate::session::AgentSession;

/// ACP protocol version negotiated in `initialize`.
const PROTOCOL_VERSION: u32 = 1;

/// Default binary that speaks ACP natively (GitHub Copilot CLI).
pub const DEFAULT_COPILOT_BIN: &str = "copilot";

/// npm package providing the Claude Code ACP adapter (run via `npx`).
pub const CLAUDE_ACP_PACKAGE: &str = "@zed-industries/claude-code-acp";

/// Default binary providing the Codex ACP adapter.
pub const DEFAULT_CODEX_ACP_BIN: &str = "codex-acp";

/// Build the `initialize` request params.
///
/// File-system and terminal client capabilities are advertised as `local_io`:
/// when the opt-in `agent.local_file_access` setting is on (ADR 0012) Notesmith
/// proxies the agent's `fs/*` and `terminal/*` requests, scoped to the vault
/// directory; otherwise they stay off and the agent reaches the vault through
/// the Notesmith MCP tools only.
fn initialize_params(local_io: bool) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": { "readTextFile": local_io, "writeTextFile": local_io },
            "terminal": local_io,
        },
    })
}

/// Build a one-time context preamble steering the agent to the Notesmith MCP
/// tools (ADR 0012). It is prepended to the first prompt of a session so the
/// agent prefers vault-aware tools over guessing at the filesystem.
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

/// Build the `session/new` request params for `cwd`, wiring the active vault's
/// MCP endpoint (when present) into the `mcpServers` array.
fn session_new_params(cwd: &str, mcp: Option<&McpBinding>) -> Value {
    let servers: Vec<Value> = mcp.map(|m| vec![m.acp_server_json()]).unwrap_or_default();
    json!({ "cwd": cwd, "mcpServers": servers })
}

/// Build the `session/prompt` request params for a single user turn. When
/// `preamble` is set it is sent as a leading text block so the agent receives
/// the Notesmith context ahead of the user's first message.
fn prompt_params(session_id: &str, preamble: Option<&str>, text: &str) -> Value {
    let mut blocks = Vec::new();
    if let Some(preamble) = preamble {
        blocks.push(json!({ "type": "text", "text": preamble }));
    }
    blocks.push(json!({ "type": "text", "text": text }));
    json!({
        "sessionId": session_id,
        "prompt": blocks,
    })
}

/// Map a `session/update` notification's `update` object to normalized events.
///
/// Recognized `sessionUpdate` kinds:
/// - `agent_message_chunk` — a text content block becomes an
///   [`AgentEvent::AgentMessageDelta`].
/// - `tool_call` — becomes an [`AgentEvent::ToolCall`].
/// - `tool_call_update` — a terminal status (`completed`/`failed`) becomes an
///   [`AgentEvent::ToolResult`]; non-terminal updates are ignored.
///
/// Any other kind (commands/config/plan/thoughts) yields no event.
fn map_session_update(update: &Value) -> Vec<AgentEvent> {
    let kind = update.get("sessionUpdate").and_then(Value::as_str);
    match kind {
        Some("agent_message_chunk") => update
            .get("content")
            .and_then(text_content)
            .map(|text| vec![AgentEvent::AgentMessageDelta { text }])
            .unwrap_or_default(),
        Some("tool_call") => {
            let name = update
                .get("title")
                .and_then(Value::as_str)
                .or_else(|| update.get("kind").and_then(Value::as_str))
                .unwrap_or("tool")
                .to_string();
            vec![AgentEvent::ToolCall(ToolCall {
                id: update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                name,
                args: update.get("rawInput").cloned().unwrap_or_else(|| json!({})),
            })]
        }
        Some("tool_call_update") => {
            let status = update.get("status").and_then(Value::as_str);
            match status {
                Some("completed") | Some("failed") => vec![AgentEvent::ToolResult(ToolResult {
                    id: update
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    content: tool_update_content(update),
                    is_error: status == Some("failed"),
                })],
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Extract text from an ACP content block (`{"type":"text","text":...}`).
fn text_content(content: &Value) -> Option<String> {
    match content.get("type").and_then(Value::as_str) {
        Some("text") => content
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// Concatenate the text of a `tool_call_update`'s `content` array.
fn tool_update_content(update: &Value) -> String {
    update
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| {
                    // Tool content blocks wrap their payload under `content`.
                    block
                        .get("content")
                        .and_then(text_content)
                        .or_else(|| text_content(block))
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Map the response to a `session/prompt` request to a terminal event.
///
/// A JSON-RPC error becomes an [`AgentEvent::Error`]; otherwise the turn ends
/// with [`AgentEvent::Done`] (the assistant text already arrived as deltas).
fn map_prompt_response(message: &Value) -> AgentEvent {
    if let Some(error) = message.get("error") {
        let msg = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("agent returned an error");
        return AgentEvent::Error {
            message: msg.to_string(),
        };
    }
    AgentEvent::Done { result: None }
}

/// Decide how to answer a `session/request_permission` callback given the
/// session scope. Read-write selects an `allow_*` option; read-only selects a
/// `reject_*` option. Falls back to cancelling when no suitable option exists.
///
/// Returns the JSON-RPC `result` body for the response.
pub(crate) fn permission_result(params: &Value, read_only: bool) -> Value {
    let options = params.get("options").and_then(Value::as_array);
    let wanted_prefix = if read_only { "reject" } else { "allow" };

    let pick = options.and_then(|opts| {
        // Prefer a "once" option of the wanted polarity, then any of it.
        opts.iter()
            .find(|o| option_kind(o) == Some(&format!("{wanted_prefix}_once")))
            .or_else(|| {
                opts.iter()
                    .find(|o| option_kind(o).is_some_and(|k| k.starts_with(wanted_prefix)))
            })
            .and_then(|o| o.get("optionId").and_then(Value::as_str))
    });

    match pick {
        Some(option_id) => json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
        None => json!({ "outcome": { "outcome": "cancelled" } }),
    }
}

fn option_kind(option: &Value) -> Option<&str> {
    option.get("kind").and_then(Value::as_str)
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

/// Shared connection state used by both the session and its reader task.
struct Connection {
    writer_tx: mpsc::UnboundedSender<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    next_id: AtomicU64,
}

impl Connection {
    fn alloc_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn write_message(&self, message: &Value) -> Result<(), AgentError> {
        self.writer_tx
            .send(message.to_string())
            .map_err(|_| AgentError::Protocol("agent connection closed".to_string()))
    }

    /// Send a JSON-RPC request and await its response.
    async fn request(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        let id = self.alloc_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        rx.await
            .map_err(|_| AgentError::Protocol(format!("agent closed before answering {method}")))
    }

    /// Send a `session/prompt` request without blocking on completion: its
    /// response is forwarded to the event stream as a terminal event so the
    /// turn's streaming deltas keep flowing through [`AgentSession::next_event`].
    async fn send_prompt(
        &self,
        session_id: &str,
        preamble: Option<&str>,
        text: &str,
    ) -> Result<(), AgentError> {
        let id = self.alloc_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/prompt",
            "params": prompt_params(session_id, preamble, text),
        }))?;
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            if let Ok(response) = rx.await {
                let _ = event_tx.send(map_prompt_response(&response));
            }
        });
        Ok(())
    }
}

/// An [`AgentSession`] driven over the Agent Client Protocol.
///
/// The child process is spawned lazily on the first [`send`](AgentSession::send),
/// which runs the `initialize` + `session/new` handshake and then issues the
/// first `session/prompt`. Subsequent sends reuse the same session id, so ACP
/// sessions are **multi-turn** (unlike the single-shot line agents).
pub struct AcpSession {
    program: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    mcp: Option<McpBinding>,
    read_only: bool,
    local_io: bool,
    preamble_sent: bool,
    setup_hint: Option<String>,
    conn: Option<Connection>,
    event_rx: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    child: Option<Child>,
    session_id: Option<String>,
}

impl AcpSession {
    /// Build an ACP session that launches `program` with `args`.
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            working_dir: None,
            mcp: None,
            read_only: true,
            local_io: false,
            preamble_sent: false,
            setup_hint: None,
            conn: None,
            event_rx: None,
            child: None,
            session_id: None,
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

    /// Build an ACP session for Claude Code via its ACP adapter binary
    /// (ADR 0011 Phase E2).
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

    /// Build an ACP session for Codex via its ACP adapter binary
    /// (ADR 0011 Phase E2).
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

    /// Run the agent in `working_dir` (the active vault's directory).
    pub fn in_dir(mut self, working_dir: Option<PathBuf>) -> Self {
        self.working_dir = working_dir;
        self
    }

    /// Grant the agent scoped filesystem and terminal access to the vault
    /// directory (the opt-in `agent.local_file_access` setting, ADR 0012). Off
    /// by default; when off the agent reaches the vault through MCP tools only.
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

    /// Auto-wire the active vault's MCP endpoint into `session/new` and derive
    /// the permission scope (read-only vs read-write) from the endpoint URL.
    pub fn with_mcp(mut self, binding: McpBinding) -> Self {
        self.read_only = binding.url.contains("/mcp-ro/");
        self.mcp = Some(binding);
        self
    }

    /// Explicitly set the read-only permission scope (overrides the value
    /// derived from the MCP endpoint).
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Resolve the session working directory as an **absolute** path. ACP
    /// agents (Copilot) reject relative `cwd` values, so a missing or relative
    /// working directory is resolved against the process's current directory.
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
        // Best-effort canonicalization; fall back to the joined path if the
        // directory cannot be canonicalized (e.g. it does not exist).
        let resolved = std::fs::canonicalize(&absolute).unwrap_or(absolute);
        resolved.to_string_lossy().into_owned()
    }

    /// Spawn the process, start the reader/writer tasks, and run the
    /// `initialize` + `session/new` handshake.
    async fn initialize(&mut self) -> Result<(), AgentError> {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(dir) = &self.working_dir {
            command.current_dir(dir);
        }
        let mut child = command.spawn().map_err(|source| AgentError::Spawn {
            program: self.program.clone(),
            source,
        })?;

        let mut stdin = child.stdin.take().ok_or(AgentError::MissingPipe("stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(AgentError::MissingPipe("stdout"))?;

        let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(line) = writer_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let reader_pending = pending.clone();
        let reader_event_tx = event_tx.clone();
        let reader_writer_tx = writer_tx.clone();
        let cwd = self.absolute_cwd();
        let handler = Arc::new(ClientHandler::new(
            self.read_only,
            self.local_io,
            PathBuf::from(&cwd),
        ));
        tokio::spawn(async move {
            drive_acp(
                BufReader::new(stdout),
                reader_pending,
                reader_event_tx,
                reader_writer_tx,
                handler,
            )
            .await;
        });

        let conn = Connection {
            writer_tx,
            pending,
            event_tx,
            next_id: AtomicU64::new(1),
        };

        conn.request("initialize", initialize_params(self.local_io))
            .await?;

        let new_response = conn
            .request("session/new", session_new_params(&cwd, self.mcp.as_ref()))
            .await?;
        let session_id = new_response
            .pointer("/result/sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::Protocol("session/new did not return a sessionId".into()))?
            .to_string();

        self.conn = Some(conn);
        self.event_rx = Some(event_rx);
        self.child = Some(child);
        self.session_id = Some(session_id);
        Ok(())
    }

    /// Append the setup hint (when set) to a startup/handshake failure so a
    /// missing ACP adapter binary surfaces actionable guidance.
    fn explain(&self, error: AgentError) -> AgentError {
        match &self.setup_hint {
            Some(hint) => AgentError::Protocol(format!("{error} — {hint}")),
            None => error,
        }
    }
}

impl AgentSession for AcpSession {
    async fn send(&mut self, message: &str) -> Result<(), AgentError> {
        if self.conn.is_none() {
            self.initialize()
                .await
                .map_err(|error| self.explain(error))?;
        }
        let session_id = self
            .session_id
            .clone()
            .ok_or_else(|| AgentError::Protocol("session is not initialized".into()))?;
        // The Notesmith context preamble (ADR 0012) is sent once, ahead of the
        // first user message, to steer the agent toward the MCP tools.
        let preamble = if self.preamble_sent {
            None
        } else {
            self.preamble_sent = true;
            Some(session_preamble(self.mcp.is_some(), self.local_io))
        };
        let conn = self
            .conn
            .as_ref()
            .ok_or_else(|| AgentError::Protocol("session is not initialized".into()))?;
        conn.send_prompt(&session_id, preamble.as_deref(), message)
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
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}

/// Read newline-delimited JSON-RPC messages from `reader` and dispatch them:
/// responses resolve pending requests, `session/update` notifications map to
/// events, and inbound agent requests (permission / fs / terminal) are answered
/// via `writer_tx` through the [`ClientHandler`]. Returns on EOF (which drops
/// `event_tx`, ending the session) or a read error (reported as a single
/// [`AgentEvent::Error`]).
async fn drive_acp<R>(
    reader: R,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    writer_tx: mpsc::UnboundedSender<String>,
    handler: Arc<ClientHandler>,
) where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                let message: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(err) => {
                        let _ = event_tx.send(AgentEvent::Error {
                            message: format!("could not parse agent message: {err}"),
                        });
                        continue;
                    }
                };
                dispatch(&message, &pending, &event_tx, &writer_tx, &handler).await;
            }
            Ok(None) => return,
            Err(err) => {
                let _ = event_tx.send(AgentEvent::Error {
                    message: format!("could not read agent output: {err}"),
                });
                return;
            }
        }
    }
}

async fn dispatch(
    message: &Value,
    pending: &Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    event_tx: &mpsc::UnboundedSender<AgentEvent>,
    writer_tx: &mpsc::UnboundedSender<String>,
    handler: &Arc<ClientHandler>,
) {
    let method = message.get("method").and_then(Value::as_str);
    let has_id = message.get("id").is_some();

    match (has_id, method) {
        // Inbound request from the agent (carries both id and method). Handle it
        // on a detached task so a blocking handler (e.g. `terminal/wait_for_exit`)
        // never stalls the reader loop.
        (true, Some(method)) => {
            let id = message.get("id").cloned().unwrap_or(Value::Null);
            let method = method.to_string();
            let params = message.get("params").cloned();
            let handler = handler.clone();
            let writer_tx = writer_tx.clone();
            tokio::spawn(async move {
                let response = handler.handle(&method, params.as_ref(), id).await;
                let _ = writer_tx.send(response.to_string());
            });
        }
        // Response to one of our requests.
        (true, None) => {
            if let Some(id) = message.get("id").and_then(Value::as_u64) {
                if let Some(tx) = pending.lock().await.remove(&id) {
                    let _ = tx.send(message.clone());
                }
            }
        }
        // Notification.
        (false, Some("session/update")) => {
            if let Some(update) = message.pointer("/params/update") {
                for event in map_session_update(update) {
                    if event_tx.send(event).is_err() {
                        return;
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_params_advertise_protocol_version_and_capabilities_by_local_io() {
        let off = initialize_params(false);
        assert_eq!(off["protocolVersion"], json!(PROTOCOL_VERSION));
        assert_eq!(
            off["clientCapabilities"]["fs"]["readTextFile"],
            json!(false)
        );
        assert_eq!(
            off["clientCapabilities"]["fs"]["writeTextFile"],
            json!(false)
        );
        assert_eq!(off["clientCapabilities"]["terminal"], json!(false));

        let on = initialize_params(true);
        assert_eq!(on["clientCapabilities"]["fs"]["readTextFile"], json!(true));
        assert_eq!(on["clientCapabilities"]["fs"]["writeTextFile"], json!(true));
        assert_eq!(on["clientCapabilities"]["terminal"], json!(true));
    }

    #[test]
    fn session_new_params_include_mcp_server_when_bound() {
        let binding = McpBinding::new("notesmith", "http://h/mcp/work");
        let params = session_new_params("/vaults/work", Some(&binding));
        assert_eq!(params["cwd"], json!("/vaults/work"));
        assert_eq!(params["mcpServers"][0]["url"], json!("http://h/mcp/work"));
        assert_eq!(params["mcpServers"][0]["type"], json!("http"));
    }

    #[test]
    fn session_new_params_have_empty_servers_without_mcp() {
        let params = session_new_params(".", None);
        assert_eq!(params["mcpServers"], json!([]));
    }

    #[test]
    fn prompt_params_wrap_text_in_a_content_block() {
        let params = prompt_params("sess-1", None, "hello");
        assert_eq!(params["sessionId"], json!("sess-1"));
        assert_eq!(params["prompt"][0]["type"], json!("text"));
        assert_eq!(params["prompt"][0]["text"], json!("hello"));
    }

    #[test]
    fn prompt_params_prepend_preamble_block_when_present() {
        let params = prompt_params("sess-1", Some("context"), "hello");
        assert_eq!(params["prompt"][0]["text"], json!("context"));
        assert_eq!(params["prompt"][1]["text"], json!("hello"));
    }

    #[test]
    fn session_preamble_steers_to_mcp_and_reflects_local_io() {
        let mcp_only = session_preamble(true, false);
        assert!(mcp_only.contains("Notesmith MCP tools"));
        assert!(mcp_only.contains("do NOT have shell"));

        let with_io = session_preamble(true, true);
        assert!(with_io.contains("scoped filesystem and terminal access"));

        let no_mcp = session_preamble(false, false);
        assert!(!no_mcp.contains("search_notes"));
    }

    #[test]
    fn agent_message_chunk_maps_to_a_delta() {
        let update = json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "Hi" },
        });
        assert_eq!(
            map_session_update(&update),
            vec![AgentEvent::AgentMessageDelta {
                text: "Hi".to_string()
            }]
        );
    }

    #[test]
    fn non_text_content_chunk_is_ignored() {
        let update = json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "image", "data": "..." },
        });
        assert!(map_session_update(&update).is_empty());
    }

    #[test]
    fn tool_call_maps_to_a_tool_call_event() {
        let update = json!({
            "sessionUpdate": "tool_call",
            "toolCallId": "call_1",
            "title": "Read note",
            "kind": "read",
            "rawInput": { "path": "a.md" },
        });
        assert_eq!(
            map_session_update(&update),
            vec![AgentEvent::ToolCall(ToolCall {
                id: Some("call_1".to_string()),
                name: "Read note".to_string(),
                args: json!({ "path": "a.md" }),
            })]
        );
    }

    #[test]
    fn completed_tool_update_maps_to_a_tool_result() {
        let update = json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_1",
            "status": "completed",
            "content": [{ "type": "content", "content": { "type": "text", "text": "ok" } }],
        });
        assert_eq!(
            map_session_update(&update),
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("call_1".to_string()),
                content: "ok".to_string(),
                is_error: false,
            })]
        );
    }

    #[test]
    fn failed_tool_update_sets_the_error_flag() {
        let update = json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_1",
            "status": "failed",
            "content": [],
        });
        let events = map_session_update(&update);
        assert!(matches!(
            events.as_slice(),
            [AgentEvent::ToolResult(ToolResult { is_error: true, .. })]
        ));
    }

    #[test]
    fn in_progress_tool_update_yields_no_event() {
        let update = json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_1",
            "status": "in_progress",
        });
        assert!(map_session_update(&update).is_empty());
    }

    #[test]
    fn unknown_update_kinds_are_ignored() {
        for kind in ["available_commands_update", "config_option_update", "plan"] {
            let update = json!({ "sessionUpdate": kind });
            assert!(map_session_update(&update).is_empty(), "kind={kind}");
        }
    }

    #[test]
    fn prompt_response_with_stop_reason_is_done() {
        let message = json!({ "jsonrpc": "2.0", "id": 3, "result": { "stopReason": "end_turn" } });
        assert_eq!(
            map_prompt_response(&message),
            AgentEvent::Done { result: None }
        );
    }

    #[test]
    fn prompt_response_error_is_an_error_event() {
        let message =
            json!({ "jsonrpc": "2.0", "id": 3, "error": { "code": -1, "message": "boom" } });
        assert_eq!(
            map_prompt_response(&message),
            AgentEvent::Error {
                message: "boom".to_string()
            }
        );
    }

    #[test]
    fn read_write_permission_selects_an_allow_option() {
        let params = json!({
            "options": [
                { "optionId": "deny", "kind": "reject_once" },
                { "optionId": "ok", "kind": "allow_once" },
                { "optionId": "always", "kind": "allow_always" },
            ],
        });
        let result = permission_result(&params, false);
        assert_eq!(result["outcome"]["outcome"], json!("selected"));
        assert_eq!(result["outcome"]["optionId"], json!("ok"));
    }

    #[test]
    fn read_only_permission_selects_a_reject_option() {
        let params = json!({
            "options": [
                { "optionId": "ok", "kind": "allow_once" },
                { "optionId": "deny", "kind": "reject_once" },
            ],
        });
        let result = permission_result(&params, true);
        assert_eq!(result["outcome"]["optionId"], json!("deny"));
    }

    #[test]
    fn permission_with_no_matching_option_is_cancelled() {
        let params = json!({ "options": [{ "optionId": "ok", "kind": "allow_once" }] });
        let result = permission_result(&params, true);
        assert_eq!(result["outcome"]["outcome"], json!("cancelled"));
    }

    #[test]
    fn permission_with_missing_options_is_cancelled() {
        assert_eq!(
            permission_result(&Value::Null, false)["outcome"]["outcome"],
            json!("cancelled")
        );
    }

    #[test]
    fn mcp_url_scope_detection() {
        assert!(mcp_url_is_read_only(Some("http://h/mcp-ro/work")));
        assert!(!mcp_url_is_read_only(Some("http://h/mcp/work")));
        assert!(mcp_url_is_read_only(None));
        assert!(mcp_url_is_read_only(Some("")));
    }

    #[test]
    fn copilot_builds_acp_launch() {
        let session = AcpSession::copilot(None);
        assert_eq!(session.program, DEFAULT_COPILOT_BIN);
        assert_eq!(session.args, vec!["--acp".to_string()]);
        assert!(session.read_only);
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
        assert!(session.args.contains(&CLAUDE_ACP_PACKAGE.to_string()));
        assert!(session.setup_hint.is_some());
    }

    #[test]
    fn claude_code_binary_override_runs_directly() {
        let session = AcpSession::claude_code(Some("/opt/claude-code-acp"));
        assert_eq!(session.program, "/opt/claude-code-acp");
        assert!(session.args.is_empty());
    }

    #[test]
    fn codex_launches_the_default_adapter_binary() {
        let session = AcpSession::codex(None);
        assert_eq!(session.program, DEFAULT_CODEX_ACP_BIN);
        assert!(session.args.is_empty());
        assert!(session.setup_hint.is_some());
    }

    #[test]
    fn codex_honors_binary_override() {
        assert_eq!(
            AcpSession::codex(Some("/opt/codex-acp")).program,
            "/opt/codex-acp"
        );
    }

    #[test]
    fn explain_appends_setup_hint_when_present() {
        let session = AcpSession::codex(None);
        let explained = session.explain(AgentError::MissingPipe("stdout"));
        let message = explained.to_string();
        assert!(message.contains("codex-acp"), "{message}");
    }

    #[test]
    fn explain_is_a_noop_without_a_hint() {
        let session = AcpSession::copilot(None);
        assert!(session.setup_hint.is_none());
        let explained = session.explain(AgentError::MissingPipe("stdout"));
        assert_eq!(
            explained.to_string(),
            AgentError::MissingPipe("stdout").to_string()
        );
    }

    #[test]
    fn with_mcp_derives_scope_from_url() {
        let ro = AcpSession::copilot(None).with_mcp(McpBinding::new("n", "http://h/mcp-ro/w"));
        assert!(ro.read_only);
        let rw = AcpSession::copilot(None).with_mcp(McpBinding::new("n", "http://h/mcp/w"));
        assert!(!rw.read_only);
    }

    #[test]
    fn absolute_cwd_is_always_absolute() {
        // No working dir: resolves against the process cwd.
        let default_cwd = AcpSession::copilot(None).absolute_cwd();
        assert!(PathBuf::from(&default_cwd).is_absolute(), "{default_cwd}");

        // An absolute working dir is preserved (canonicalization may resolve
        // symlinks, but the result stays absolute).
        let abs = AcpSession::copilot(None)
            .in_dir(Some(PathBuf::from("/")))
            .absolute_cwd();
        assert!(PathBuf::from(&abs).is_absolute(), "{abs}");
    }
}
