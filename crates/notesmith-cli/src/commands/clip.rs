//! `notesmith clip` — clip a web page into the vault via the daemon.
//!
//! A thin trigger over [`POST /api/v/{vault}/clip`](../../../docs/http-api.md):
//! extraction happens server-side, the CLI only hands over the URL (and any
//! extra tags). See [ADR 0020](../../../docs/adr/0020-web-clipper.md).

use std::path::Path;

use clap::Args;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct ClipCommand {
    /// URL of the page to clip
    url: String,
    /// Extra tag to add alongside the mandatory `inbox` tag (repeatable)
    #[arg(long = "tag")]
    tags: Vec<String>,
}

impl ClipCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        cmd_clip(
            global_config,
            explicit_vault,
            cwd,
            &self.url,
            &self.tags,
            format,
        )
        .await
    }
}

async fn cmd_clip(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    url: &str,
    tags: &[String],
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let endpoint = build_vault_url(global_config, &detected.name, &["clip"])?;
    let body = serde_json::json!({ "url": url, "tags": tags });

    let response = reqwest::Client::new()
        .post(endpoint)
        .json(&body)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        let path = json["path"].as_str().unwrap_or_default();
        if json["duplicate"].as_bool().unwrap_or(false) {
            println!("already clipped: {path}");
        } else {
            println!("{path}");
        }
    })
    .await
}

fn build_vault_url(
    global_config: &GlobalConfig,
    vault_name: &str,
    segments: &[&str],
) -> anyhow::Result<Url> {
    let mut url = crate::daemon_client::daemon_url(global_config)?;
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
                "could not reach the Notesmith daemon at {}",
                global_config.daemon.bind
            )
        } else {
            anyhow::anyhow!("clip request failed: {error}")
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
        anyhow::bail!("clip request failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print_text(&json),
    }

    Ok(())
}
