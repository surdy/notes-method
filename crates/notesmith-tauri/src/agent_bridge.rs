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
//! MCP transport: the desktop talks to its bundled **local** daemon over HTTP,
//! so the active vault is exposed to the agent as an HTTP MCP server
//! (`/mcp/<vault>` read-write, `/mcp-ro/<vault>` read-only) pointed at the same
//! daemon URL the rest of the app uses.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, mpsc, oneshot};

use notesmith_agent::{
    AcpSession, AgentEvent, AgentSession, EditorContext, McpBinding, ModelPicker,
    PermissionDecider, PermissionDecision, PermissionRequest,
};

/// Event channel carrying normalized [`AgentEvent`]s to the chat panel.
pub const AGENT_EVENT: &str = "notesmith://agent-event";
/// Event channel carrying write-permission prompts to the chat panel.
pub const AGENT_PERMISSION: &str = "notesmith://agent-permission";

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

/// Options for starting a chat session. Mirrors the frontend
/// `StartSessionOptions` (camelCase over the wire).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionOptions {
    vault: String,
    agent: String,
    read_only: bool,
    #[serde(default)]
    break_glass: bool,
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
struct PermissionRequestDto {
    tool: String,
    kind: Option<String>,
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

fn agent_catalog() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "copilot",
            "GitHub Copilot",
            notesmith_agent::DEFAULT_COPILOT_BIN.to_string(),
        ),
        (
            "claude",
            "Claude Code",
            // Claude is driven via an npx-launched ACP adapter; treat the node
            // package manager presence as the availability signal.
            "npx".to_string(),
        ),
        (
            "codex",
            "Codex",
            notesmith_agent::DEFAULT_CODEX_ACP_BIN.to_string(),
        ),
    ]
}

/// Build (but do not start) an [`AcpSession`] for `opts`, wired to the local
/// daemon's HTTP MCP endpoint and a UI permission decider.
fn build_session(
    app: &AppHandle,
    opts: &StartSessionOptions,
    session_id: &str,
    pending: Arc<PendingPermissions>,
) -> Result<AcpSession, String> {
    let mut session = match opts.agent.as_str() {
        "copilot" => AcpSession::copilot(None),
        "claude" => AcpSession::claude_code(None),
        "codex" => AcpSession::codex(None),
        other => return Err(format!("unknown agent '{other}'")),
    };

    // Scope the working directory (and any break-glass fs access) to the vault.
    if let Some(path) = vault_root(&opts.vault) {
        session = session.in_dir(Some(path));
    }

    // Expose the active vault over HTTP MCP against the local daemon, choosing
    // the read-only or read-write endpoint to match the requested scope.
    let base = crate::current_daemon_url(app);
    let base = base.trim_end_matches('/');
    let scope = if opts.read_only { "mcp-ro" } else { "mcp" };
    let url = format!("{base}/{scope}/{}", opts.vault);
    session = session.with_mcp(McpBinding::http("notesmith", url));

    let decider = Arc::new(BridgeDecider {
        app: app.clone(),
        session_id: session_id.to_string(),
        pending,
    });

    Ok(session
        .read_only(opts.read_only)
        .with_local_io(opts.break_glass)
        .with_permission_decider(decider))
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
    opts: StartSessionOptions,
) -> Result<(SessionEntry, Option<ModelPickerDto>), String> {
    let mut session = build_session(app, &opts, session_id, bridge.pending.clone())?;

    // Eagerly run the handshake so the model picker is available immediately.
    session.start().await.map_err(|e| e.to_string())?;
    let models = session.model_picker().as_ref().map(ModelPickerDto::from);

    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(run_session(
        session,
        rx,
        app.clone(),
        session_id.to_string(),
    ));

    Ok((SessionEntry { commands: tx, opts }, models))
}

#[tauri::command]
pub async fn agent_list() -> Result<Vec<AgentInfo>, String> {
    Ok(agent_catalog()
        .into_iter()
        .map(|(id, name, program)| AgentInfo {
            id: id.to_string(),
            name: name.to_string(),
            available: binary_on_path(&program),
        })
        .collect())
}

#[tauri::command]
pub async fn agent_start(
    app: AppHandle,
    bridge: tauri::State<'_, AgentBridge>,
    opts: StartSessionOptions,
) -> Result<StartSessionResult, String> {
    let session_id = format!(
        "sess-{}",
        bridge.next_session.fetch_add(1, Ordering::Relaxed)
    );

    let (entry, models) = spawn_session(&app, &bridge, &session_id, opts).await?;

    bridge
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), entry);

    Ok(StartSessionResult { session_id, models })
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
    bridge: tauri::State<'_, AgentBridge>,
    session_id: String,
    read_only: bool,
) -> Result<(), String> {
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
            return Ok(());
        }
        let _ = entry.commands.send(SessionCommand::Stop);
        entry.opts.clone()
    };
    opts.read_only = read_only;

    let (entry, _models) = spawn_session(&app, &bridge, &session_id, opts).await?;
    bridge.sessions.lock().await.insert(session_id, entry);
    Ok(())
}

#[tauri::command]
pub async fn agent_answer_permission(
    bridge: tauri::State<'_, AgentBridge>,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    let decision = match decision.as_str() {
        "allow_once" => PermissionDecision::AllowOnce,
        "allow_always" => PermissionDecision::AllowAlways,
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
