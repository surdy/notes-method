//! Desktop agent runner (ADR 0011 Phase B).
//!
//! Spawns an agent CLI as a local subprocess via the [`notesmith_agent`] crate
//! and bridges its normalized [`AgentEvent`] stream to the SvelteKit frontend
//! over Tauri IPC. There is no network transport: the runner is desktop-local
//! and uses the user's existing local CLI credentials. Server/hosted agent
//! access stays MCP-only (ADR 0011).
//!
//! Lifecycle:
//! - [`agent_start`] resolves the active vault's directory, spawns the agent in
//!   it, registers an input channel, and drives a per-session task that pumps
//!   user messages in and emits events out.
//! - [`agent_send`] forwards a user message to a running session.
//! - [`agent_stop`] drops the session's input channel, which ends the driver
//!   loop and kills the child process (no orphans, see
//!   [`notesmith_agent::ProcessAgentSession`]'s `Drop`).
//!
//! Each emitted [`AgentEvent`] is wrapped in an [`AgentEventEnvelope`] tagged
//! with the session id and broadcast as `notesmith://agent-event`; when a
//! session ends, `notesmith://agent-ended` is emitted with the session id.
//! Frontends filter by the session id returned from [`agent_start`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use notesmith_agent::{
    AcpSession, AgentError, AgentEvent, AgentSession, ClaudeCodeAdapter, CodexAdapter,
    CopilotCliAdapter, Launch, LineAdapter, McpBinding, OneShotProcessSession, ProcessAgentSession,
};
use notesmith_config::GlobalConfig;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::sync::mpsc;

/// Event name carrying a single normalized agent event to the frontend.
pub const AGENT_EVENT: &str = "notesmith://agent-event";
/// Event name signalling that a session's process has ended.
pub const AGENT_ENDED: &str = "notesmith://agent-ended";

/// Which agent CLI to drive. Mirrors the `notesmith agent run` CLI surface.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind {
    /// Anthropic Claude Code (`stream-json` transport).
    ClaudeCode,
    /// OpenAI Codex (`codex exec --json`, single-shot).
    Codex,
    /// GitHub Copilot CLI (`copilot -p`, single-shot plain text).
    CopilotCli,
    /// GitHub Copilot CLI over the Agent Client Protocol (`copilot --acp`),
    /// multi-turn (ADR 0011 Phase E).
    CopilotAcp,
}

/// A line adapter dispatching to one of the supported agents.
///
/// Keeping a single concrete adapter type lets both session variants stay
/// monomorphic (the [`AgentSession`] trait is not dyn-compatible).
#[derive(Clone)]
enum NotesmithAdapter {
    ClaudeCode(ClaudeCodeAdapter),
    Codex(CodexAdapter),
    CopilotCli(CopilotCliAdapter),
}

impl LineAdapter for NotesmithAdapter {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        match self {
            NotesmithAdapter::ClaudeCode(a) => a.parse_line(line),
            NotesmithAdapter::Codex(a) => a.parse_line(line),
            NotesmithAdapter::CopilotCli(a) => a.parse_line(line),
        }
    }

    fn encode_user_message(&self, text: &str) -> Vec<u8> {
        match self {
            NotesmithAdapter::ClaudeCode(a) => a.encode_user_message(text),
            NotesmithAdapter::Codex(a) => a.encode_user_message(text),
            NotesmithAdapter::CopilotCli(a) => a.encode_user_message(text),
        }
    }

    fn command(&self) -> (String, Vec<String>) {
        match self {
            NotesmithAdapter::ClaudeCode(a) => a.command(),
            NotesmithAdapter::Codex(a) => a.command(),
            NotesmithAdapter::CopilotCli(a) => a.command(),
        }
    }

    fn launch(&self) -> Launch {
        match self {
            NotesmithAdapter::ClaudeCode(a) => a.launch(),
            NotesmithAdapter::Codex(a) => a.launch(),
            NotesmithAdapter::CopilotCli(a) => a.launch(),
        }
    }

    fn command_for_prompt(&self, prompt: &str) -> (String, Vec<String>) {
        match self {
            NotesmithAdapter::ClaudeCode(a) => a.command_for_prompt(prompt),
            NotesmithAdapter::Codex(a) => a.command_for_prompt(prompt),
            NotesmithAdapter::CopilotCli(a) => a.command_for_prompt(prompt),
        }
    }
}

/// A running session: streaming (Claude Code), single-shot (Codex/Copilot CLI),
/// or persistent ACP (Copilot `--acp`, ADR 0011 Phase E).
enum DriverSession {
    Streaming(ProcessAgentSession<NotesmithAdapter>),
    OneShot(OneShotProcessSession<NotesmithAdapter>),
    Acp(AcpSession),
}

impl DriverSession {
    async fn send(&mut self, message: &str) -> Result<(), AgentError> {
        match self {
            DriverSession::Streaming(session) => session.send(message).await,
            DriverSession::OneShot(session) => session.send(message).await,
            DriverSession::Acp(session) => session.send(message).await,
        }
    }

