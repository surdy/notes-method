//! `notesmith inbox` subcommands: add, list

use std::path::Path;

use anyhow::Context;
use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum InboxCommand {
    /// Quick-capture a note to the inbox
    Add {
        /// Note text content
        text: String,
        /// Optional title (used in filename)
        #[arg(long)]
        title: Option<String>,
    },
    /// List unarchived notes in the inbox
    List,
}

impl InboxCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            InboxCommand::Add { text, title } => {
                cmd_add(
                    global_config,
                    explicit_vault,
                    cwd,
                    text,
                    title.as_deref(),
                    format,
                )
                .await
            }
            InboxCommand::List => cmd_list(global_config, explicit_vault, cwd, format).await,
        }
    }
}

async fn cmd_add(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    text: &str,
    title: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["inbox"])?;
    let mut body = serde_json::json!({ "text": text });
    if let Some(title) = title {
        body["title"] = serde_json::json!(title);
    }
    let response = reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        println!("{}", json["path"].as_str().unwrap_or_default());
    })
    .await
}

async fn cmd_list(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["inbox"])?;
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("inbox list failed with {status}: {body}");
    }

    let notes = response.json::<Vec<serde_json::Value>>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&notes)?),
        OutputFormat::Text => {
            for note in &notes {
                let path = note["path"].as_str().unwrap_or_default();
                let title = note["title"].as_str().unwrap_or_default();
                println!("{path}  {title}");
            }
        }
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
            anyhow::anyhow!("inbox request failed: {error}")
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
        anyhow::bail!("inbox request failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print_text(&json),
    }

    Ok(())
}
