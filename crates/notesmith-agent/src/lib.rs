//! notesmith-agent: drive agent CLIs as a normalized event stream.
//!
//! This crate is the embedded agent-chat foundation described in
//! [ADR 0012](../docs/adr/0012-agent-transport-acp-mcp.md). It defines:
//!
//! - [`AgentEvent`] — the single normalized event model the UI renders.
//! - [`AgentSession`] — push user messages in, pull events out.
//! - [`AcpSession`] — the single Agent Client Protocol transport. One client,
//!   built on the official Zed [`agent_client_protocol`] crate, drives every
//!   supported agent: Copilot speaks ACP natively (`copilot --acp`); Claude
//!   Code and Codex are driven over the same protocol via small adapter
//!   binaries.
//!
//! The crate has no Tauri, HTTP or UI dependency: the desktop runner and the
//! headless CLI both build on this surface.

mod acp;
mod acp_client;
mod context;
mod diag_log;
mod error;
mod event;
mod mcp;
mod model;
mod permission;
mod registry;
mod session;
mod spawn_mcp;

pub use acp::{
    AcpSession, CLAUDE_ACP_PACKAGE, DEFAULT_CODEX_ACP_BIN, DEFAULT_COPILOT_BIN, DEFAULT_GEMINI_BIN,
};
pub use context::{EditorContext, VaultSummary};
pub use diag_log::{AgentDiagnosticsLog, DiagEntry, DiagKind};
pub use error::AgentError;
pub use event::{AgentEvent, ToolCall, ToolResult};
pub use mcp::{
    McpBinding, extra_mcp_bindings, load_mcp_config, server_name_for_namespaced_vault,
    server_name_for_vault,
};
pub use model::{ModelOption, ModelPicker};
pub use permission::{
    DenyAll, DiffPreview, PermissionDecider, PermissionDecision, PermissionRequest, PermissionState,
};
pub use registry::{AgentDescriptor, LaunchCandidate, builtin_registry, descriptor};
pub use session::AgentSession;
