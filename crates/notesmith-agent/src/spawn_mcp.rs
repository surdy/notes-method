//! Spawn-time MCP config injection for agents that reject client-supplied
//! stdio MCP servers (ADR 0012, 2026-09-03 addendum).
//!
//! GitHub Copilot's ACP mode refuses every stdio MCP server handed to it by the
//! ACP client in `session/new` — it logs `Rejecting non-http/sse MCP server
//! "<id>" from client` and the server never reaches the session
//! (github/copilot-cli#3889). Its *own* config path accepts stdio servers just
//! fine, so Notesmith writes the user's external stdio `[[mcp.servers]]`
//! entries into a per-session JSON file and points Copilot at it with
//! `--additional-mcp-config=@<absolute path>` when the process is spawned. The
//! ACP-supplied HTTP bindings (the vault server, HTTP externals) are unaffected
//! and coexist with the injected servers in the same session — field-verified
//! against Copilot CLI 1.0.83-3 on 2026-09-03.
//!
//! **Security.** A stdio server's `env` may carry credentials, so the generated
//! file is created with `0600` permissions, lives only as long as the session
//! (it is removed when the [`SpawnMcpConfig`] is dropped), and neither its
//! contents nor any resolved env value is ever logged.

use std::io::{self, Write};
use std::path::Path;

use serde_json::{Map, Value, json};
use tempfile::{Builder, NamedTempFile};

use crate::mcp::McpBinding;

/// The Copilot CLI flag that loads an extra MCP config document at startup.
/// The value is `@` followed by an **absolute** path to the JSON file.
pub(crate) const ADDITIONAL_MCP_CONFIG_FLAG: &str = "--additional-mcp-config";

/// Per-server MCP initialization budget written into every generated entry, in
/// milliseconds. Deliberately just under Copilot's hard 60s MCP init budget
/// (github/copilot-cli#4421) so a slow server fails with our timeout — a single
/// dead server — rather than tripping Copilot's global one, which aborts the
/// whole session start.
pub(crate) const SPAWN_MCP_INIT_TIMEOUT_MS: u64 = 55_000;

/// Render the spawn-time MCP config document for `bindings`.
///
/// Only [`McpBinding::Stdio`] entries are emitted (HTTP servers reach the agent
/// through the ACP `session/new` `mcpServers` array as usual). The shape is the
/// one validated against Copilot 1.0.83-3:
///
/// ```json
/// {
///   "mcpServers": {
///     "<name>": {
///       "type": "local",
///       "command": "/absolute/path/to/binary",
///       "args": ["..."],
///       "tools": ["*"],
///       "deferTools": "never",
///       "disableToolCache": true,
///       "timeout": 55000
///     }
///   }
/// }
/// ```
///
/// A binding that carries environment variables additionally gets an `"env"`
/// object (`{"NAME": "value"}`), which is how Copilot's `local` server schema
/// expresses a server's environment. The object is omitted entirely when the
/// binding has no env, keeping the common document byte-identical to the
/// validated shape.
pub(crate) fn render_spawn_mcp_config(bindings: &[McpBinding]) -> Value {
    let mut servers = Map::new();
    for binding in bindings {
        let McpBinding::Stdio {
            name,
            command,
            args,
            env,
            ..
        } = binding
        else {
            continue;
        };
        let mut entry = Map::new();
        entry.insert("type".to_string(), json!("local"));
        entry.insert("command".to_string(), json!(command));
        entry.insert("args".to_string(), json!(args));
        entry.insert("tools".to_string(), json!(["*"]));
        entry.insert("deferTools".to_string(), json!("never"));
        entry.insert("disableToolCache".to_string(), json!(true));
        entry.insert("timeout".to_string(), json!(SPAWN_MCP_INIT_TIMEOUT_MS));
        if !env.is_empty() {
            let vars: Map<String, Value> = env
                .iter()
                .map(|(key, value)| (key.clone(), json!(value)))
                .collect();
            entry.insert("env".to_string(), Value::Object(vars));
        }
        servers.insert(name.clone(), Value::Object(entry));
    }
    json!({ "mcpServers": Value::Object(servers) })
}

/// A materialized spawn-time MCP config file, owned by one session.
///
/// The file is created `0600` in the system temp directory under a unique
/// per-session name and **deleted when this value is dropped**, which happens
/// when the owning session is dropped. Its contents may include credentials
/// (see the module docs), so they are never logged.
pub(crate) struct SpawnMcpConfig {
    file: NamedTempFile,
}

