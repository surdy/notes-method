//! notesmith-agent: drive agent CLIs as a normalized event stream.
//!
//! This crate is the desktop-only embedded-agent-chat foundation described in
//! `docs/adr/0011-embedded-agent-chat.md`. It defines:
//!
//! - [`AgentEvent`] — the single normalized event model the UI renders.
//! - [`AgentSession`] — push user messages in, pull events out.
//! - [`AcpSession`] — the single Agent Client Protocol transport (ADR 0011
//!   Phase E): one JSON-RPC 2.0 client drives every supported agent. Copilot
//!   speaks ACP natively (`copilot --acp`); Claude Code and Codex are driven
//!   over the same protocol via small adapter binaries.
//!
//! The crate has no Tauri, HTTP or UI dependency: the desktop runner (Phase B)
//! and the headless `notesmith agent run` command both build on this surface.

mod acp;
mod acp_client;
mod error;
mod event;
mod mcp;
mod session;

pub use acp::{
    AcpSession, CLAUDE_ACP_PACKAGE, DEFAULT_CODEX_ACP_BIN, DEFAULT_COPILOT_BIN,
    mcp_url_is_read_only,
};
pub use error::AgentError;
pub use event::{AgentEvent, ToolCall, ToolResult};
pub use mcp::McpBinding;
pub use session::AgentSession;
