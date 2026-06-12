//! `notesmith agent` subcommands.
//!
//! Phase A of [ADR 0011](../../../../docs/adr/0011-embedded-agent-chat.md): a
//! headless driver for the `notesmith-agent` crate so adapters can be exercised
//! without the desktop UI. `notesmith agent run` spawns an agent CLI, sends one
//! user message, and streams the normalized event stream to stdout (as text or
//! JSON lines).

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use notesmith_agent::{
    AcpSession, AgentEvent, AgentSession, ClaudeCodeAdapter, CodexAdapter, CopilotCliAdapter,
    Launch, LineAdapter, OneShotProcessSession, ProcessAgentSession,
};

#[derive(Debug, Clone, ValueEnum)]
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
    /// Claude Code over ACP via its adapter binary (`npx @zed-industries/claude-code-acp`).
    ClaudeAcp,
    /// Codex over ACP via the `codex-acp` adapter binary.
    CodexAcp,
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Run a headless agent turn and stream its normalized events.
    Run {
        /// The user message to send to the agent.
        message: String,

        /// Which agent CLI to drive.
        #[arg(long, value_enum, default_value = "claude-code")]
        agent: AgentKind,

        /// Override the agent binary (path or name on PATH).
        #[arg(long)]
        bin: Option<String>,

        /// Emit each event as a JSON line instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

impl AgentCommand {
    pub async fn run(&self) -> Result<()> {
        match self {
            AgentCommand::Run {
                message,
                agent,
                bin,
                json,
            } => cmd_run(message, agent, bin.as_deref(), *json).await,
        }
    }
}

async fn cmd_run(message: &str, agent: &AgentKind, bin: Option<&str>, json: bool) -> Result<()> {
    match agent {
        AgentKind::ClaudeCode => {
            let adapter = match bin {
                Some(bin) => ClaudeCodeAdapter::new(bin),
                None => ClaudeCodeAdapter::default(),
            };
            drive(adapter, message, json).await
        }
        AgentKind::Codex => {
            let adapter = match bin {
                Some(bin) => CodexAdapter::new(bin),
                None => CodexAdapter::default(),
            };
            drive(adapter, message, json).await
        }
        AgentKind::CopilotCli => {
            let adapter = match bin {
                Some(bin) => CopilotCliAdapter::new(bin),
                None => CopilotCliAdapter::default(),
            };
            drive(adapter, message, json).await
        }
        AgentKind::CopilotAcp => {
            let mut session = AcpSession::copilot(bin);
            session.send(message).await?;
            stream_until_done(&mut session, json).await
        }
        AgentKind::ClaudeAcp => {
            let mut session = AcpSession::claude_code(bin);
            session.send(message).await?;
            stream_until_done(&mut session, json).await
        }
        AgentKind::CodexAcp => {
            let mut session = AcpSession::codex(bin);
            session.send(message).await?;
            stream_until_done(&mut session, json).await
        }
    }
}

/// Spawn the right session variant for `adapter`, send `message`, and stream the
/// resulting events to stdout.
async fn drive<A: LineAdapter + Clone + 'static>(
    adapter: A,
    message: &str,
    json: bool,
) -> Result<()> {
    match adapter.launch() {
        Launch::Streaming => {
            let mut session = ProcessAgentSession::spawn(adapter)?;
            session.send(message).await?;
            stream(&mut session, json).await
        }
        Launch::OneShot(_) => {
            let mut session = OneShotProcessSession::new(adapter, None);
            session.send(message).await?;
            stream(&mut session, json).await
        }
    }
}

async fn stream<S: AgentSession>(session: &mut S, json: bool) -> Result<()> {
    while let Some(event) = session.next_event().await {
        emit(&event, json)?;
    }
    Ok(())
}

/// Stream events for a single turn of a **persistent** session (ACP), stopping
/// at the terminal [`AgentEvent::Done`]/[`AgentEvent::Error`]. Unlike the
/// single-shot agents, an ACP process stays alive between turns, so the headless
/// command must stop itself after one turn rather than waiting for process EOF.
async fn stream_until_done<S: AgentSession>(session: &mut S, json: bool) -> Result<()> {
    while let Some(event) = session.next_event().await {
        let terminal = matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. });
        emit(&event, json)?;
        if terminal {
            break;
        }
    }
    Ok(())
}

fn emit(event: &AgentEvent, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(event)?);
    } else {
        print_event(event);
    }
    Ok(())
}

fn print_event(event: &AgentEvent) {
    match event {
        AgentEvent::UserMessage { text } => println!("> {text}"),
        AgentEvent::AgentMessageDelta { text } => print!("{text}"),
        AgentEvent::ToolCall(call) => {
            println!("\n[tool] {}({})", call.name, call.args);
        }
        AgentEvent::ToolResult(result) => {
            let tag = if result.is_error {
                "tool error"
            } else {
                "tool"
            };
            println!("[{tag}] {}", result.content);
        }
        AgentEvent::Status { message } => println!("[status] {message}"),
        AgentEvent::Done { result } => {
            if let Some(result) = result {
                println!("\n[done] {result}");
            } else {
                println!("\n[done]");
            }
        }
        AgentEvent::Error { message } => eprintln!("[error] {message}"),
    }
}
