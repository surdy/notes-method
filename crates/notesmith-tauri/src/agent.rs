//! Desktop agent runner (ADR 0011 Phase B).
//!
//! Drives an agent over the Agent Client Protocol (ADR 0011 Phase E — the
//! single transport) via the [`notesmith_agent`] crate and bridges its
//! normalized [`AgentEvent`] stream to the SvelteKit frontend over Tauri IPC.
//! There is no network transport: the runner is desktop-local and uses the
//! user's existing local CLI credentials. Server/hosted agent access stays
//! MCP-only (ADR 0011).
//!
//! Lifecycle:
//! - [`agent_start`] resolves the active vault's directory, builds an
//!   [`AcpSession`] for it, registers an input channel, and drives a per-session
//!   task that pumps user messages in and emits events out.
//! - [`agent_send`] forwards a user message to a running session.
//! - [`agent_stop`] drops the session's input channel, which ends the driver
//!   loop and terminates the agent process (no orphans, see [`AcpSession`]'s
//!   `Drop`).
//!
//! Each emitted [`AgentEvent`] is wrapped in an [`AgentEventEnvelope`] tagged
//! with the session id and broadcast as `notesmith://agent-event`; when a
//! session ends, `notesmith://agent-ended` is emitted with the session id.
//! Frontends filter by the session id returned from [`agent_start`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use notesmith_agent::{AcpSession, AgentEvent, AgentSession, McpBinding};
use notesmith_config::GlobalConfig;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::sync::mpsc;

/// Event name carrying a single normalized agent event to the frontend.
pub const AGENT_EVENT: &str = "notesmith://agent-event";
/// Event name signalling that a session's process has ended.
pub const AGENT_ENDED: &str = "notesmith://agent-ended";

/// Which agent to drive. Mirrors the `notesmith agent run` CLI surface. All
/// agents are driven over the Agent Client Protocol (ADR 0011 Phase E).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind {
    /// Anthropic Claude Code over ACP (`npx @zed-industries/claude-code-acp`).
    ClaudeCode,
    /// OpenAI Codex over ACP (the `codex-acp` adapter binary).
    Codex,
    /// GitHub Copilot over ACP (`copilot --acp`, native).
    Copilot,
}

/// A normalized agent event tagged with the session it belongs to.
#[derive(Debug, Clone, Serialize)]
pub struct AgentEventEnvelope {
    /// Identifier returned by [`agent_start`].
    pub session_id: String,
    /// The normalized event.
    #[serde(flatten)]
    pub event: AgentEvent,
}

/// Signal that a session ended (process exited or was stopped).
#[derive(Debug, Clone, Serialize)]
pub struct AgentEndedEnvelope {
    /// Identifier of the session that ended.
    pub session_id: String,
}

/// Registry of live sessions: maps a session id to the channel feeding its
/// driver task. Dropping a sender stops the corresponding session.
#[derive(Default)]
struct SessionRegistry {
    next_id: u64,
    sessions: HashMap<String, mpsc::UnboundedSender<String>>,
}

impl SessionRegistry {
    fn allocate_id(&mut self) -> String {
        self.next_id += 1;
        format!("agent-{}", self.next_id)
    }
}

/// Managed Tauri state holding all running agent sessions.
#[derive(Default)]
pub struct AgentSessions(Mutex<SessionRegistry>);

