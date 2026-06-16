use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub default_vault: Option<String>,
    #[serde(default)]
    pub vaults: BTreeMap<String, VaultRegistration>,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

/// The `[agents]` section (ADR 0013, decision 4): the manual escape hatch for
/// agent discovery. `debug` is an opt-in diagnostics flag that lives directly
/// under `[agents]`; every other key is an agent id whose value is an
/// [`AgentEntry`] subtable (e.g. `[agents.copilot]`). The agent ids are
/// flattened so they sit as siblings of `debug`, matching the schema in the ADR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentsConfig {
    /// Opt-in structured diagnostics flag (default `false`).
    #[serde(default)]
    pub debug: bool,
    /// Per-agent overrides / custom agents, keyed by agent id. Flattened so each
    /// id is a subtable directly under `[agents]` (`[agents.<id>]`).
    #[serde(default, flatten)]
    pub entries: BTreeMap<String, AgentEntry>,
}

/// A single `[agents.<id>]` entry: either an override of a built-in agent (when
/// `<id>` matches a registry id) or a brand-new custom ACP agent. A user entry
/// always wins over auto-detection for the same id; a custom id is launched
/// verbatim; `enabled = false` hides a built-in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentEntry {
    /// Program to launch (path or PATH-resolved name). Tilde / `$VAR` allowed.
    #[serde(default)]
    pub command: Option<String>,
    /// Base launch arguments. Tilde / `$VAR` allowed per element.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables applied to the spawned agent process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Display name shown in the picker for a custom agent (falls back to id).
    #[serde(default)]
    pub display_name: Option<String>,
    /// Whether the agent is enabled. `false` hides a built-in. Defaults to true.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AgentEntry {
    fn default() -> Self {
        Self {
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            display_name: None,
            enabled: true,
        }
    }
}

/// The `[mcp]` section (ADR 0016, decision 3): external MCP servers the agent
/// can reach in addition to the built-in per-vault daemon tools. Lives in the
/// **global** config so a server list is reusable across vaults. Each
/// `[[mcp.servers]]` entry is an [`McpServerEntry`]; the built-in vault tools
/// are *not* stored here (the daemon always exposes them and they are
/// non-removable in the UI).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpConfig {
    /// External MCP servers, in user-defined order. Each carries its own `id`.
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,
}

/// A single `[[mcp.servers]]` entry. The transport is **stdio** when `command`
/// is set and **HTTP(S)** when `url` is set; if both are present `command`
/// wins. `enabled = false` keeps the entry configured but hides it from the
/// agent session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerEntry {
    /// Stable identifier and the server name surfaced to the agent.
    pub id: String,
    /// Executable to launch for a stdio server (path or PATH-resolved name).
    /// Tilde / `$VAR` allowed. Mutually exclusive with `url` (`command` wins).
    #[serde(default)]
    pub command: Option<String>,
    /// Launch arguments for a stdio server. Tilde / `$VAR` allowed per element.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables applied to a spawned stdio server.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Streamable HTTP(S) MCP endpoint for an HTTP server. Mutually exclusive
    /// with `command`.
    #[serde(default)]
    pub url: Option<String>,
    /// Display name shown in the Settings list (falls back to `id`).
    #[serde(default)]
    pub display_name: Option<String>,
    /// Whether the server is handed to agent sessions. Defaults to true.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for McpServerEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            url: None,
            display_name: None,
            enabled: true,
        }
    }
}

/// Expand a leading `~` (to `$HOME`) and `$VAR` / `${VAR}` references in `s`
/// against the process environment. Unknown variables are left verbatim; an
/// empty or garbage input never panics. Kept dependency-light (manual scan).
pub fn expand_path_vars(s: &str) -> String {
    // Expand a leading `~` (only when it stands alone or precedes a `/`).
    let mut out = String::with_capacity(s.len());
    let rest = if let Some(after) = s.strip_prefix('~') {
        if after.is_empty() || after.starts_with('/') {
            match std::env::var("HOME") {
                Ok(home) => out.push_str(&home),
                Err(_) => out.push('~'),
            }
            after
        } else {
            s
        }
    } else {
        s
    };

    // Expand `$VAR` and `${VAR}` against the environment.
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('{') => {
                chars.next();
                let mut name = String::new();
                let mut closed = false;
                for nc in chars.by_ref() {
                    if nc == '}' {
                        closed = true;
                        break;
                    }
                    name.push(nc);
                }
                if closed {
                    match std::env::var(&name) {
                        Ok(value) => out.push_str(&value),
                        Err(_) => {
                            out.push_str("${");
                            out.push_str(&name);
                            out.push('}');
                        }
                    }
                } else {
                    // Unterminated `${…` — emit verbatim.
                    out.push_str("${");
                    out.push_str(&name);
                }
            }
            Some(p) if p.is_ascii_alphabetic() || *p == '_' => {
                let mut name = String::new();
                while let Some(p) = chars.peek() {
                    if p.is_ascii_alphanumeric() || *p == '_' {
                        name.push(*p);
                        chars.next();
                    } else {
                        break;
                    }
                }
                match std::env::var(&name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        out.push('$');
                        out.push_str(&name);
                    }
                }
            }
            // Lone `$` (end of string or non-name char) — emit verbatim.
            _ => out.push('$'),
        }
    }

    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

