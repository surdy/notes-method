//! `notesmith mcp` subcommands.

use std::path::Path;

use anyhow::{Context, Result};
use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Bridge a stdio MCP client to the daemon's MCP endpoint over HTTP
    Start {
        /// Bridge to the read-only endpoint, where write tools are rejected.
        #[arg(long)]
        read_only: bool,
    },
}

impl McpCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
    ) -> anyhow::Result<()> {
        match self {
            McpCommand::Start { read_only } => {
                cmd_start(global_config, explicit_vault, cwd, *read_only).await
            }
        }
    }
}

async fn cmd_start(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    read_only: bool,
) -> anyhow::Result<()> {
    let vault_name = resolve_vault_name(global_config, explicit_vault, cwd)?;

    // Resolves to the remote daemon when `--url` / `NOTESMITH_URL` is set,
    // otherwise the local daemon (auto-started on demand).
    let base = crate::daemon_client::ensure_daemon(global_config).await?;

    let endpoint = mcp_endpoint(&base, &vault_name, read_only)?;
    notesmith_mcp::run_stdio_bridge(endpoint.to_string()).await
}

fn resolve_vault_name(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
) -> Result<String> {
    // An explicit vault name is taken as-is so the bridge works against a
    // remote daemon without a matching local vault on disk.
    if let Some(vault) = explicit_vault {
        return Ok(vault.to_string());
    }
    Ok(detect_vault(cwd, explicit_vault, global_config)?.name)
}

fn mcp_endpoint(base: &Url, vault: &str, read_only: bool) -> Result<Url> {
    let prefix = if read_only { "mcp-ro" } else { "mcp" };
    // `base` typically ends in `/`; trim before joining for a clean path.
    let joined = format!("{}/{prefix}/{vault}", base.as_str().trim_end_matches('/'));
    Url::parse(&joined).with_context(|| format!("could not build MCP endpoint URL: {joined}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_read_write_endpoint() {
        let base = Url::parse("http://127.0.0.1:27183/").unwrap();
        let endpoint = mcp_endpoint(&base, "work", false).unwrap();
        assert_eq!(endpoint.as_str(), "http://127.0.0.1:27183/mcp/work");
    }

    #[test]
    fn builds_read_only_endpoint() {
        let base = Url::parse("http://host:8080").unwrap();
        let endpoint = mcp_endpoint(&base, "notes", true).unwrap();
        assert_eq!(endpoint.as_str(), "http://host:8080/mcp-ro/notes");
    }
}