/// Resolve the working directory the agent should run in for `vault`.
///
/// Prefers a registered vault path; falls back to treating `vault` as an
/// absolute directory path. Returns `None` when nothing resolves, in which case
/// the agent runs in the process's default working directory.
fn vault_working_dir(config: &GlobalConfig, vault: &str) -> Option<PathBuf> {
    if vault.is_empty() {
        return None;
    }
    if let Some(registration) = config.vault(vault) {
        return Some(registration.path.clone());
    }
    let candidate = PathBuf::from(vault);
    if candidate.is_absolute() && candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

/// MCP server name exposed to spawned agents for the active vault.
const MCP_SERVER_NAME: &str = "notesmith";

/// Build a persistent ACP session for the active vault (ADR 0011 Phase E).
///
/// Copilot speaks ACP natively (`copilot --acp`); Claude Code and Codex are
/// driven via their ACP adapter binaries, which carry a setup hint so a missing
/// adapter surfaces actionable guidance. The MCP endpoint is passed via the ACP
/// `session/new` `mcpServers` param, and the read-only / read-write scope is
/// derived from the endpoint URL. `local_io` grants the agent scoped filesystem
/// and terminal access to the vault directory (ADR 0012, off by default).
fn build_acp_session(
    kind: &AgentKind,
    bin: Option<&str>,
    mcp_url: Option<&str>,
    working_dir: Option<PathBuf>,
    local_io: bool,
) -> AcpSession {
    let bin = bin.filter(|b| !b.is_empty());
    let session = match kind {
        AgentKind::ClaudeCode => AcpSession::claude_code(bin),
        AgentKind::Codex => AcpSession::codex(bin),
        AgentKind::Copilot => AcpSession::copilot(bin),
    }
    .in_dir(working_dir)
    .with_local_io(local_io);
    match mcp_url {
        Some(url) if !url.is_empty() => session.with_mcp(McpBinding::new(MCP_SERVER_NAME, url)),
        _ => session,
    }
}

/// Start an agent session for `vault` and stream its events to the frontend.
///
/// Returns the session id the frontend uses to correlate events and to send /
/// stop the session.
#[tauri::command]
pub async fn agent_start<R: Runtime>(
    app: AppHandle<R>,
    sessions: State<'_, AgentSessions>,
    vault: String,
    agent: AgentKind,
    bin: Option<String>,
    mcp_url: Option<String>,
) -> Result<String, String> {
    let config = GlobalConfig::load().unwrap_or_default();
    let working_dir = vault_working_dir(&config, &vault);
    let local_io = config.agent.local_file_access;

    let session = build_acp_session(
        &agent,
        bin.as_deref(),
        mcp_url.as_deref(),
        working_dir,
        local_io,
    );

    let (input_tx, input_rx) = mpsc::unbounded_channel::<String>();
    let session_id = {
        let mut registry = sessions.0.lock().map_err(|_| "session registry poisoned")?;
        let id = registry.allocate_id();
        registry.sessions.insert(id.clone(), input_tx);
        id
    };

    let task_app = app.clone();
    let task_id = session_id.clone();
    tauri::async_runtime::spawn(async move {
        drive_session(task_app, task_id, session, input_rx).await;
    });

    Ok(session_id)
}

/// Send a user message to a running session.
#[tauri::command]
pub async fn agent_send(
    sessions: State<'_, AgentSessions>,
    session_id: String,
    message: String,
) -> Result<(), String> {
    let sender = {
        let registry = sessions.0.lock().map_err(|_| "session registry poisoned")?;
        registry.sessions.get(&session_id).cloned()
    };
    match sender {
        Some(sender) => sender
            .send(message)
            .map_err(|_| "agent session is no longer running".to_string()),
        None => Err("no such agent session".to_string()),
    }
}

/// Stop a running session, terminating its child process.
#[tauri::command]
pub async fn agent_stop(
    sessions: State<'_, AgentSessions>,
    session_id: String,
) -> Result<(), String> {
    let mut registry = sessions.0.lock().map_err(|_| "session registry poisoned")?;
    // Dropping the sender closes the input channel; the driver loop then ends
    // and the session is dropped, killing the child process.
    registry.sessions.remove(&session_id);
    Ok(())
}

/// Pump user messages into `session` and emit its events until the input
/// channel closes or the agent process ends. Always emits a final
/// [`AGENT_ENDED`] and removes itself from the registry on exit.
async fn drive_session<R: Runtime>(
    app: AppHandle<R>,
    session_id: String,
    mut session: AcpSession,
    mut input_rx: mpsc::UnboundedReceiver<String>,
) {
    loop {
        tokio::select! {
            incoming = input_rx.recv() => match incoming {
                Some(message) => {
                    emit_event(&app, &session_id, AgentEvent::UserMessage { text: message.clone() });
                    if let Err(error) = session.send(&message).await {
                        emit_event(
                            &app,
                            &session_id,
                            AgentEvent::Error { message: format!("failed to send message: {error}") },
                        );
                    }
                }
                None => break,
            },
            event = session.next_event() => match event {
                Some(event) => emit_event(&app, &session_id, event),
                None => break,
            },
        }
    }

    if let Ok(mut registry) = app.state::<AgentSessions>().0.lock() {
        registry.sessions.remove(&session_id);
    }
    let _ = app.emit(
        AGENT_ENDED,
        AgentEndedEnvelope {
            session_id: session_id.clone(),
        },
    );
}

fn emit_event<R: Runtime>(app: &AppHandle<R>, session_id: &str, event: AgentEvent) {
    let envelope = AgentEventEnvelope {
        session_id: session_id.to_string(),
        event,
    };
    if let Err(error) = app.emit(AGENT_EVENT, envelope) {
        tracing::warn!(session = session_id, "failed to emit agent event: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_config::VaultRegistration;

    #[test]
    fn agent_kinds_deserialize_from_kebab_case() {
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"claude-code\"").unwrap(),
            AgentKind::ClaudeCode
        );
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"codex\"").unwrap(),
            AgentKind::Codex
        );
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"copilot\"").unwrap(),
            AgentKind::Copilot
        );
    }

    #[test]
    fn build_acp_session_constructs_each_kind_without_spawning() {
        // Smoke test: building the session must not spawn a process or panic.
        let _ = build_acp_session(&AgentKind::Copilot, None, None, None, false);
        let _ = build_acp_session(
            &AgentKind::ClaudeCode,
            Some("/opt/claude-acp"),
            Some(""),
            None,
            false,
        );
        let _ = build_acp_session(&AgentKind::Codex, None, None, None, false);
    }

    #[test]
    fn build_acp_session_wires_mcp_when_url_provided() {
        let _ = build_acp_session(
            &AgentKind::Copilot,
            None,
            Some("http://127.0.0.1:27183/mcp-ro/work"),
            None,
            false,
        );
    }

    #[test]
    fn build_acp_session_honors_binary_override() {
        let _ = build_acp_session(&AgentKind::Copilot, Some("/opt/copilot"), None, None, false);
    }

    #[test]
    fn build_acp_session_grants_local_io_when_enabled() {
        // Smoke test: enabling local I/O must not spawn or panic at build time.
        let _ = build_acp_session(&AgentKind::Copilot, None, None, None, true);
    }

    #[test]
    fn vault_working_dir_prefers_registered_path() {
        let mut config = GlobalConfig::default();
        config.vaults.insert(
            "notes".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/notes"),
            },
        );
        assert_eq!(
            vault_working_dir(&config, "notes"),
            Some(PathBuf::from("/vaults/notes"))
        );
    }

    #[test]
    fn vault_working_dir_is_none_for_unknown_relative_vault() {
        let config = GlobalConfig::default();
        assert_eq!(vault_working_dir(&config, "not-a-registered-name"), None);
    }

    #[test]
    fn vault_working_dir_is_none_for_empty_vault() {
        let config = GlobalConfig::default();
        assert_eq!(vault_working_dir(&config, ""), None);
    }
}