    async fn next_event(&mut self) -> Option<AgentEvent> {
        match self {
            DriverSession::Streaming(session) => session.next_event().await,
            DriverSession::OneShot(session) => session.next_event().await,
            DriverSession::Acp(session) => session.next_event().await,
        }
    }
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

/// Build the line adapter for the requested agent, honoring a binary override
/// and an optional MCP endpoint to auto-wire (ADR 0011 Phase C).
fn build_adapter(kind: &AgentKind, bin: Option<&str>, mcp_url: Option<&str>) -> NotesmithAdapter {
    let mcp = match mcp_url {
        Some(url) if !url.is_empty() => Some(McpBinding::new(MCP_SERVER_NAME, url)),
        _ => None,
    };
    let bin = bin.filter(|b| !b.is_empty());
    match kind {
        AgentKind::ClaudeCode => {
            let mut adapter = match bin {
                Some(bin) => ClaudeCodeAdapter::new(bin),
                None => ClaudeCodeAdapter::default(),
            };
            if let Some(binding) = mcp {
                adapter = adapter.with_mcp(binding);
            }
            NotesmithAdapter::ClaudeCode(adapter)
        }
        AgentKind::Codex => {
            let mut adapter = match bin {
                Some(bin) => CodexAdapter::new(bin),
                None => CodexAdapter::default(),
            };
            if let Some(binding) = mcp {
                adapter = adapter.with_mcp(binding);
            }
            NotesmithAdapter::Codex(adapter)
        }
        // `CopilotAcp` is routed to an `AcpSession` in `agent_start` and never
        // reaches this line-adapter builder; fold it into the Copilot CLI arm
        // (same binary) as a non-panicking defensive fallback.
        AgentKind::CopilotCli | AgentKind::CopilotAcp => {
            let mut adapter = match bin {
                Some(bin) => CopilotCliAdapter::new(bin),
                None => CopilotCliAdapter::default(),
            };
            if let Some(binding) = mcp {
                adapter = adapter.with_mcp(binding);
            }
            NotesmithAdapter::CopilotCli(adapter)
        }
    }
}

/// Build a persistent ACP session for the active vault (ADR 0011 Phase E).
///
/// Currently the only ACP-wired agent is the Copilot CLI (`copilot --acp`);
/// Claude Code and Codex join via their ACP adapter binaries in Phase E2. The
/// MCP endpoint is passed via the ACP `session/new` `mcpServers` param, and the
/// read-only / read-write scope is derived from the endpoint URL.
fn build_acp_session(
    bin: Option<&str>,
    mcp_url: Option<&str>,
    working_dir: Option<PathBuf>,
) -> AcpSession {
    let bin = bin.filter(|b| !b.is_empty());
    let session = AcpSession::copilot(bin).in_dir(working_dir);
    match mcp_url {
        Some(url) if !url.is_empty() => session.with_mcp(McpBinding::new(MCP_SERVER_NAME, url)),
        _ => session,
    }
}

/// Spawn the appropriate session variant for `adapter`'s launch strategy.
fn spawn_session(
    adapter: NotesmithAdapter,
    working_dir: Option<PathBuf>,
) -> Result<DriverSession, AgentError> {
    match adapter.launch() {
        Launch::Streaming => Ok(DriverSession::Streaming(ProcessAgentSession::spawn_in(
            adapter,
            working_dir,
        )?)),
        Launch::OneShot(_) => Ok(DriverSession::OneShot(OneShotProcessSession::new(
            adapter,
            working_dir,
        ))),
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

    let session = match agent {
        AgentKind::CopilotAcp => DriverSession::Acp(build_acp_session(
            bin.as_deref(),
            mcp_url.as_deref(),
            working_dir,
        )),
        _ => {
            let adapter = build_adapter(&agent, bin.as_deref(), mcp_url.as_deref());
            spawn_session(adapter, working_dir)
                .map_err(|error| format!("could not start agent: {error}"))?
        }
    };

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
    mut session: DriverSession,
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
    fn claude_code_kind_deserializes_from_kebab_case() {
        let kind: AgentKind = serde_json::from_str("\"claude-code\"").unwrap();
        assert_eq!(kind, AgentKind::ClaudeCode);
    }

    #[test]
    fn codex_and_copilot_kinds_deserialize_from_kebab_case() {
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"codex\"").unwrap(),
            AgentKind::Codex
        );
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"copilot-cli\"").unwrap(),
            AgentKind::CopilotCli
        );
    }

    #[test]
    fn copilot_acp_kind_deserializes_from_kebab_case() {
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"copilot-acp\"").unwrap(),
            AgentKind::CopilotAcp
        );
    }

    #[test]
    fn build_acp_session_constructs_without_mcp() {
        // Smoke test: building the session must not spawn a process or panic.
        let _ = build_acp_session(None, None, None);
        let _ = build_acp_session(Some("/opt/copilot"), Some(""), None);
    }

    #[test]
    fn build_acp_session_wires_mcp_when_url_provided() {
        let _ = build_acp_session(None, Some("http://127.0.0.1:27183/mcp-ro/work"), None);
    }

    #[test]
    fn build_adapter_dispatches_to_each_agent() {
        use notesmith_agent::LineAdapter;
        assert!(matches!(
            build_adapter(&AgentKind::ClaudeCode, None, None),
            NotesmithAdapter::ClaudeCode(_)
        ));
        assert!(matches!(
            build_adapter(&AgentKind::Codex, None, None),
            NotesmithAdapter::Codex(_)
        ));
        assert!(matches!(
            build_adapter(&AgentKind::CopilotCli, None, None),
            NotesmithAdapter::CopilotCli(_)
        ));
        // Codex reads its prompt from stdin via the trailing `-`.
        let (_, codex_args) = build_adapter(&AgentKind::Codex, None, None).command();
        assert_eq!(codex_args.last().map(String::as_str), Some("-"));
    }

    #[test]
    fn build_adapter_launch_strategy_matches_agent() {
        use notesmith_agent::{Launch, LineAdapter, PromptDelivery};
        assert_eq!(
            build_adapter(&AgentKind::ClaudeCode, None, None).launch(),
            Launch::Streaming
        );
        assert_eq!(
            build_adapter(&AgentKind::Codex, None, None).launch(),
            Launch::OneShot(PromptDelivery::Stdin)
        );
        assert_eq!(
            build_adapter(&AgentKind::CopilotCli, None, None).launch(),
            Launch::OneShot(PromptDelivery::Arg)
        );
    }

    #[test]
    fn build_adapter_wires_mcp_for_codex_via_config_override() {
        use notesmith_agent::LineAdapter;
        let adapter = build_adapter(
            &AgentKind::Codex,
            None,
            Some("http://127.0.0.1:27183/mcp/work"),
        );
        let (_, args) = adapter.command();
        let idx = args.iter().position(|a| a == "-c").expect("-c present");
        assert!(args[idx + 1].contains("mcp_servers.notesmith.url"));
    }

    #[test]
    fn build_adapter_uses_default_binary_without_override() {
        let adapter = build_adapter(&AgentKind::ClaudeCode, None, None);
        let (program, _) = {
            use notesmith_agent::LineAdapter;
            adapter.command()
        };
        assert_eq!(program, notesmith_agent::DEFAULT_BIN);
    }

    #[test]
    fn build_adapter_honors_binary_override() {
        let adapter = build_adapter(&AgentKind::ClaudeCode, Some("/opt/claude"), None);
        let (program, _) = {
            use notesmith_agent::LineAdapter;
            adapter.command()
        };
        assert_eq!(program, "/opt/claude");
    }

    #[test]
    fn build_adapter_ignores_empty_binary_override() {
        let adapter = build_adapter(&AgentKind::ClaudeCode, Some(""), None);
        let (program, _) = {
            use notesmith_agent::LineAdapter;
            adapter.command()
        };
        assert_eq!(program, notesmith_agent::DEFAULT_BIN);
    }

    #[test]
    fn build_adapter_wires_mcp_when_url_provided() {
        use notesmith_agent::LineAdapter;
        let adapter = build_adapter(
            &AgentKind::ClaudeCode,
            None,
            Some("http://127.0.0.1:27183/mcp-ro/work"),
        );
        let (_, args) = adapter.command();
        let idx = args
            .iter()
            .position(|a| a == "--mcp-config")
            .expect("--mcp-config present");
        assert!(args[idx + 1].contains("http://127.0.0.1:27183/mcp-ro/work"));
        assert!(args[idx + 1].contains("notesmith"));
        assert!(args.iter().any(|a| a == "--strict-mcp-config"));
    }

    #[test]
    fn build_adapter_skips_mcp_for_empty_url() {
        use notesmith_agent::LineAdapter;
        let adapter = build_adapter(&AgentKind::ClaudeCode, None, Some(""));
        let (_, args) = adapter.command();
        assert!(!args.iter().any(|a| a == "--mcp-config"));
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

    #[test]
    fn allocate_id_is_monotonic_and_unique() {
        let mut registry = SessionRegistry::default();
        let first = registry.allocate_id();
        let second = registry.allocate_id();
        assert_eq!(first, "agent-1");
        assert_eq!(second, "agent-2");
        assert_ne!(first, second);
    }

    #[test]
    fn envelope_flattens_event_under_session_id() {
        let envelope = AgentEventEnvelope {
            session_id: "agent-1".to_string(),
            event: AgentEvent::Status {
                message: "ready".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_value(&envelope).unwrap(),
            serde_json::json!({
                "session_id": "agent-1",
                "type": "status",
                "message": "ready"
            })
        );
    }
}
