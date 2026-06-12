//! MCP wiring for spawned agents (ADR 0011 Phase C).
//!
//! A spawned agent is auto-wired to Notesmith's per-vault MCP endpoint so it
//! can read (and, when read-write, edit) the vault the user is viewing. The
//! daemon serves Streamable HTTP MCP at `/mcp/<vault>` (read-write) and
//! `/mcp-ro/<vault>` (read-only); the scope is encoded in the URL, so this type
//! only needs the resolved endpoint URL plus the server name to expose to the
//! agent.
//!
//! Each adapter translates a [`McpBinding`] into its CLI's own MCP-config
//! surface (e.g. Claude Code's `--mcp-config`, Codex's `-c mcp_servers.*`).

use serde_json::{Map, Value, json};

/// A single MCP server to expose to an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBinding {
    /// Server name surfaced to the agent (e.g. `notesmith`).
    pub name: String,
    /// Streamable HTTP MCP endpoint URL (already scope-resolved).
    pub url: String,
}

impl McpBinding {
    /// Build a binding for `name` pointing at the HTTP MCP endpoint `url`.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
        }
    }

    /// JSON for Claude Code's `--mcp-config` flag (Streamable HTTP transport):
    /// `{"mcpServers":{"<name>":{"type":"http","url":"<url>"}}}`.
    pub fn claude_config_json(&self) -> String {
        let mut server = Map::new();
        server.insert("type".to_string(), json!("http"));
        server.insert("url".to_string(), json!(self.url));

        let mut servers = Map::new();
        servers.insert(self.name.clone(), Value::Object(server));

        let mut root = Map::new();
        root.insert("mcpServers".to_string(), Value::Object(servers));
        Value::Object(root).to_string()
    }

    /// `-c key=value` config overrides registering this server with Codex as a
    /// Streamable HTTP MCP server.
    pub fn codex_config_overrides(&self) -> Vec<String> {
        vec![format!("mcp_servers.{}.url=\"{}\"", self.name, self.url)]
    }

    /// An entry for an ACP `session/new` `mcpServers` array, describing this
    /// server as an HTTP MCP transport:
    /// `{"type":"http","name":"<name>","url":"<url>","headers":[]}`.
    ///
    /// The agent advertises `mcpCapabilities.http` during `initialize`; this is
    /// the single MCP-wiring path shared by every ACP agent (ADR 0011 Phase E).
    pub fn acp_server_json(&self) -> Value {
        json!({
            "type": "http",
            "name": self.name,
            "url": self.url,
            "headers": [],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn claude_config_json_uses_http_transport() {
        let binding = McpBinding::new("notesmith", "http://127.0.0.1:27183/mcp-ro/work");
        let value: Value = serde_json::from_str(&binding.claude_config_json()).unwrap();
        assert_eq!(
            value,
            json!({
                "mcpServers": {
                    "notesmith": {
                        "type": "http",
                        "url": "http://127.0.0.1:27183/mcp-ro/work"
                    }
                }
            })
        );
    }

    #[test]
    fn codex_overrides_target_the_named_server_url() {
        let binding = McpBinding::new("notesmith", "http://127.0.0.1:27183/mcp/work");
        assert_eq!(
            binding.codex_config_overrides(),
            vec!["mcp_servers.notesmith.url=\"http://127.0.0.1:27183/mcp/work\"".to_string()]
        );
    }

    #[test]
    fn acp_server_json_describes_an_http_transport() {
        let binding = McpBinding::new("notesmith", "http://127.0.0.1:27183/mcp-ro/work");
        assert_eq!(
            binding.acp_server_json(),
            json!({
                "type": "http",
                "name": "notesmith",
                "url": "http://127.0.0.1:27183/mcp-ro/work",
                "headers": [],
            })
        );
    }
}
