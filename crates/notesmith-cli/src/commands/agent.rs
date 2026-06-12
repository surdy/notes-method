//! `notesmith agent` subcommands.
//!
//! Phase A of [ADR 0011](../../../../docs/adr/0011-embedded-agent-chat.md): a
//! headless driver for the `notesmith-agent` crate so adapters can be exercised
//! without the desktop UI. `notesmith agent run` spawns an agent CLI, sends one
//! user message, and streams the normalized event stream to stdout (as text or
//! JSON lines).

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use notesmith_agent::{AgentEvent, AgentSession, ClaudeCodeAdapter, ProcessAgentSession};

#[derive(Debug, Clone, ValueEnum)]
pub enum AgentKind {
    /// Anthropic Claude Code (`stream-json` transport).
    ClaudeCode,
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
    let adapter = match agent {
        AgentKind::ClaudeCode => match bin {
            Some(bin) => ClaudeCodeAdapter::new(bin),
            None => ClaudeCodeAdapter::default(),
        },
    };

    let mut session = ProcessAgentSession::spawn(adapter)?;
    session.send(message).await?;

    while let Some(event) = session.next_event().await {
        if json {
            println!("{}", serde_json::to_string(&event)?);
        } else {
            print_event(&event);
        }
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