fn default_bind() -> String {
    "127.0.0.1:27183".to_string()
}

fn default_auto_start() -> bool {
    true
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            auto_start: default_auto_start(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultRegistration {
    pub path: PathBuf,
}

impl GlobalConfig {
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadError {
            path: path.to_path_buf(),
            source,
        })?;

        toml::from_str(&content).map_err(|error| ConfigError::ParseError {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    }

    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_path().ok_or(ConfigError::NoConfigDir)?;
        Self::load_from(&path)
    }

    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(dirs::config_dir)
            .map(|dir| dir.join("notesmith").join("config.toml"))
    }

    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            })?;
        }

        let content =
            toml::to_string_pretty(self).map_err(|error| ConfigError::SerializeError {
                message: error.to_string(),
            })?;

        std::fs::write(path, content).map_err(|source| ConfigError::WriteError {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn vault(&self, name: &str) -> Option<&VaultRegistration> {
        self.vaults.get(name)
    }

    pub fn effective_default(&self) -> Option<&str> {
        self.default_vault.as_deref().or_else(|| {
            if self.vaults.len() == 1 {
                self.vaults.keys().next().map(String::as_str)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_global_config() -> GlobalConfig {
        let mut config = GlobalConfig {
            daemon: DaemonConfig {
                bind: "0.0.0.0:8080".to_string(),
                auto_start: false,
            },
            default_vault: Some("work".to_string()),
            vaults: BTreeMap::new(),
            agents: AgentsConfig::default(),
            mcp: McpConfig::default(),
        };
        config.vaults.insert(
            "work".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/work"),
            },
        );
        config.vaults.insert(
            "personal".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/personal"),
            },
        );
        config
    }

    #[test]
    fn load_from_returns_default_when_path_does_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("missing.toml");

        let config = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(config, GlobalConfig::default());
    }

    #[test]
    fn load_from_reads_valid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
default_vault = "work"

[daemon]
bind = "0.0.0.0:8080"
auto_start = false

[vaults.work]
path = "/vaults/work"

[vaults.personal]
path = "/vaults/personal"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(config.default_vault.as_deref(), Some("work"));
        assert_eq!(config.daemon.bind, "0.0.0.0:8080");
        assert!(!config.daemon.auto_start);
        assert_eq!(config.vaults["work"].path, PathBuf::from("/vaults/work"));
        assert_eq!(
            config.vaults["personal"].path,
            PathBuf::from("/vaults/personal")
        );
    }

    #[test]
    fn load_from_returns_parse_error_for_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(&path, "invalid {{ toml").unwrap();

        let error = GlobalConfig::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::ParseError { .. }));
    }

    #[test]
    fn save_to_round_trips_through_disk() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nested").join("config.toml");
        let expected = sample_global_config();

        expected.save_to(&path).unwrap();
        let actual = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn vault_looks_up_registered_entries_and_missing_names() {
        let config = sample_global_config();

        assert_eq!(
            config
                .vault("work")
                .map(|registration| registration.path.clone()),
            Some(PathBuf::from("/vaults/work"))
        );
        assert!(config.vault("missing").is_none());
    }

    #[test]
    fn effective_default_prefers_explicit_default_then_single_vault_fallback() {
        let config = sample_global_config();
        assert_eq!(config.effective_default(), Some("work"));

        let mut single = GlobalConfig::default();
        single.vaults.insert(
            "solo".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/solo"),
            },
        );
        assert_eq!(single.effective_default(), Some("solo"));

        let mut multiple = GlobalConfig::default();
        multiple.vaults.insert(
            "a".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/a"),
            },
        );
        multiple.vaults.insert(
            "b".to_string(),
            VaultRegistration {
                path: PathBuf::from("/vaults/b"),
            },
        );
        assert_eq!(multiple.effective_default(), None);
    }

    #[test]
    fn agents_config_round_trips_through_disk() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[agents]
debug = true

[agents.copilot]
command = "/opt/copilot/bin/copilot"
args = ["--acp"]

[agents.my-agent]
display_name = "My Agent"
command = "node"
args = ["~/projects/agent/index.js", "--acp"]
enabled = true

[agents.my-agent.env]
FOO = "bar"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();

        assert!(config.agents.debug);

        let copilot = &config.agents.entries["copilot"];
        assert_eq!(copilot.command.as_deref(), Some("/opt/copilot/bin/copilot"));
        assert_eq!(copilot.args, vec!["--acp".to_string()]);
        assert!(copilot.enabled);

        let custom = &config.agents.entries["my-agent"];
        assert_eq!(custom.display_name.as_deref(), Some("My Agent"));
        assert_eq!(custom.command.as_deref(), Some("node"));
        assert_eq!(
            custom.args,
            vec!["~/projects/agent/index.js".to_string(), "--acp".to_string()]
        );
        assert!(custom.enabled);
        assert_eq!(custom.env["FOO"], "bar");

        // Re-serialize to disk and re-load: all fields survive intact.
        let round_trip_path = temp_dir.path().join("round-trip.toml");
        config.save_to(&round_trip_path).unwrap();
        let reloaded = GlobalConfig::load_from(&round_trip_path).unwrap();
        assert_eq!(reloaded, config);
    }

    #[test]
    fn agent_entry_enabled_defaults_to_true_when_omitted() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[agents.codex]
