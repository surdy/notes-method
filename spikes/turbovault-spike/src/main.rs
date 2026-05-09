use std::path::{Path, PathBuf};

use anyhow::Result;
use turbovault_core::{ServerConfig, VaultConfig};
use turbovault_vault::VaultManager;

#[tokio::main]
async fn main() -> Result<()> {
    let vault_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../golden-vault"));

    let vault_path = vault_path.canonicalize().unwrap_or(vault_path);

    println!("=== TurboVault Evaluation Spike ===");
    println!("Vault path: {}", vault_path.display());

    let mut config = ServerConfig::new();
    let vault_config = VaultConfig::builder("evaluation", vault_path.clone())
        .as_default()
        .build()?;
    config.vaults.push(vault_config);

    let manager = VaultManager::new(config)?;
    manager.initialize().await?;

    let files = manager.scan_vault().await?;
    println!("\nFiles found: {}", files.len());

    let mut parsed_notes = 0usize;
    let mut total_links = 0usize;
    let mut total_tasks = 0usize;
    let mut total_tags = 0usize;
    let mut total_callouts = 0usize;
    let mut parse_errors = Vec::new();

    for file_path in &files {
        match manager.parse_file(file_path).await {
            Ok(vault_file) => {
                parsed_notes += 1;
                total_links += vault_file.links.len();
                total_tasks += vault_file.tasks.len();
                total_tags += vault_file.tags.len();
                total_callouts += vault_file.callouts.len();

                let display_path = display_path(file_path, &vault_path);
                println!("\n--- {} ---", display_path.display());
                println!(
                    "  Links: {}, Tasks: {}, Tags: {}, Callouts: {}",
                    vault_file.links.len(),
                    vault_file.tasks.len(),
                    vault_file.tags.len(),
                    vault_file.callouts.len(),
                );

                if let Some(frontmatter) = &vault_file.frontmatter {
                    let mut keys = frontmatter.data.keys().cloned().collect::<Vec<_>>();
                    keys.sort();
                    println!("  Frontmatter keys: {:?}", keys);
                }
            }
            Err(error) => {
                parse_errors.push((file_path.clone(), error.to_string()));
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total files: {}", files.len());
    println!("Notes parsed: {}", parsed_notes);
    println!("Total links: {}", total_links);
    println!("Total tasks: {}", total_tasks);
    println!("Total tags: {}", total_tags);
    println!("Total callouts: {}", total_callouts);

    if !parse_errors.is_empty() {
        println!("\nParse errors:");
        for (path, error) in &parse_errors {
            let display_path = display_path(path, &vault_path);
            println!("  {} — {}", display_path.display(), error);
        }
    }

    Ok(())
}

fn display_path<'a>(path: &'a Path, vault_root: &'a Path) -> &'a Path {
    path.strip_prefix(vault_root).unwrap_or(path)
}
