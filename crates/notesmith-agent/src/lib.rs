//! notesmith-agent: drive agent CLIs as a normalized event stream.
//!
//! This crate is the desktop-only embedded-agent-chat foundation described in
//! `docs/adr/0011-embedded-agent-chat.md`. It defines:
//!
//! - [`AgentEvent`] — the single normalized event model the UI renders.
//! - [`LineAdapter`] — converts one agent CLI's streaming output into events.
//!   [`ClaudeCodeAdapter`] is the first implementation (Claude Code
//!   `stream-json`).
//! - [`AgentSession`] — push user messages in, pull events out;
//!   [`ProcessAgentSession`] spawns an agent CLI and streams its stdout.
//! - [`AcpSession`] — the single Agent Client Protocol transport (ADR 0011
//!   Phase E): one JSON-RPC 2.0 client drives any ACP-speaking agent
//!   (Copilot CLI natively via `copilot --acp`).
//!
//! The crate has no Tauri, HTTP or UI dependency: the desktop runner (Phase B)
//! and the headless `notesmith agent run` command both build on this surface.

mod acp;
mod adapter;
mod claude_code;
mod codex;
mod copilot_cli;
mod error;
mod event;
mod mcp;
mod session;

pub use acp::{
    AcpSession, CLAUDE_ACP_PACKAGE, DEFAULT_CODEX_ACP_BIN, DEFAULT_COPILOT_BIN,
    mcp_url_is_read_only,
};
pub use adapter::{Launch, LineAdapter, PromptDelivery};
pub use claude_code::{ClaudeCodeAdapter, DEFAULT_BIN};
pub use codex::CodexAdapter;
pub use copilot_cli::CopilotCliAdapter;
pub use error::AgentError;
pub use event::{AgentEvent, ToolCall, ToolResult};
pub use mcp::McpBinding;
pub use session::{AgentSession, OneShotProcessSession, ProcessAgentSession, drive_lines};
