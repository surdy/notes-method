use std::path::Path;

use clap::{Subcommand, ValueEnum};
use notesmith_config::{GlobalConfig, detect_vault};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum PeriodicCommand {
    /// Open the current periodic note for a kind, creating it if missing
    Open {
        #[arg(value_enum)]
        kind: PeriodicKindArg,
        /// Offset from the current period (-1 = previous period, 1 = next period)
        #[arg(
            long,
            default_value_t = 0,
            allow_hyphen_values = true,
            allow_negative_numbers = true
        )]
        offset: i32,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PeriodicKindArg {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl PeriodicKindArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }
}

impl PeriodicCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        match self {
            Self::Open { kind, offset } => {
                cmd_open(global_config, explicit_vault, cwd, *kind, *offset, format).await
            }
        }
    }
}

async fn cmd_open(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    kind: PeriodicKindArg,
    offset: i32,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let mut url = crate::daemon_client::daemon_url(global_config)?;
    {
        let mut path_segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?;
        path_segments
            .push("api")
            .push("v")
            .push(&detected.name)
            .push("periodic")
            .push(kind.as_str())
            .push("current");
    }
    url.query_pairs_mut()
        .append_pair("offset", &offset.to_string());

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                anyhow::anyhow!(
                    "could not reach the Notesmith daemon at {}",
                    global_config.daemon.bind
                )
            } else {
                anyhow::anyhow!("periodic request failed: {error}")
            }
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("periodic open failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print!("{}", json["content"].as_str().unwrap_or_default()),
    }

    Ok(())
}
