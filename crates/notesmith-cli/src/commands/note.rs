use std::{io::Read, path::Path};

use anyhow::Context;
use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum NoteCommand {
    /// Create a new note
    Create {
        /// Note title
        title: String,
        /// Folder to create in (default: Inbox)
        #[arg(long)]
        folder: Option<String>,
        /// Initial body content
        #[arg(long)]
        content: Option<String>,
    },
    /// Get a note's content and metadata
    Get {
        /// Note path (relative to vault root)
        path: String,
    },
    /// Replace a note's content
    Put {
        /// Note path
        path: String,
        /// Read content from stdin
        #[arg(long)]
        from_stdin: bool,
        /// Content to write (if not --from-stdin)
        #[arg(long)]
        content: Option<String>,
    },
    /// Append content to a note
    Append {
        /// Note path
        path: String,
        /// Content to append
        content: String,
    },
    /// Delete a note
    Delete {
        /// Note path
        path: String,
    },
    /// Move a note to a new path
    Move {
        /// Source path
        src: String,
        /// Destination path
        dst: String,
    },
}

impl NoteCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        match self {
            NoteCommand::Create {
                title,
                folder,
                content,
            } => {
                cmd_create(
                    global_config,
                    explicit_vault,
                    cwd,
                    title,
                    folder.as_deref(),
                    content.as_deref(),
                    format,
                )
                .await
            }
            NoteCommand::Get { path } => {
                cmd_get(global_config, explicit_vault, cwd, path, format).await
            }
            NoteCommand::Put {
                path,
                from_stdin,
                content,
            } => {
                cmd_put(
                    global_config,
                    explicit_vault,
                    cwd,
                    path,
                    *from_stdin,
                    content.as_deref(),
                    format,
                )
                .await
            }
            NoteCommand::Append { path, content } => {
                cmd_append(global_config, explicit_vault, cwd, path, content, format).await
            }
            NoteCommand::Delete { path } => {
                cmd_delete(global_config, explicit_vault, cwd, path, format).await
            }
            NoteCommand::Move { src, dst } => {
                cmd_move(global_config, explicit_vault, cwd, src, dst, format).await
            }
        }
    }
}

async fn cmd_create(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    title: &str,
    folder: Option<&str>,
    content: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["notes"], None)?;
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({
            "title": title,
            "folder": folder,
            "content": content,
        }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        println!("{}", json["path"].as_str().unwrap_or_default());
    })
    .await
}

async fn cmd_get(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    path: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["notes"], Some(path))?;
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("note get failed with {status}: {body}");
    }

    let note = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&note)?),
        OutputFormat::Text => print!("{}", note["body"].as_str().unwrap_or_default()),
    }

    Ok(())
}

async fn cmd_put(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    path: &str,
    from_stdin: bool,
    content: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["notes"], Some(path))?;
    let content = if from_stdin {
        read_stdin()?
    } else {
        content
            .map(ToOwned::to_owned)
            .context("provide --content or --from-stdin")?
    };
    let response = reqwest::Client::new()
        .put(url)
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        println!("{}", json["path"].as_str().unwrap_or_default());
    })
    .await
}

async fn cmd_append(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    path: &str,
    content: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["notes-append"], Some(path))?;
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        println!("{}", json["path"].as_str().unwrap_or_default());
    })
    .await
}

async fn cmd_delete(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    path: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["notes"], Some(path))?;
    let response = reqwest::Client::new()
        .delete(url)
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("note delete failed with {status}: {body}");
    }

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({ "path": path }))?
            );
        }
        OutputFormat::Text => println!("{path}"),
    }

    Ok(())
}

async fn cmd_move(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    src: &str,
    dst: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let url = build_vault_url(global_config, &detected.name, &["notes-move"], Some(src))?;
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "destination": dst }))
        .send()
        .await
        .map_err(map_request_error(global_config))?;

    print_json_response(response, format, |json| {
        println!(
            "{} -> {}",
            json["from"].as_str().unwrap_or_default(),
            json["to"].as_str().unwrap_or_default()
        );
    })
    .await
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

fn read_stdin() -> anyhow::Result<String> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    Ok(input)
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
            anyhow::anyhow!("note request failed: {error}")
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
        anyhow::bail!("note request failed with {status}: {body}");
    }

    let json = response.json::<serde_json::Value>().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        OutputFormat::Text => print_text(&json),
    }

    Ok(())
}
