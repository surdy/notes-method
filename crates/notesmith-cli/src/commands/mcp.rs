//! `notesmith mcp` subcommands.

use std::path::Path;

use anyhow::Context;
use clap::Subcommand;
use notesmith_config::{GlobalConfig, VaultConfig, detect_vault};
use notesmith_core::VaultEngine;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Start the MCP server over stdio
    Start,
}

impl McpCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
    ) -> anyhow::Result<()> {
        match self {
            McpCommand::Start => cmd_start(global_config, explicit_vault, cwd).await,
        }
    }
}

async fn cmd_start(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let engine = NativeVaultEngine;
    let notes = engine
        .scan(&detected.root)
        .with_context(|| format!("failed to scan vault {}", detected.name))?;
    let cache = VaultCache::open_in_memory()?;
    cache.reindex(&detected.name, &notes)?;
    let search_index = SearchIndex::open_in_memory()?;
    search_index.reindex(&detected.name, &notes)?;
    let vault_config =
        VaultConfig::load_from_vault(&detected.root).unwrap_or_else(|_| VaultConfig {
            name: detected.name.clone(),
            homepage: None,
            capture: Default::default(),
            daily: Default::default(),
            editor: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
        });

    let mcp = notesmith_mcp::NotesmithMcp::new(
        detected.name.clone(),
        detected.root,
        cache,
        search_index,
        vault_config,
    );
    notesmith_mcp::run_stdio(mcp).await
}
