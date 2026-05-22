use std::path::Path;

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum TemplateCommand {
    /// List available templates
    List,
    /// Render a template to stdout without creating a file
    Render {
        /// Template name
        name: String,
        /// Prompt values as KEY=VALUE
        #[arg(long = "prompt", value_parser = parse_key_val, action = clap::ArgAction::Append)]
        prompts: Vec<(String, String)>,
    },
    /// Render and create the note at the computed output path
    Instantiate {
        /// Template name
        name: String,
        /// Prompt values as KEY=VALUE
        #[arg(long = "prompt", value_parser = parse_key_val, action = clap::ArgAction::Append)]
        prompts: Vec<(String, String)>,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("expected KEY=VALUE, got `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

impl TemplateCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        match self {
            TemplateCommand::List => cmd_list(global_config, explicit_vault, cwd, format).await,
            TemplateCommand::Render { name, prompts } => {
                cmd_render(global_config, explicit_vault, cwd, name, prompts, format).await
            }
            TemplateCommand::Instantiate { name, prompts } => {
                cmd_instantiate(global_config, explicit_vault, cwd, name, prompts, format).await
            }
        }
    }
}

async fn cmd_list(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["templates"], None)?;
    let response = reqwest::get(url)
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("template list failed with {status}: {body}");
    }

    let templates = response.json::<Vec<serde_json::Value>>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&templates)?),
        OutputFormat::Text => {
            for t in &templates {
                let name = t["name"].as_str().unwrap_or_default();
                let desc = t["description"].as_str().unwrap_or_default();
                println!("{name:20} {desc}");
            }
        }
    }
    Ok(())
}

async fn cmd_render(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    name: &str,
    prompts: &[(String, String)],
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(
        global_config,
        &detected.name,
        &["templates", name, "render"],
        None,
    )?;
    let prompts_map: std::collections::HashMap<String, String> = prompts.iter().cloned().collect();
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "prompts": prompts_map }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("template render failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => {
            print!("{}", json["content"].as_str().unwrap_or_default());
        }
    }
    Ok(())
}

async fn cmd_instantiate(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    name: &str,
    prompts: &[(String, String)],
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(
        global_config,
        &detected.name,
        &["templates", name, "instantiate"],
        None,
    )?;
    let prompts_map: std::collections::HashMap<String, String> = prompts.iter().cloned().collect();
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "prompts": prompts_map }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("template instantiate failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => {
            println!("{}", json["path"].as_str().unwrap_or_default());
        }
    }
    Ok(())
}

fn build_vault_url(
    global_config: &GlobalConfig,
    vault_name: &str,
    prefix_segments: &[&str],
    note_path: Option<&str>,
) -> anyhow::Result<Url> {
    let mut url = crate::daemon_client::daemon_url(global_config)?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?;
    segments.push("api").push("v").push(vault_name);
    for segment in prefix_segments {
        segments.push(segment);
    }
    if let Some(note_path) = note_path {
        for segment in note_path.split('/').filter(|segment| !segment.is_empty()) {
            segments.push(segment);
        }
    }
    drop(segments);
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
            anyhow::anyhow!("template request failed: {error}")
        }
    }
}
