//! `notesmith kit` — install a blessed vault configuration.
//!
//! Purely local filesystem work: no daemon required, so a vault can be
//! scaffolded before anything is running.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use notesmith_kit::{ApplyOptions, Kit};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum KitCommand {
    /// List the available vault kits
    List,
    /// Show the files a kit installs
    Show {
        /// Kit id (e.g. work-notes)
        id: String,
    },
    /// Install a kit's config, templates and dashboards into a vault
    Apply {
        /// Kit id (e.g. work-notes)
        id: String,
        /// Target directory (defaults to the detected vault; created if missing)
        #[arg(long)]
        path: Option<PathBuf>,
        /// Vault name written into vault.toml (defaults to the detected vault
        /// name, or the target directory name)
        #[arg(long)]
        name: Option<String>,
        /// Report what would change without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Overwrite files that already exist
        #[arg(long)]
        force: bool,
    },
}

impl KitCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            KitCommand::List => cmd_list(format),
            KitCommand::Show { id } => cmd_show(id, format),
            KitCommand::Apply {
                id,
                path,
                name,
                dry_run,
                force,
            } => cmd_apply(
                id,
                path.as_deref(),
                name.as_deref(),
                *dry_run,
                *force,
                global_config,
                explicit_vault,
                cwd,
                format,
            ),
        }
    }
}

fn cmd_list(format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Text => {
            for kit in Kit::all() {
                println!("{}", kit.id());
                println!("  {}", kit.description());
                println!(
                    "  {} files, {} folders",
                    kit.files().len(),
                    kit.folders().len()
                );
            }
        }
        OutputFormat::Json => {
            let kits: Vec<_> = Kit::all()
                .iter()
                .map(|kit| {
                    serde_json::json!({
                        "id": kit.id(),
                        "description": kit.description(),
                        "files": kit.files().len(),
                        "folders": kit.folders().len(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&kits)?);
        }
    }
    Ok(())
}

fn cmd_show(id: &str, format: OutputFormat) -> anyhow::Result<()> {
    let kit = Kit::require(id)?;
    let files: Vec<&str> = kit.files().iter().map(|(path, _)| *path).collect();

    match format {
        OutputFormat::Text => {
            println!("{} — {}", kit.id(), kit.description());
            println!("\nFolders:");
            for folder in kit.folders() {
                println!("  {folder}/");
            }
            println!("\nFiles:");
            for file in &files {
                println!("  {file}");
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": kit.id(),
                "description": kit.description(),
                "folders": kit.folders(),
                "files": files,
            }))?
        ),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_apply(
    id: &str,
    path: Option<&Path>,
    name: Option<&str>,
    dry_run: bool,
    force: bool,
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let kit = Kit::require(id)?;

    // An explicit --path wins; otherwise scaffold the vault we are standing in.
    let (root, default_name) = match path {
        Some(path) => {
            let root = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            let default_name = root
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "vault".to_string());
            (root, default_name)
        }
        None => {
            let detected = detect_vault(cwd, explicit_vault, global_config)?;
            (detected.root.clone(), detected.name.clone())
        }
    };

    let vault_name = name.unwrap_or(&default_name).to_string();
    let options = ApplyOptions::for_vault(&vault_name)
        .force(force)
        .dry_run(dry_run);
    let report = kit.apply(&root, &options)?;

    match format {
        OutputFormat::Text => {
            let verb = if dry_run { "Would apply" } else { "Applied" };
            println!(
                "{verb} kit '{}' to {} (vault name: {vault_name})",
                kit.id(),
                root.display()
            );
            for folder in &report.created_dirs {
                println!("  + {folder}/");
            }
            for file in &report.written {
                println!("  + {file}");
            }
            for file in &report.skipped {
                println!("  = {file} (exists, left alone)");
            }
            if report.is_noop() {
                println!("Nothing to do — the kit is already installed.");
            } else if !report.skipped.is_empty() && !force {
                println!(
                    "\n{} file(s) already existed and were not modified. Re-run with --force to overwrite.",
                    report.skipped.len()
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "kit": kit.id(),
                "path": root,
                "vault_name": vault_name,
                "dry_run": report.dry_run,
                "created_dirs": report.created_dirs,
                "written": report.written,
                "skipped": report.skipped,
            }))?
        ),
    }

    Ok(())
}
