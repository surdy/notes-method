//! `notesmith url-open` — handle `notesmith://` deep-link URLs via the daemon API.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use clap::Args;
use notesmith_config::{GlobalConfig, detect_vault};
use notesmith_core::url_actions::{self, ActionStep, UrlActionsError};
use notesmith_core::url_scheme::{self, NotesmithUrl};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct UrlOpenCommand {
    /// The notesmith:// URL to open
    pub url: String,
}

impl UrlOpenCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        let parsed = url_scheme::parse_notesmith_url(&self.url)
            .map_err(|e| anyhow::anyhow!("invalid notesmith URL: {e}"))?;

        match parsed {
            NotesmithUrl::Open { vault, path } => {
                let url = build_vault_url(global_config, &vault, &["notes", &path])?;
                let response = daemon_get(global_config, url).await?;
                print_response(response, format).await
            }
            NotesmithUrl::Daily { vault } => {
                let url = build_vault_url(global_config, &vault, &["daily", "today"])?;
                let response = daemon_post(global_config, url, None).await?;
                print_response(response, format).await
            }
            NotesmithUrl::Search { vault, query } => {
                let mut url = build_vault_url(global_config, &vault, &["search"])?;
                url.query_pairs_mut().append_pair("q", &query);
                let response = daemon_get(global_config, url).await?;
                print_response(response, format).await
            }
            NotesmithUrl::New {
                vault,
                template,
                folder,
            } => {
                let template_name = template.as_deref().unwrap_or("default");
                let url = build_vault_url(
                    global_config,
                    &vault,
                    &["templates", template_name, "instantiate"],
                )?;
                let mut body = serde_json::Map::new();
                if let Some(f) = folder {
                    body.insert("folder".into(), serde_json::Value::String(f));
                }
                let response =
                    daemon_post(global_config, url, Some(serde_json::Value::Object(body))).await?;
                print_response(response, format).await
            }
            NotesmithUrl::Inbox { vault, text } => {
                let url = build_vault_url(global_config, &vault, &["inbox"])?;
                let body = serde_json::json!({ "text": text });
                let response = daemon_post(global_config, url, Some(body)).await?;
                print_response(response, format).await
            }
            NotesmithUrl::Task {
                vault,
                path,
                line_hash,
                status,
            } => {
                let url = build_vault_url(global_config, &vault, &["tasks", "toggle"])?;
                let body = serde_json::json!({
                    "path": path,
                    "line_hash": line_hash,
                    "status": status,
                });
                let response = daemon_post(global_config, url, Some(body)).await?;
                print_response(response, format).await
            }
            NotesmithUrl::Command { command_name, args } => {
                println!("command: {command_name}");
                for (key, value) in &args {
                    println!("  {key}: {value}");
                }
                println!("\n(Commands are handled by the desktop app.)");
                Ok(())
            }
            NotesmithUrl::UserAction {
                action_name,
                params,
            } => {
                run_user_action(
                    global_config,
                    explicit_vault,
                    cwd,
                    &action_name,
                    &params,
                    format,
                )
                .await
            }
        }
    }
}

async fn run_user_action(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    action_name: &str,
    params: &HashMap<String, String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let vault_root = &detected.root;
    let actions_file = url_actions::load_url_actions(vault_root).map_err(|e| match e {
        UrlActionsError::Io { path, .. } => {
            anyhow::anyhow!("no url-actions.yaml found at {path}")
        }
        other => anyhow::anyhow!("{other}"),
    })?;

    let action = url_actions::find_action(&actions_file, action_name)
        .ok_or_else(|| anyhow::anyhow!("unknown user action: {action_name}"))?;

    for step in &action.steps {
        match step {
            ActionStep::ApiCall { method, path, body } => {
                let interpolated_path = url_actions::interpolate(path, params);
                let base = format!("http://{}", global_config.daemon.bind);
                let url_str = format!("{}{}", base.trim_end_matches('/'), interpolated_path);
                let url: Url = url_str
                    .parse()
                    .with_context(|| format!("invalid URL: {url_str}"))?;

                let client = reqwest::Client::new();
                let request = match method.to_uppercase().as_str() {
                    "GET" => client.get(url),
                    "POST" => {
                        let mut req = client.post(url);
                        if let Some(body) = body {
                            let interpolated = interpolate_json(body, params);
                            req = req.json(&interpolated);
                        }
                        req
                    }
                    "PUT" => {
                        let mut req = client.put(url);
                        if let Some(body) = body {
                            let interpolated = interpolate_json(body, params);
                            req = req.json(&interpolated);
                        }
                        req
                    }
                    other => anyhow::bail!("unsupported HTTP method: {other}"),
                };

                let response = request
                    .send()
                    .await
                    .map_err(map_request_error(global_config))?;
                print_response(response, format).await?;
            }
            ActionStep::OpenNote { vault, path } => {
                let interpolated_path = url_actions::interpolate(path, params);
                let interpolated_vault = url_actions::interpolate(vault, params);
                let url = build_vault_url(
                    global_config,
                    &interpolated_vault,
                    &["notes", &interpolated_path],
                )?;
                let response = daemon_get(global_config, url).await?;
                print_response(response, format).await?;
            }
        }
    }

    Ok(())
}

/// Recursively interpolate `{param}` placeholders in JSON string values.
fn interpolate_json(
    value: &serde_json::Value,
    params: &HashMap<String, String>,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            serde_json::Value::String(url_actions::interpolate(s, params))
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), interpolate_json(v, params)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| interpolate_json(v, params)).collect())
        }
        other => other.clone(),
    }
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

async fn daemon_get(global_config: &GlobalConfig, url: Url) -> anyhow::Result<reqwest::Response> {
    reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(map_request_error(global_config))
}

async fn daemon_post(
    global_config: &GlobalConfig,
    url: Url,
    body: Option<serde_json::Value>,
) -> anyhow::Result<reqwest::Response> {
    let mut req = reqwest::Client::new().post(url);
    if let Some(body) = body {
        req = req.json(&body);
    }
    req.send().await.map_err(map_request_error(global_config))
}

async fn print_response(response: reqwest::Response, format: OutputFormat) -> anyhow::Result<()> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("request failed with {status}: {body}");
    }

    let json: serde_json::Value = response.json().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => {
            if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
                println!("{path}");
            } else {
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
        }
    }

    Ok(())
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
            anyhow::anyhow!("url-open request failed: {error}")
        }
    }
}
