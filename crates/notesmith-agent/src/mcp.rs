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
//!   HTTP-capable agent uses this — and GitHub Copilot needs it, because its
//!   ACP mode *rejects* stdio MCP servers supplied by the ACP client
//!   (`Rejecting non-http/sse MCP server "<id>" from client`, verified on
//!   Copilot CLI 1.0.83-1). Copilot does support stdio MCP configured through
//!   its own config/SDK paths; it is the ACP-client-supplied case that fails.
//!   See the 2026-09-02 amendment to ADR 0012.
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
use notesmith_config::{McpConfig, expand_path_vars};

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

    /// Build the HTTP binding for a daemon-served vault: `<base>/mcp/<vault>`
    /// read-write, `<base>/mcp-ro/<vault>` read-only. Named via
    /// [`server_name_for_vault`] so it surfaces to the agent as the same server
    /// as the stdio [`local_bridge`](Self::local_bridge) for that vault,
    /// whichever transport the session ends up selecting.
    pub fn daemon_http(daemon_url: &str, vault: &str, read_only: bool) -> Self {
        let base = daemon_url.trim_end_matches('/');
        let scope = if read_only { "mcp-ro" } else { "mcp" };
        Self::http(
            server_name_for_vault(vault),
            format!("{base}/{scope}/{vault}"),
        )
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

    /// Whether this binding launches a local stdio subprocess.
    ///
    /// Agents whose ACP mode refuses client-supplied stdio servers (Copilot,
    /// github/copilot-cli#3889) receive these through a spawn-time config file
    /// instead of the `session/new` `mcpServers` array — see
    /// [`AcpSession::with_spawn_stdio_mcp_config`](crate::AcpSession::with_spawn_stdio_mcp_config).
    pub fn is_stdio(&self) -> bool {
        matches!(self, Self::Stdio { .. })
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

/// Convert the enabled external `[[mcp.servers]]` entries of the global
/// `[mcp]` config into [`McpBinding`]s for an agent session (ADR 0016 / #211,
/// hoisted from the Tauri bridge for the headless CLI in #283). A server with a
/// non-empty `command` becomes a stdio binding (env + tilde/`$VAR` expansion
/// applied); otherwise a non-empty `url` becomes an HTTP binding whose header
/// *values* also go through `$VAR` expansion, so config can hold
/// `"Bearer $WORKIQ_TOKEN"` and the secret stays in the environment. Disabled
/// servers and servers with neither a command nor a url are skipped (ADR 0009:
/// malformed config never panics).
///
/// A **stdio** entry reaches every agent, but not by the same route: Copilot's
/// ACP mode rejects client-supplied stdio MCP servers outright
/// (github/copilot-cli#3889), so for agents flagged with
/// [`AgentDescriptor::rejects_client_stdio_mcp`](crate::AgentDescriptor::rejects_client_stdio_mcp)
/// the stdio bindings are written to a per-session config file and injected at
/// spawn time via `--additional-mcp-config` instead of being advertised in
/// `session/new` (ADR 0012, 2026-09-02 amendment and 2026-09-03 addendum).
/// Every other agent receives them in the `session/new` `mcpServers` array as
/// before.
pub fn extra_mcp_bindings(cfg: &McpConfig) -> Vec<McpBinding> {
    let mut bindings = Vec::new();
    for server in &cfg.servers {
        if !server.enabled {
            continue;
        }
        let name = server.id.clone();
        match server.command.as_deref().filter(|c| !c.trim().is_empty()) {
            Some(command) => {
                let args = server.args.iter().map(|a| expand_path_vars(a)).collect();
                let env = server
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), expand_path_vars(value)))
                    .collect();
                bindings.push(McpBinding::stdio_with_env(
                    name,
                    expand_path_vars(command),
                    args,
                    env,
                    false,
                ));
            }
            None => {
                if let Some(url) = server.url.as_deref().filter(|u| !u.trim().is_empty()) {
                    let headers = server
                        .headers
                        .iter()
                        .map(|(header, value)| (header.clone(), expand_path_vars(value)))
                        .collect();
                    bindings.push(McpBinding::http_with_headers(
                        name,
                        url.to_string(),
                        headers,
                    ));
                }
            }
        }
    }
    bindings
}

