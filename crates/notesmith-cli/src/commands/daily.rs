//! `notesmith daily` subcommands: ensure, open

use std::path::Path;

use anyhow::Context;
use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum DailyCommand {
    /// Create daily note for a date if it doesn't exist
    Ensure {
        /// Date in YYYY-MM-DD format (defaults to today)
        #[arg(long)]
        date: Option<String>,
    },
    /// Open today's (or specified date's) daily note
    Open {
        /// Date in YYYY-MM-DD format (defaults to today)
        #[arg(long)]
        date: Option<String>,
    },
}

impl DailyCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            DailyCommand::Ensure { date } => {
                cmd_ensure(global_config, explicit_vault, cwd, date.as_deref(), format).await
            }
            DailyCommand::Open { date } => {
                cmd_open(global_config, explicit_vault, cwd, date.as_deref(), format).await
            }
        }
    }
}

fn resolve_date(date: Option<&str>) -> String {
    date.map(|d| d.to_string())
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string())
}

async fn cmd_ensure(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    date: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let date_str = resolve_date(date);
    let url = build_vault_url(global_config, &detected.name, &["daily", &date_str])?;
    let response = reqwest::Client::new()
        .post(url)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        let path = json["path"].as_str().unwrap_or_default();
        let created = json["created"].as_bool().unwrap_or(false);
        if created {
            println!("Created {path}");
        } else {
            println!("{path} (already exists)");
        }
    })
    .await
}

async fn cmd_open(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    date: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let date_str = resolve_date(date);
    let url = build_vault_url(global_config, &detected.name, &["daily", &date_str])?;
    let client = reqwest::Client::new();

    // Ensure the note exists first
    let _ = client
        .post(url.clone())
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    // Then fetch and display it
    let response = client
        .get(url)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("daily open failed with {status}: {body}");
    }

    let note = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&note)?),
        OutputFormat::Text => print!("{}", note["content"].as_str().unwrap_or_default()),
    }

    Ok(())
}

fn build_vault_url(
    global_config: &GlobalConfig,
    vault_name: &str,
    segments: &[&str],
) -> anyhow::Result<Url> {
    let mut url = Url::parse(&format!("http://{}/", global_config.daemon.bind))
        .with_context(|| format!("invalid daemon bind address: {}", global_config.daemon.bind))?;
    let mut path_segments = url
        .path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?;
    path_segments.push("api").push("v").push(vault_name);
    for segment in segments {
        path_segments.push(segment);
    }
    drop(path_segments);
    Ok(url)
}

fn map_request_error<'a>(
    global_config: &'a GlobalConfig,
) -> impl Fn(reqwest::Error) -> anyhow::Error + 'a {
    move |error| {
        if error.is_connect() {
            anyhow::anyhow!(
                "could not reach the Notesmith daemon at {}. Start it with `notesmith daemon start`",
                global_config.daemon.bind
            )
        } else {
            anyhow::anyhow!("daily request failed: {error}")
        }
    }
}

async fn print_json_response(
    response: reqwest::Response,
    format: OutputFormat,
    print_text: impl FnOnce(&serde_json::Value),
) -> anyhow::Result<()> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("daily request failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print_text(&json),
    }

    Ok(())
}
