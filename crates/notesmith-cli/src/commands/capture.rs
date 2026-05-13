//! `notesmith capture` quick-capture command.

use std::path::Path;

use anyhow::Context;
use clap::Args;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct CaptureCommand {
    /// Note text content
    text: String,
    /// Optional title (used in filename)
    #[arg(long)]
    title: Option<String>,
}

impl CaptureCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        cmd_capture(
            global_config,
            explicit_vault,
            cwd,
            &self.text,
            self.title.as_deref(),
            format,
        )
        .await
    }
}

async fn cmd_capture(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    text: &str,
    title: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["capture"])?;
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
            anyhow::anyhow!("capture request failed: {error}")
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
        anyhow::bail!("capture request failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print_text(&json),
    }

    Ok(())
}
