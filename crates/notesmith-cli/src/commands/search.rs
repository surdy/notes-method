use std::path::Path;

use anyhow::Context;
use clap::Args;
use notesmith_config::{GlobalConfig, detect_vault};
use notesmith_index::SearchResult;
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct SearchCommand {
    /// Maximum number of results to return
    #[arg(long)]
    limit: Option<usize>,
    /// Search terms
    #[arg(required = true)]
    terms: Vec<String>,
}

impl SearchCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        let detected = detect_vault(cwd, explicit_vault, global_config)?;
        let mut url =
            Url::parse(&format!("http://{}/", global_config.daemon.bind)).with_context(|| {
                format!("invalid daemon bind address: {}", global_config.daemon.bind)
            })?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
            .push("api")
            .push("v")
            .push(&detected.name)
            .push("search");

        let query = self.terms.join(" ");
        {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("q", &query);
            if let Some(limit) = self.limit {
                query_pairs.append_pair("limit", &limit.to_string());
            }
        }

        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_err(|error| {
                if error.is_connect() {
                    anyhow::anyhow!(
                        "could not reach the Notesmith daemon at {}. Start it with `notesmith daemon start`",
                        global_config.daemon.bind
                    )
                } else {
                    anyhow::anyhow!("search request failed: {error}")
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("search failed with {status}: {body}");
        }

        let results: Vec<SearchResult> = response.json().await?;
        match format {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&results)?),
            OutputFormat::Text => print_results(&results),
        }

        Ok(())
    }
}

fn print_results(results: &[SearchResult]) {
    if results.is_empty() {
        println!("No results.");
        return;
    }

    for (index, result) in results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{}", result.path);
        println!("  title: {}", result.title);
        println!("  score: {:.3}", result.score);
        println!("  snippet: {}", result.snippet);
    }
}
