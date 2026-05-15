use std::path::Path;

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum RouteCommand {
    /// Preview where a note would be routed
    Preview {
        /// Path to the note (relative to vault root)
        path: String,
    },
    /// Apply routing to move note(s) to their destination
    Apply {
        /// Path to the note (relative to vault root)
        path: String,
    },
}

impl RouteCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        let detected = detect_vault(cwd, explicit_vault, global_config)?;

        match self {
            RouteCommand::Preview { path } => {
                let url = build_route_url(global_config, &detected.name, "preview")?;
                let response = reqwest::Client::new()
                    .post(url)
                    .json(&serde_json::json!({ "path": path }))
                    .send()
                    .await
                    .map_err(map_request_error(global_config))?;

                print_json_response(response, format, |json| {
                    println!(
                        "{} -> {} (rule: {})",
                        json["path"].as_str().unwrap_or_default(),
                        json["destination"].as_str().unwrap_or_default(),
                        json["rule_id"].as_str().unwrap_or_default(),
                    );
                })
                .await
            }
            RouteCommand::Apply { path } => {
                let url = build_route_url(global_config, &detected.name, "apply")?;
                let body = serde_json::json!({ "paths": [path] });
                let response = reqwest::Client::new()
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(map_request_error(global_config))?;

                print_json_response(response, format, |json| {
                    let routed = json["routed"].as_u64().unwrap_or(0);
                    println!("routed {routed} note(s)");
                    if let Some(results) = json["results"].as_array() {
                        for r in results {
                            println!(
                                "  {} -> {}",
                                r["from"].as_str().unwrap_or_default(),
                                r["to"].as_str().unwrap_or_default(),
                            );
                        }
                    }
                })
                .await
            }
        }
    }
}

fn build_route_url(
    global_config: &GlobalConfig,
    vault_name: &str,
    action: &str,
) -> anyhow::Result<reqwest::Url> {
    let url = crate::daemon_client::daemon_url(global_config)?;
    Ok(url.join(&format!("api/v/{vault_name}/route/{action}"))?)
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
            anyhow::anyhow!("route request failed: {error}")
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
        anyhow::bail!("route request failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print_text(&json),
    }

    Ok(())
}
