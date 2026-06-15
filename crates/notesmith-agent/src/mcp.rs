//! MCP wiring for ACP sessions (ADR 0012).
//!
//! The active vault is exposed to a spawned agent as a `session/new`
//! `mcpServers` entry so it can read (and, when read-write, edit) the vault the
//! user is viewing. Two transports carry the same per-vault MCP server, and the
//! one advertised to a given agent is chosen from its declared
//! `mcpCapabilities` during the ACP handshake (see [`crate::acp::AcpSession`]):
//!
//! - **HTTP(S)** (preferred): the agent connects to the daemon's Streamable
//!   HTTP MCP endpoint (`/mcp/<vault>` or `/mcp-ro/<vault>`) directly. Every
//!   HTTP-capable agent uses this — including GitHub Copilot, whose ACP client
//!   supports *only* HTTP/SSE MCP and silently ignores stdio servers.
//! - **stdio** (local fallback): Notesmith launches the `notesmith mcp start`
//!   bridge as a child process; the bridge forwards every request to the
//!   daemon's HTTP MCP endpoint, so stdio and HTTP clients share the daemon's
//!   live indexes (ADR 0010 Phase 3). Supplied only for a **local** daemon and
//!   used only when the agent does not advertise HTTP MCP support.
//!
//! Read-only vs read-write scope is carried explicitly on the binding so the
//! permission gate matches the endpoint the agent actually talks to (the
//! daemon encodes the scope in the HTTP path and in the bridge's `--read-only`
//! flag).

use std::path::PathBuf;

use agent_client_protocol::schema::{McpServer, McpServerHttp, McpServerStdio};

/// Server name surfaced to the agent for the vault's MCP server.
pub const MCP_SERVER_NAME: &str = "notesmith";

/// How a spawned agent reaches the active vault's MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpBinding {
    /// A stdio bridge subprocess (`command args...`, typically
    /// `notesmith --vault <vault> mcp start [--read-only]`). Used as the local
    /// fallback for agents that do not support HTTP MCP.
    Stdio {
        /// Server name surfaced to the agent.
        name: String,
        /// Executable to launch (path or PATH-resolved name).
        command: String,
        /// Arguments passed to `command`.
        args: Vec<String>,
        /// Whether the bridge targets the read-only scope.
        read_only: bool,
    },
    /// A Streamable HTTP(S) MCP endpoint. Preferred when the agent advertises
    /// `mcpCapabilities.http`.
    Http {
        /// Server name surfaced to the agent.
        name: String,
        /// Streamable HTTP MCP endpoint URL (already scope-resolved).
        url: String,
        /// Whether the endpoint targets the read-only scope.
        read_only: bool,
    },
}

impl McpBinding {
    /// Build an HTTP(S) binding for `name` at `url`. The read-only scope
    /// is derived from the endpoint path: `/mcp-ro/` denotes read-only.
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        let url = url.into();
        let read_only = url.contains("/mcp-ro/");
        Self::Http {
            name: name.into(),
            url,
            read_only,
        }
    }

    /// Build a local stdio binding that launches `command` with `args`.
    pub fn stdio(
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        read_only: bool,
    ) -> Self {
        Self::Stdio {
            name: name.into(),
            command: command.into(),
            args,
            read_only,
        }
    }

    /// Build the local `notesmith mcp start` stdio bridge for `vault`.
    ///
    /// Equivalent to `notesmith --vault <vault> mcp start [--read-only]`, run
    /// through the resolved `notesmith` binary (`notesmith_bin`). The bridge
    /// resolves the daemon endpoint itself and forwards MCP traffic to it.
    pub fn local_bridge(notesmith_bin: impl Into<String>, vault: &str, read_only: bool) -> Self {
        let mut args = vec![
            "--vault".to_string(),
            vault.to_string(),
            "mcp".to_string(),
            "start".to_string(),
        ];
        if read_only {
            args.push("--read-only".to_string());
        }
        Self::stdio(MCP_SERVER_NAME, notesmith_bin, args, read_only)
    }

    /// The server name surfaced to the agent.
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio { name, .. } | Self::Http { name, .. } => name,
        }
    }

    /// Whether this binding targets the read-only scope.
    pub fn read_only(&self) -> bool {
        match self {
            Self::Stdio { read_only, .. } | Self::Http { read_only, .. } => *read_only,
        }
    }

    /// Convert to a typed ACP `session/new` `mcpServers` entry.
    pub fn to_mcp_server(&self) -> McpServer {
        match self {
            Self::Stdio {
                name,
                command,
                args,
                ..
            } => McpServer::Stdio(
                McpServerStdio::new(name.clone(), PathBuf::from(command)).args(args.clone()),
            ),
            Self::Http { name, url, .. } => {
                McpServer::Http(McpServerHttp::new(name.clone(), url.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn http_binding_derives_read_only_from_the_endpoint_path() {
        assert!(!McpBinding::http("notesmith", "http://h/mcp/work").read_only());
        assert!(McpBinding::http("notesmith", "http://h/mcp-ro/work").read_only());
    }

    #[test]
    fn http_binding_serializes_as_an_http_transport() {
        let server = McpBinding::http("notesmith", "https://h/mcp/work").to_mcp_server();
        let value = serde_json::to_value(server).unwrap();
        assert_eq!(value["type"], json!("http"));
        assert_eq!(value["name"], json!("notesmith"));
        assert_eq!(value["url"], json!("https://h/mcp/work"));
    }

    #[test]
    fn local_bridge_builds_the_read_write_mcp_start_command() {
        let binding = McpBinding::local_bridge("notesmith", "work", false);
        assert!(!binding.read_only());
        assert_eq!(binding.name(), "notesmith");
        match &binding {
            McpBinding::Stdio { command, args, .. } => {
                assert_eq!(command, "notesmith");
                assert_eq!(args, &["--vault", "work", "mcp", "start"]);
            }
            other => panic!("expected a stdio binding, got {other:?}"),
        }
    }

    #[test]
    fn local_bridge_appends_read_only_flag_for_the_read_only_scope() {
        let binding = McpBinding::local_bridge("/usr/bin/notesmith", "journal", true);
        assert!(binding.read_only());
        match &binding {
            McpBinding::Stdio { command, args, .. } => {
                assert_eq!(command, "/usr/bin/notesmith");
                assert_eq!(args, &["--vault", "journal", "mcp", "start", "--read-only"]);
            }
            other => panic!("expected a stdio binding, got {other:?}"),
        }
    }

    #[test]
    fn stdio_binding_serializes_as_a_stdio_transport() {
        let binding = McpBinding::local_bridge("notesmith", "work", false);
        let value = serde_json::to_value(binding.to_mcp_server()).unwrap();
        assert_eq!(value["name"], json!("notesmith"));
        assert_eq!(value["command"], json!("notesmith"));
        assert_eq!(value["args"], json!(["--vault", "work", "mcp", "start"]));
        // The stdio variant is untagged: it carries no `type` discriminator.
        assert!(value.get("type").is_none());
    }
}
