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

use agent_client_protocol::schema::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerStdio,
};

/// Server name surfaced to the agent for the vault's MCP server.
pub const MCP_SERVER_NAME: &str = "notesmith";

fn sanitize_server_component(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Build the per-vault MCP server name surfaced to the agent, e.g.
/// `notesmith-work` (issue #259).
///
/// Naming the built-in server after the vault lets an agent that also has other
/// MCP servers available (e.g. a globally-registered server for a different
/// vault) tell which server maps to the vault it is working in, and prefer it.
/// The vault name is sanitized to the identifier-safe set `[A-Za-z0-9_-]` (other
/// characters become `-`); an empty result falls back to the bare
/// [`MCP_SERVER_NAME`].
pub fn server_name_for_vault(vault: &str) -> String {
    let slug = sanitize_server_component(vault);
    if slug.is_empty() {
        MCP_SERVER_NAME.to_string()
    } else {
        format!("{MCP_SERVER_NAME}-{slug}")
    }
}

/// Build a stable MCP server name for a vault that needs a namespace suffix
/// (for example the same vault name hosted by another saved daemon
/// connection). The base vault naming convention still comes from
/// [`server_name_for_vault`].
pub fn server_name_for_namespaced_vault(namespace: &str, vault: &str) -> String {
    let base = server_name_for_vault(vault);
    let namespace = sanitize_server_component(namespace);
    if namespace.is_empty() {
        base
    } else {
        format!("{base}-{namespace}")
    }
}

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
        /// Environment variables applied to the spawned server process. Empty
        /// for the built-in vault bridge; populated for external stdio servers.
        env: Vec<(String, String)>,
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
        /// HTTP headers the agent sends with every request to the endpoint
        /// (e.g. an `Authorization` bearer credential for an auth-protected
        /// remote MCP server). Empty for the built-in daemon endpoints;
        /// populated for external servers. Values must already be fully
        /// resolved (any `$VAR` expansion happens before the binding is built).
        headers: Vec<(String, String)>,
        /// Whether the endpoint targets the read-only scope.
        read_only: bool,
    },
}

