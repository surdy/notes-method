//! `notesmith vault` subcommands: list, detect, info

use clap::Subcommand;
use notesmith_config::{DetectionSource, GlobalConfig, VaultConfig, detect_vault};
use notesmith_core::VaultEngine;
use notesmith_http::{cache_path_for_vault, search_index_path_for_vault};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use std::path::Path;

#[derive(Debug, Subcommand)]
pub enum VaultCommand {
    /// List all registered vaults from global config
    List,
    /// Show which vault would be selected for the current directory
    Detect,
    /// Show vault path, name, and config summary
    Info,
    /// Rebuild the local SQLite cache for the detected vault
    Reindex,
}

impl VaultCommand {
    pub fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            VaultCommand::List => cmd_list(global_config, format),
            VaultCommand::Detect => cmd_detect(global_config, explicit_vault, cwd, format),
            VaultCommand::Info => cmd_info(global_config, explicit_vault, cwd, format),
            VaultCommand::Reindex => cmd_reindex(global_config, explicit_vault, cwd),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

fn cmd_list(global_config: &GlobalConfig, format: OutputFormat) -> anyhow::Result<()> {
    let default_name = global_config.effective_default();

    match format {
        OutputFormat::Text => {
            if global_config.vaults.is_empty() {
                println!("No vaults registered. Add vaults to ~/.config/notesmith/config.toml");
                return Ok(());
            }
            for (name, reg) in &global_config.vaults {
                let marker = if Some(name.as_str()) == default_name {
                    " (default)"
                } else {
                    ""
                };
                println!("{name}{marker}  {}", reg.path.display());
            }
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = global_config
                .vaults
                .iter()
                .map(|(name, reg)| {
                    serde_json::json!({
                        "name": name,
                        "path": reg.path,
                        "default": Some(name.as_str()) == default_name,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
    }
    Ok(())
}

fn cmd_detect(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match detect_vault(cwd, explicit_vault, global_config) {
        Ok(detected) => {
            let source_str = match detected.source {
                DetectionSource::DirectoryWalk => "directory walk",
                DetectionSource::ExplicitFlag => "--vault flag",
                DetectionSource::DefaultConfig => "default config",
            };
            match format {
                OutputFormat::Text => {
                    println!("Vault:  {}", detected.name);
                    println!("Root:   {}", detected.root.display());
                    println!("Source: {source_str}");
                }
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "name": detected.name,
                            "root": detected.root,
                            "source": source_str,
                        }))?
                    );
                }
            }
        }
        Err(e) => {
            match format {
                OutputFormat::Text => {
                    eprintln!("No vault detected: {e}");
                }
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "error": e.to_string(),
                        }))?
                    );
                }
            }
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_info(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let vault_config = VaultConfig::load_from_vault(&detected.root)?;

    match format {
        OutputFormat::Text => {
            println!("Vault:    {}", vault_config.name);
            println!("Root:     {}", detected.root.display());
            if let Some(ref homepage) = vault_config.homepage {
                println!("Homepage: {homepage}");
            }
            println!();
            println!("[capture]");
            println!("  folder:   {}", vault_config.capture.folder);
            println!("  template: {}", vault_config.capture.template);
            println!();
            println!("[daily]");
            println!("  folder:   {}", vault_config.daily.folder);
            println!("  template: {}", vault_config.daily.template);
            if let Some(ref generate_at) = vault_config.daily.generate_at {
                println!("  generate_at: {generate_at}");
            }
            println!("  catch_up: {}", vault_config.daily.catch_up);
            println!();
            println!("[editor]");
            println!("  live_preview:  {}", vault_config.editor.live_preview);
            println!("  default_mode:  {}", vault_config.editor.default_mode);
            println!(
                "  strict_line_breaks:  {}",
                vault_config.editor.strict_line_breaks
            );
            println!(
                "  show_line_numbers:  {}",
                vault_config.editor.show_line_numbers
            );
            println!(
                "  hide_duplicate_h1:  {}",
                vault_config.editor.hide_duplicate_h1
            );
            println!(
                "  paste_url_image_whitelist:  {}",
                vault_config.editor.paste_url_image_whitelist
            );
            println!();
            println!("[git]");
            println!("  enabled: {}", vault_config.git.enabled);
            if let Some(ref interval) = vault_config.git.auto_commit_every {
                println!("  auto_commit_every: {interval}");
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": vault_config.name,
                    "root": detected.root,
                    "homepage": vault_config.homepage,
                    "capture": {
                        "folder": vault_config.capture.folder,
                        "template": vault_config.capture.template,
                    },
                    "daily": {
                        "folder": vault_config.daily.folder,
                        "template": vault_config.daily.template,
                        "generate_at": vault_config.daily.generate_at,
                        "catch_up": vault_config.daily.catch_up,
                    },
                    "editor": {
                        "live_preview": vault_config.editor.live_preview,
                        "default_mode": vault_config.editor.default_mode,
                        "strict_line_breaks": vault_config.editor.strict_line_breaks,
                        "show_line_numbers": vault_config.editor.show_line_numbers,
                        "hide_duplicate_h1": vault_config.editor.hide_duplicate_h1,
                        "paste_url_image_whitelist": vault_config.editor.paste_url_image_whitelist,
                    },
                    "git": {
                        "enabled": vault_config.git.enabled,
                    },
                }))?
            );
        }
    }
    Ok(())
}

fn cmd_reindex(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let engine = NativeVaultEngine;
    let notes = engine.scan(&detected.root)?;
    let vault_config = VaultConfig::load_from_vault(&detected.root)?;
    let cache_path = cache_path_for_vault(&detected.name)?;
    let cache = VaultCache::open_for_vault(&cache_path, &detected.root)?;
    cache.reindex_with_periodic(&detected.name, &notes, &vault_config.periodic)?;
    let search_index_path = search_index_path_for_vault(&detected.name)?;
    let search_index = SearchIndex::open(&search_index_path)?;
    search_index.reindex(&detected.name, &notes)?;

    println!(
        "Reindexed {} notes for {} into {} and {}",
        notes.len(),
        detected.name,
        cache_path.display(),
        search_index_path.display()
    );

    Ok(())
}
