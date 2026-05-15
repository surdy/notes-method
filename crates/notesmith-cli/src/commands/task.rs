//! `notesmith task` subcommands: list, add, toggle, set-status

use std::path::Path;

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List tasks with optional filters
    List {
        /// Filter by status (todo, in_progress, blocked, waiting, on_hold, done, cancelled)
        #[arg(long)]
        status: Option<String>,
        /// Filter by customer name
        #[arg(long)]
        customer: Option<String>,
        /// Filter to tasks due before this date (YYYY-MM-DD)
        #[arg(long)]
        due_before: Option<String>,
        /// Maximum results
        #[arg(long, default_value = "200")]
        limit: usize,
    },
    /// Add a new task to a note
    Add {
        /// Note path (relative to vault root)
        note_path: String,
        /// Task description
        description: String,
        /// Associate with customer
        #[arg(long)]
        customer: Option<String>,
        /// Associate with stream
        #[arg(long)]
        stream: Option<String>,
        /// Due date (YYYY-MM-DD)
        #[arg(long)]
        due: Option<String>,
        /// Priority (highest, high, medium, low, lowest)
        #[arg(long)]
        priority: Option<String>,
    },
    /// Toggle a task to a new status using its content hash
    Toggle {
        /// Note path (relative to vault root)
        note_path: String,
        /// Blake3 content hash of the task line
        task_hash: String,
        /// New status (todo, in_progress, blocked, waiting, on_hold, done, cancelled)
        new_status: String,
    },
    /// Explicitly set a task's status (alias for toggle)
    SetStatus {
        /// Note path (relative to vault root)
        note_path: String,
        /// Blake3 content hash of the task line
        task_hash: String,
        /// New status (todo, in_progress, blocked, waiting, on_hold, done, cancelled)
        new_status: String,
    },
}

impl TaskCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        match self {
            TaskCommand::List {
                status,
                customer,
                due_before,
                limit,
            } => {
                cmd_list(
                    global_config,
                    explicit_vault,
                    cwd,
                    status.as_deref(),
                    customer.as_deref(),
                    due_before.as_deref(),
                    *limit,
                    format,
                )
                .await
            }
            TaskCommand::Add {
                note_path,
                description,
                customer,
                stream,
                due,
                priority,
            } => {
                cmd_add(
                    global_config,
                    explicit_vault,
                    cwd,
                    note_path,
                    description,
                    customer.as_deref(),
                    stream.as_deref(),
                    due.as_deref(),
                    priority.as_deref(),
                    format,
                )
                .await
            }
            TaskCommand::Toggle {
                note_path,
                task_hash,
                new_status,
            }
            | TaskCommand::SetStatus {
                note_path,
                task_hash,
                new_status,
            } => {
                cmd_toggle(
                    global_config,
                    explicit_vault,
                    cwd,
                    note_path,
                    task_hash,
                    new_status,
                    format,
                )
                .await
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn cmd_list(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    status: Option<&str>,
    customer: Option<&str>,
    due_before: Option<&str>,
    limit: usize,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let mut url = build_vault_url(global_config, &detected.name, "tasks")?;

    {
        let mut pairs = url.query_pairs_mut();
        if let Some(s) = status {
            pairs.append_pair("status", s);
        }
        if let Some(c) = customer {
            pairs.append_pair("customer", c);
        }
        if let Some(d) = due_before {
            pairs.append_pair("due_before", d);
        }
        pairs.append_pair("limit", &limit.to_string());
    }

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let code = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("task list failed with {code}: {body}");
    }

    let tasks = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&tasks)?),
        OutputFormat::Text => {
            let tasks = tasks.as_array().cloned().unwrap_or_default();
            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                for task in &tasks {
                    let status = task["status"].as_str().unwrap_or("?");
                    let text = task["text"].as_str().unwrap_or("?");
                    let note_path = task["note_path"].as_str().unwrap_or("?");
                    let due = task["due"].as_str().unwrap_or("");
                    let marker = status_to_marker(status);
                    if due.is_empty() {
                        println!("[{marker}] {text}  ({note_path})");
                    } else {
                        println!("[{marker}] {text}  📅 {due}  ({note_path})");
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_add(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    note_path: &str,
    description: &str,
    customer: Option<&str>,
    stream: Option<&str>,
    due: Option<&str>,
    priority: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, "tasks")?;

    let mut body = serde_json::json!({
        "note_path": note_path,
        "description": description,
    });
    if let Some(c) = customer {
        body["customer"] = c.into();
    }
    if let Some(s) = stream {
        body["stream"] = s.into();
    }
    if let Some(d) = due {
        body["due"] = d.into();
    }
    if let Some(p) = priority {
        body["priority"] = p.into();
    }

    let response = reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let code = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("task add failed with {code}: {text}");
    }

    let result = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
        OutputFormat::Text => {
            println!(
                "Task added to {}",
                result["path"].as_str().unwrap_or(note_path)
            );
        }
    }
    Ok(())
}

async fn cmd_toggle(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    note_path: &str,
    task_hash: &str,
    new_status: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, "tasks/toggle")?;

    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({
            "note_path": note_path,
            "task_hash": task_hash,
            "new_status": new_status,
        }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let code = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("task toggle failed with {code}: {text}");
    }

    let result = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
        OutputFormat::Text => {
            println!(
                "Task updated in {}",
                result["path"].as_str().unwrap_or(note_path)
            );
        }
    }
    Ok(())
}

fn build_vault_url(
    global_config: &GlobalConfig,
    vault_name: &str,
    endpoint: &str,
) -> anyhow::Result<Url> {
    let mut url = crate::daemon_client::daemon_url(global_config)?;
    let mut path_segments = url
        .path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?;
    path_segments.push("api").push("v").push(vault_name);
    for segment in endpoint.split('/').filter(|segment| !segment.is_empty()) {
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
            anyhow::anyhow!("task request failed: {error}")
        }
    }
}

fn status_to_marker(status: &str) -> char {
    match status {
        "todo" => ' ',
        "in_progress" => '/',
        "blocked" => 'b',
        "waiting" => 'w',
        "on_hold" => 'h',
        "done" => 'x',
        "cancelled" => '-',
        _ => '?',
    }
}
