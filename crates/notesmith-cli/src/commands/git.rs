//! `notesmith git` subcommands: status, pull, push, sync, log

use std::path::Path;

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum GitCommand {
    /// Show working tree status
    Status,
    /// Pull from remote (fast-forward only)
    Pull,
    /// Push to remote
    Push,
    /// Pull then push (sync)
    Sync,
    /// Show recent commits
    Log {
        /// Number of commits to show
        #[arg(short, long, default_value = "10")]
        count: usize,
    },
}

impl GitCommand {
    pub fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        let detected = detect_vault(cwd, explicit_vault, global_config)?;
        let root = &detected.root;

        if !notesmith_git::ops::is_git_repo(root) {
            anyhow::bail!("vault '{}' is not a git repository", detected.name);
        }

        match self {
            GitCommand::Status => cmd_status(root, format),
            GitCommand::Pull => cmd_pull(root, format),
            GitCommand::Push => cmd_push(root, format),
            GitCommand::Sync => cmd_sync(root, format),
            GitCommand::Log { count } => cmd_log(root, *count, format),
        }
    }
}

fn cmd_status(root: &Path, format: OutputFormat) -> anyhow::Result<()> {
    let status = notesmith_git::ops::status(root)?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        OutputFormat::Text => {
            if status.clean {
                println!("Working tree clean");
            } else {
                if !status.staged.is_empty() {
                    println!("Staged:");
                    for f in &status.staged {
                        println!("  {f}");
                    }
                }
                if !status.changed.is_empty() {
                    println!("Changed:");
                    for f in &status.changed {
                        println!("  {f}");
                    }
                }
                if !status.untracked.is_empty() {
                    println!("Untracked:");
                    for f in &status.untracked {
                        println!("  {f}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_pull(root: &Path, format: OutputFormat) -> anyhow::Result<()> {
    let result = notesmith_git::ops::pull_ff(root, "origin")?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Text => {
            if result.conflict {
                println!("Pull aborted: not fast-forwardable (conflict)");
            } else if result.updated {
                println!("Updated to {}", result.new_head.as_deref().unwrap_or("unknown"));
            } else {
                println!("Already up to date");
            }
        }
    }
    Ok(())
}

fn cmd_push(root: &Path, format: OutputFormat) -> anyhow::Result<()> {
    let result = notesmith_git::ops::push(root, "origin")?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Text => {
            if result.pushed {
                println!("Pushed successfully");
            } else if let Some(ref err) = result.error {
                println!("Push failed: {err}");
            }
        }
    }
    Ok(())
}

fn cmd_sync(root: &Path, format: OutputFormat) -> anyhow::Result<()> {
    let pull_result = notesmith_git::ops::pull_ff(root, "origin")?;
    if pull_result.conflict {
        match format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "pull": pull_result,
                        "push": null,
                        "error": "pull conflict, push skipped",
                    }))?
                );
            }
            OutputFormat::Text => {
                println!("Pull aborted: not fast-forwardable (conflict)");
                println!("Push skipped");
            }
        }
        return Ok(());
    }

    let push_result = notesmith_git::ops::push(root, "origin")?;

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "pull": pull_result,
                    "push": push_result,
                }))?
            );
        }
        OutputFormat::Text => {
            if pull_result.updated {
                println!(
                    "Pulled to {}",
                    pull_result.new_head.as_deref().unwrap_or("unknown")
                );
            } else {
                println!("Already up to date");
            }
            if push_result.pushed {
                println!("Pushed successfully");
            } else if let Some(ref err) = push_result.error {
                println!("Push failed: {err}");
            }
        }
    }
    Ok(())
}

fn cmd_log(root: &Path, count: usize, format: OutputFormat) -> anyhow::Result<()> {
    let entries = notesmith_git::ops::log(root, count)?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        OutputFormat::Text => {
            if entries.is_empty() {
                println!("No commits");
                return Ok(());
            }
            for (i, entry) in entries.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!("{} {}", &entry.sha[..8], entry.message);
                println!("  {} | {}", entry.author, entry.timestamp);
            }
        }
    }
    Ok(())
}
