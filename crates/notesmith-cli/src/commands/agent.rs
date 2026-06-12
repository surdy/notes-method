//! `notesmith agent` subcommands.
//!
//! Phase A of [ADR 0011](../../../../docs/adr/0011-embedded-agent-chat.md): a
//! headless driver for the `notesmith-agent` crate so the agent transport can
//! be exercised without the desktop UI. `notesmith agent run` starts an agent
//! over the Agent Client Protocol (ADR 0011 Phase E — the single transport),
//! sends one user message, and streams the normalized event stream to stdout
//! (as text or JSON lines).

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use notesmith_agent::{AcpSession, AgentEvent, AgentSession};

#[derive(Debug, Clone, ValueEnum)]
pub enum AgentKind {
    /// Anthropic Claude Code over ACP (`npx @zed-industries/claude-code-acp`).
    ClaudeCode,
    /// OpenAI Codex over ACP (the `codex-acp` adapter binary).
    Codex,
    /// GitHub Copilot over ACP (`copilot --acp`, native).
    Copilot,
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Run a headless agent turn and stream its normalized events.
    Run {
        /// The user message to send to the agent.
        message: String,

        /// Which agent to drive (all over the Agent Client Protocol).
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
    let mut session = match agent {
        AgentKind::ClaudeCode => AcpSession::claude_code(bin),
        AgentKind::Codex => AcpSession::codex(bin),
        AgentKind::Copilot => AcpSession::copilot(bin),
    };
    session.send(message).await?;
    stream_until_done(&mut session, json).await
}

/// Stream events for a single turn of a **persistent** ACP session, stopping at
/// the terminal [`AgentEvent::Done`]/[`AgentEvent::Error`]. An ACP process stays
/// alive between turns, so the headless command must stop itself after one turn
/// rather than waiting for process EOF.
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