impl McpBinding {
    /// Build an HTTP(S) binding for `name` at `url` with no request headers.
    /// The read-only scope is derived from the endpoint path: `/mcp-ro/`
    /// denotes read-only. Use
    /// [`http_with_headers`](Self::http_with_headers) for an external server
    /// that needs auth (or other) headers on every request.
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self::http_with_headers(name, url, Vec::new())
    }

    /// Build an HTTP(S) binding with explicit request headers, for an external
    /// MCP server that requires credentials (e.g. an `Authorization` bearer
    /// token for an Entra-protected remote server). Header values must already
    /// be fully resolved — `$VAR` expansion happens at binding-build time,
    /// before this constructor is called.
    pub fn http_with_headers(
        name: impl Into<String>,
        url: impl Into<String>,
        headers: Vec<(String, String)>,
    ) -> Self {
        let url = url.into();
        let read_only = url.contains("/mcp-ro/");
        Self::Http {
            name: name.into(),
            url,
            headers,
            read_only,
        }
    }

    /// Build a local stdio binding that launches `command` with `args` and no
    /// extra environment. Use [`stdio_with_env`](Self::stdio_with_env) to pass
    /// environment variables to an external server.
    pub fn stdio(
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        read_only: bool,
    ) -> Self {
        Self::stdio_with_env(name, command, args, Vec::new(), read_only)
    }

    /// Build a local stdio binding with explicit environment variables, for an
    /// external MCP server that needs credentials or configuration in its env.
    pub fn stdio_with_env(
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        env: Vec<(String, String)>,
        read_only: bool,
    ) -> Self {
        Self::Stdio {
            name: name.into(),
            command: command.into(),
            args,
            env,
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
        Self::stdio(server_name_for_vault(vault), notesmith_bin, args, read_only)
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
                env,
                ..
            } => {
                let env_vars: Vec<EnvVariable> = env
                    .iter()
                    .map(|(key, value)| EnvVariable::new(key.clone(), value.clone()))
                    .collect();
                McpServer::Stdio(
                    McpServerStdio::new(name.clone(), PathBuf::from(command))
                        .args(args.clone())
                        .env(env_vars),
                )
            }
            Self::Http {
                name, url, headers, ..
            } => {
                let http_headers: Vec<HttpHeader> = headers
                    .iter()
                    .map(|(header_name, value)| HttpHeader::new(header_name.clone(), value.clone()))
                    .collect();
                McpServer::Http(McpServerHttp::new(name.clone(), url.clone()).headers(http_headers))
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
        assert_eq!(binding.name(), "notesmith-work");
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
        assert_eq!(binding.name(), "notesmith-journal");
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
        assert_eq!(value["name"], json!("notesmith-work"));
        assert_eq!(value["command"], json!("notesmith"));
        assert_eq!(value["args"], json!(["--vault", "work", "mcp", "start"]));
        // The stdio variant is untagged: it carries no `type` discriminator.
        assert!(value.get("type").is_none());
    }

    #[test]
    fn server_name_for_vault_slugs_and_falls_back() {
        assert_eq!(server_name_for_vault("work"), "notesmith-work");
        assert_eq!(server_name_for_vault("embed-test"), "notesmith-embed-test");
        // Non-identifier characters (spaces, slashes) become `-`, trimmed.
        assert_eq!(server_name_for_vault("My Vault"), "notesmith-My-Vault");
        assert_eq!(server_name_for_vault(" a/b "), "notesmith-a-b");
        // Empty / all-invalid names fall back to the bare server name.
        assert_eq!(server_name_for_vault(""), "notesmith");
        assert_eq!(server_name_for_vault("///"), "notesmith");
    }

    #[test]
    fn namespaced_server_name_stays_stable_and_unique() {
        assert_eq!(
            server_name_for_namespaced_vault("memory-host", "work"),
            "notesmith-work-memory-host"
        );
        assert_eq!(
            server_name_for_namespaced_vault("memory host", "work"),
            "notesmith-work-memory-host"
        );
        assert_eq!(
            server_name_for_namespaced_vault("other", "work"),
            "notesmith-work-other"
        );
        assert_eq!(
            server_name_for_namespaced_vault("other", " / "),
            "notesmith-other"
        );
    }

    #[test]
    fn http_binding_serializes_with_no_headers_by_default() {
        let server = McpBinding::http("notesmith", "https://h/mcp/work").to_mcp_server();
        let value = serde_json::to_value(server).unwrap();
        assert_eq!(value["headers"], json!([]));
    }

    #[test]
    fn http_with_headers_serializes_http_headers() {
        let binding = McpBinding::http_with_headers(
            "workiq",
            "https://workiq.example.com/mcp",
            vec![
                ("Authorization".to_string(), "Bearer tok".to_string()),
                ("X-Client".to_string(), "notesmith".to_string()),
            ],
        );
        assert!(!binding.read_only());
        let value = serde_json::to_value(binding.to_mcp_server()).unwrap();
        assert_eq!(value["type"], json!("http"));
        assert_eq!(value["url"], json!("https://workiq.example.com/mcp"));
        let headers = value["headers"].as_array().unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0]["name"], json!("Authorization"));
        assert_eq!(headers[0]["value"], json!("Bearer tok"));
        assert_eq!(headers[1]["name"], json!("X-Client"));
        assert_eq!(headers[1]["value"], json!("notesmith"));
    }

    #[test]
    fn stdio_with_env_serializes_environment_variables() {
        let binding = McpBinding::stdio_with_env(
            "filesystem",
            "npx",
            vec!["-y".to_string()],
            vec![("TOKEN".to_string(), "secret".to_string())],
            false,
        );
        let value = serde_json::to_value(binding.to_mcp_server()).unwrap();
        assert_eq!(value["command"], json!("npx"));
        let env = value["env"].as_array().unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(env[0]["name"], json!("TOKEN"));
        assert_eq!(env[0]["value"], json!("secret"));
    }
}
