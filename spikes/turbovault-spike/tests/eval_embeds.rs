//! Test TurboVault embed syntax parsing.

use std::path::{Path, PathBuf};

use anyhow::Result;
use turbovault_core::{Link, LinkType, ServerConfig, VaultConfig, VaultFile};
use turbovault_vault::VaultManager;

#[allow(dead_code)]
fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

async fn parse_fixture(relative_path: &str) -> Result<VaultFile> {
    let vault_path = golden_vault();
    let mut config = ServerConfig::new();
    config.vaults.push(
        VaultConfig::builder("test", vault_path)
            .as_default()
            .build()?,
    );

    let manager = VaultManager::new(config)?;
    manager.initialize().await?;
    Ok(manager.parse_file(Path::new(relative_path)).await?)
}

fn find_embed<'a>(vault_file: &'a VaultFile, target: &str) -> &'a Link {
    vault_file
        .links
        .iter()
        .find(|link| link.type_ == LinkType::Embed && link.target == target)
        .unwrap_or_else(|| panic!("missing embed target: {target}"))
}

#[tokio::test]
async fn parses_image_embed() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;
    let embed = find_embed(&vault_file, "meeting-screenshot.png");

    assert_eq!(embed.type_, LinkType::Embed);
    assert_eq!(embed.display_text, None);

    Ok(())
}

#[tokio::test]
async fn parses_note_section_embed() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;
    let embed = find_embed(&vault_file, "Migration to v2#Phase 1");

    assert_eq!(embed.type_, LinkType::Embed);
    assert_eq!(embed.display_text, None);

    Ok(())
}