impl SpawnMcpConfig {
    /// Write the stdio `bindings` to a fresh per-session config file.
    ///
    /// Returns an error only for genuine I/O failures; callers degrade to the
    /// previous behavior (advertise the servers over ACP and let the agent
    /// refuse them) rather than failing the session.
    pub(crate) fn write(bindings: &[McpBinding]) -> io::Result<Self> {
        let document = render_spawn_mcp_config(bindings);
        // `tempfile` creates with `0600` on unix; we set it explicitly anyway so
        // the guarantee is local to this function and does not ride on a
        // dependency's default (the file may hold credentials).
        let mut file = Builder::new()
            .prefix("notesmith-mcp-")
            .suffix(".json")
            .tempfile()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        file.write_all(&bytes)?;
        file.as_file_mut().flush()?;
        Ok(Self { file })
    }

    /// Absolute path of the generated config file.
    pub(crate) fn path(&self) -> &Path {
        self.file.path()
    }

    /// The spawn argument that loads this config:
    /// `--additional-mcp-config=@<absolute path>`.
    pub(crate) fn flag_arg(&self) -> String {
        format!("{ADDITIONAL_MCP_CONFIG_FLAG}=@{}", self.path().display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(name: &str) -> McpBinding {
        McpBinding::stdio(
            name,
            "/opt/workiq/bin/workiq",
            vec!["mcp".to_string()],
            false,
        )
    }

    #[test]
    fn renders_the_field_validated_copilot_shape() {
        let value = render_spawn_mcp_config(&[stdio("notesmith-workiq")]);
        assert_eq!(
            value,
            json!({
                "mcpServers": {
                    "notesmith-workiq": {
                        "type": "local",
                        "command": "/opt/workiq/bin/workiq",
                        "args": ["mcp"],
                        "tools": ["*"],
                        "deferTools": "never",
                        "disableToolCache": true,
                        "timeout": 55000
                    }
                }
            })
        );
    }

    #[test]
    fn init_timeout_stays_below_the_copilot_sixty_second_budget() {
        const { assert!(SPAWN_MCP_INIT_TIMEOUT_MS < 60_000) };
        assert_eq!(SPAWN_MCP_INIT_TIMEOUT_MS, 55_000);
        // The rendered document carries the same budget.
        let value = render_spawn_mcp_config(&[stdio("one")]);
        assert_eq!(value["mcpServers"]["one"]["timeout"], json!(55_000));
    }

    #[test]
    fn renders_an_env_object_only_when_the_binding_carries_env() {
        let with_env = McpBinding::stdio_with_env(
            "secretive",
            "/usr/local/bin/server",
            vec!["--stdio".to_string()],
            vec![
                ("TOKEN".to_string(), "s3cr3t".to_string()),
                ("REGION".to_string(), "eu".to_string()),
            ],
            false,
        );
        let value = render_spawn_mcp_config(&[with_env]);
        let entry = &value["mcpServers"]["secretive"];
        assert_eq!(entry["type"], json!("local"));
        assert_eq!(entry["env"], json!({ "TOKEN": "s3cr3t", "REGION": "eu" }));

        // No env on the binding → no `env` key at all.
        let plain = render_spawn_mcp_config(&[stdio("plain")]);
        assert!(plain["mcpServers"]["plain"].get("env").is_none());
    }

    #[test]
    fn renders_every_stdio_binding_and_skips_http_ones() {
        let bindings = vec![
            stdio("one"),
            McpBinding::http("remote", "https://tools.example.com/mcp"),
            stdio("two"),
        ];
        let value = render_spawn_mcp_config(&bindings);
        let servers = value["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("one"));
        assert!(servers.contains_key("two"));
        assert!(!servers.contains_key("remote"));
    }

    #[test]
    fn written_file_holds_the_rendered_document_at_an_absolute_path() {
        let config = SpawnMcpConfig::write(&[stdio("notesmith-workiq")]).expect("write config");
        assert!(config.path().is_absolute());
        let text = std::fs::read_to_string(config.path()).expect("read back");
        let parsed: Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(
            parsed,
            render_spawn_mcp_config(&[stdio("notesmith-workiq")])
        );
    }

    #[test]
    fn flag_arg_uses_the_at_path_form() {
        let config = SpawnMcpConfig::write(&[stdio("one")]).expect("write config");
        let arg = config.flag_arg();
        assert_eq!(
            arg,
            format!("--additional-mcp-config=@{}", config.path().display())
        );
    }

    #[cfg(unix)]
    #[test]
    fn written_file_is_owner_read_write_only() {
        use std::os::unix::fs::PermissionsExt;

        let config = SpawnMcpConfig::write(&[stdio("one")]).expect("write config");
        let mode = std::fs::metadata(config.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "config file must be 0600");
    }

    #[test]
    fn dropping_the_config_removes_the_file() {
        let config = SpawnMcpConfig::write(&[stdio("one")]).expect("write config");
        let path = config.path().to_path_buf();
        assert!(path.exists());
        drop(config);
        assert!(!path.exists(), "config file should be removed on drop");
    }
}