command = "/usr/local/bin/codex-acp"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert!(config.agents.entries["codex"].enabled);
    }

    #[test]
    fn missing_agents_section_yields_default_agents_config() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
default_vault = "work"

[vaults.work]
path = "/vaults/work"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(config.agents, AgentsConfig::default());
        assert!(!config.agents.debug);
        assert!(config.agents.entries.is_empty());
    }

    #[test]
    fn mcp_config_round_trips_through_disk() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[[mcp.servers]]
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "~/notes"]

[mcp.servers.env]
TOKEN = "secret"

[[mcp.servers]]
id = "remote-tools"
url = "https://tools.example.com/mcp"
display_name = "Remote Tools"
enabled = false
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(config.mcp.servers.len(), 2);

        let fs_server = &config.mcp.servers[0];
        assert_eq!(fs_server.id, "filesystem");
        assert_eq!(fs_server.command.as_deref(), Some("npx"));
        assert_eq!(
            fs_server.args,
            vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "~/notes".to_string()
            ]
        );
        assert_eq!(fs_server.env["TOKEN"], "secret");
        assert!(fs_server.url.is_none());
        assert!(fs_server.enabled);

        let remote = &config.mcp.servers[1];
        assert_eq!(remote.id, "remote-tools");
        assert_eq!(remote.url.as_deref(), Some("https://tools.example.com/mcp"));
        assert_eq!(remote.display_name.as_deref(), Some("Remote Tools"));
        assert!(remote.command.is_none());
        assert!(!remote.enabled);

        // Re-serialize and re-load: every field survives intact.
        let round_trip_path = temp_dir.path().join("round-trip.toml");
        config.save_to(&round_trip_path).unwrap();
        let reloaded = GlobalConfig::load_from(&round_trip_path).unwrap();
        assert_eq!(reloaded, config);
    }

    #[test]
    fn mcp_server_enabled_defaults_to_true_when_omitted() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[[mcp.servers]]
id = "tools"
command = "tools-server"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert!(config.mcp.servers[0].enabled);
    }

    #[test]
    fn missing_mcp_section_yields_default_mcp_config() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
default_vault = "work"

[vaults.work]
path = "/vaults/work"
"#,
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(config.mcp, McpConfig::default());
        assert!(config.mcp.servers.is_empty());
    }

    #[test]
    fn malformed_agent_entry_returns_parse_error_without_panicking() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        // `args` should be an array of strings; a bare integer is a type error.
        fs::write(
            &path,
            r#"
[agents.copilot]
args = 42
"#,
        )
        .unwrap();

        let error = GlobalConfig::load_from(&path).unwrap_err();
        assert!(matches!(error, ConfigError::ParseError { .. }));
    }

    #[test]
    fn expand_path_vars_expands_tilde_to_home() {
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(
                expand_path_vars("~/projects/agent"),
                format!("{home}/projects/agent")
            );
            assert_eq!(expand_path_vars("~"), home);
        }
        // A tilde that is not a path prefix (e.g. `~user`) is left untouched.
        assert_eq!(expand_path_vars("~user/x"), "~user/x");
    }

    #[test]
    fn expand_path_vars_expands_named_variables() {
        // SAFETY: a uniquely-named variable avoids collisions with parallel tests.
        let var = "NOTESMITH_TEST_EXPAND_VAR";
        unsafe {
            std::env::set_var(var, "expanded");
        }
        assert_eq!(
            expand_path_vars(&format!("/bin/${var}/x")),
            "/bin/expanded/x"
        );
        assert_eq!(
            expand_path_vars(&format!("/bin/${{{var}}}/x")),
            "/bin/expanded/x"
        );
        unsafe {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn expand_path_vars_leaves_unknown_variables_verbatim() {
        let missing = "NOTESMITH_TEST_DEFINITELY_UNSET_VAR";
        unsafe {
            std::env::remove_var(missing);
        }
        assert_eq!(
            expand_path_vars(&format!("${missing}/x")),
            format!("${missing}/x")
        );
        assert_eq!(
            expand_path_vars(&format!("${{{missing}}}/x")),
            format!("${{{missing}}}/x")
        );
    }

    #[test]
    fn expand_path_vars_does_not_panic_on_empty_or_garbage() {
        assert_eq!(expand_path_vars(""), "");
        assert_eq!(expand_path_vars("$"), "$");
        assert_eq!(expand_path_vars("${"), "${");
        assert_eq!(expand_path_vars("${unterminated"), "${unterminated");
        assert_eq!(expand_path_vars("$$$"), "$$$");
        assert_eq!(expand_path_vars("plain/path"), "plain/path");
        // Non-ASCII content is preserved without panicking.
        assert_eq!(expand_path_vars("café/ünïcode"), "café/ünïcode");
    }
}