/// Read the `[mcp]` section of the global config, degrading to the default
/// (empty) config when the file is absent or unreadable (ADR 0009).
pub fn load_mcp_config() -> McpConfig {
    notesmith_config::GlobalConfig::load()
        .map(|config| config.mcp)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_config::McpServerEntry;
    use serde_json::json;
    use std::collections::BTreeMap;

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
    fn daemon_http_builds_the_read_write_endpoint() {
        let binding = McpBinding::daemon_http("http://127.0.0.1:27183", "work", false);
        assert_eq!(binding.name(), "notesmith-work");
        assert!(!binding.read_only());
        match &binding {
            McpBinding::Http { url, .. } => assert_eq!(url, "http://127.0.0.1:27183/mcp/work"),
            other => panic!("expected an http binding, got {other:?}"),
        }
    }

    #[test]
    fn daemon_http_builds_the_read_only_endpoint_and_trims_a_trailing_slash() {
        let binding = McpBinding::daemon_http("https://notes.example.com/", "journal", true);
        assert_eq!(binding.name(), "notesmith-journal");
        assert!(binding.read_only());
        match &binding {
            McpBinding::Http { url, .. } => {
                assert_eq!(url, "https://notes.example.com/mcp-ro/journal");
            }
            other => panic!("expected an http binding, got {other:?}"),
        }
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
    fn extra_mcp_bindings_maps_command_to_stdio_and_url_to_http() {
        let cfg = McpConfig {
            servers: vec![
                McpServerEntry {
                    id: "fs".to_string(),
                    command: Some("npx".to_string()),
                    args: vec!["-y".to_string()],
                    env: BTreeMap::from([("K".to_string(), "v".to_string())]),
                    ..Default::default()
                },
                McpServerEntry {
                    id: "remote".to_string(),
                    url: Some("https://tools.example.com/mcp".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let bindings = extra_mcp_bindings(&cfg);
        assert_eq!(bindings.len(), 2);
        match &bindings[0] {
            McpBinding::Stdio {
                name,
                command,
                args,
                env,
                ..
            } => {
                assert_eq!(name, "fs");
                assert_eq!(command, "npx");
                assert_eq!(args, &["-y".to_string()]);
                assert_eq!(env, &[("K".to_string(), "v".to_string())]);
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        match &bindings[1] {
            McpBinding::Http {
                name, url, headers, ..
            } => {
                assert_eq!(name, "remote");
                assert_eq!(url, "https://tools.example.com/mcp");
                assert!(headers.is_empty());
            }
            other => panic!("expected http, got {other:?}"),
        }
    }

    #[test]
    fn extra_mcp_bindings_skips_disabled_and_transportless_servers() {
        let cfg = McpConfig {
            servers: vec![
                McpServerEntry {
                    id: "disabled".to_string(),
                    command: Some("npx".to_string()),
                    enabled: false,
                    ..Default::default()
                },
                McpServerEntry {
                    id: "no-transport".to_string(),
                    command: None,
                    url: None,
                    ..Default::default()
                },
                McpServerEntry {
                    id: "blank-url".to_string(),
                    command: Some("  ".to_string()),
                    url: Some("   ".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        assert!(extra_mcp_bindings(&cfg).is_empty());
    }

    #[test]
    fn extra_mcp_bindings_expands_header_values_from_the_environment() {
        // SAFETY: a uniquely-named variable avoids collisions with parallel tests.
        let var = "NOTESMITH_TEST_MCP_HEADER_TOKEN";
        unsafe {
            std::env::set_var(var, "tok-123");
        }
        let cfg = McpConfig {
            servers: vec![McpServerEntry {
                id: "workiq".to_string(),
                url: Some("https://workiq.example.com/mcp".to_string()),
                headers: BTreeMap::from([
                    ("Authorization".to_string(), format!("Bearer ${var}")),
                    ("X-Static".to_string(), "plain".to_string()),
                ]),
                ..Default::default()
            }],
            ..Default::default()
        };

        let bindings = extra_mcp_bindings(&cfg);
        unsafe {
            std::env::remove_var(var);
        }
        assert_eq!(bindings.len(), 1);
        match &bindings[0] {
            McpBinding::Http { headers, .. } => {
                // Header *values* are expanded against the environment at
                // binding-build time; names pass through verbatim.
                assert_eq!(
                    headers,
                    &[
                        ("Authorization".to_string(), "Bearer tok-123".to_string()),
                        ("X-Static".to_string(), "plain".to_string()),
                    ]
                );
            }
            other => panic!("expected http, got {other:?}"),
        }
    }

    #[test]
    fn extra_mcp_bindings_ignores_headers_on_a_stdio_server() {
        let cfg = McpConfig {
            servers: vec![McpServerEntry {
                id: "fs".to_string(),
                command: Some("npx".to_string()),
                headers: BTreeMap::from([("Authorization".to_string(), "Bearer x".to_string())]),
                ..Default::default()
            }],
            ..Default::default()
        };

        let bindings = extra_mcp_bindings(&cfg);
        assert!(matches!(&bindings[0], McpBinding::Stdio { .. }));
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
