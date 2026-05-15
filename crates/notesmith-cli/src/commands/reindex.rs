use anyhow::Context;
use clap::Args;
use notesmith_config::GlobalConfig;
use reqwest::Url;
use serde_json::Value;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct ReindexCommand {
    /// Only rebuild the SQLite cache (skip search index)
    #[arg(long, conflicts_with = "search_only")]
    cache_only: bool,

    /// Only rebuild the search index (skip SQLite cache)
    #[arg(long, conflicts_with = "cache_only")]
    search_only: bool,
}

impl ReindexCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        let vault_names = resolve_vault_names(global_config, explicit_vault)?;
        let client = reqwest::Client::new();
        let mut responses = Vec::with_capacity(vault_names.len());

        for vault_name in vault_names {
            let mut url = Url::parse(&format!("http://{}/", global_config.daemon.bind))
                .with_context(|| {
                    format!("invalid daemon bind address: {}", global_config.daemon.bind)
                })?;
            url.path_segments_mut()
                .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
                .push("api")
                .push("app")
                .push("vaults")
                .push(&vault_name)
                .push("reindex");

            {
                let mut query = url.query_pairs_mut();
                if self.cache_only {
                    query.append_pair("cache_only", "true");
                }
                if self.search_only {
                    query.append_pair("search_only", "true");
                }
            }

            let response = client.post(url).send().await.map_err(|error| {
                if error.is_connect() {
                    anyhow::anyhow!(
                        "could not reach the Notesmith daemon at {}. Start it with `notesmith daemon start`",
                        global_config.daemon.bind
                    )
                } else {
                    anyhow::anyhow!("reindex request failed: {error}")
                }
            })?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("reindex failed for {vault_name} with {status}: {body}");
            }

            responses.push(response.json::<Value>().await?);
        }

        match format {
            OutputFormat::Json => {
                if responses.len() == 1 {
                    println!("{}", serde_json::to_string_pretty(&responses[0])?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&responses)?);
                }
            }
            OutputFormat::Text => {
                for response in &responses {
                    let vault = response
                        .get("vault")
                        .and_then(Value::as_str)
                        .unwrap_or("<unknown>");
                    let notes = response.get("notes").and_then(Value::as_u64).unwrap_or(0);
                    println!("Reindexed {notes} notes for {vault}");
                }
            }
        }

        Ok(())
    }
}

fn resolve_vault_names(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(vault_name) = explicit_vault {
        if global_config.vault(vault_name).is_none() {
            anyhow::bail!("Vault '{vault_name}' is not registered");
        }
        return Ok(vec![vault_name.to_string()]);
    }

    let mut vault_names = global_config.vaults.keys().cloned().collect::<Vec<_>>();
    vault_names.sort();

    if vault_names.is_empty() {
        anyhow::bail!("No vaults registered. Add vaults to ~/.config/notesmith/config.toml");
    }

    Ok(vault_names)
}
