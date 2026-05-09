//! Test TurboVault wikilink parsing.

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

fn find_link<'a>(vault_file: &'a VaultFile, target: &str) -> &'a Link {
    vault_file
        .links
        .iter()
        .find(|link| link.target == target)
        .unwrap_or_else(|| panic!("missing link target: {target}"))
}

#[tokio::test]
async fn parses_basic_wikilinks() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;

    let customer = find_link(&vault_file, "Acme Corp");
    assert_eq!(customer.type_, LinkType::WikiLink);
    assert_eq!(customer.display_text, None);

    let stream = find_link(&vault_file, "Migration to v2");
    assert_eq!(stream.type_, LinkType::WikiLink);
    assert_eq!(stream.display_text, None);

    Ok(())
}

#[tokio::test]
async fn parses_wikilink_with_heading() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;
    let heading_ref = find_link(&vault_file, "Acme Corp#Current Status");

    assert_eq!(heading_ref.type_, LinkType::HeadingRef);
    assert_eq!(heading_ref.display_text, None);

    Ok(())
}

#[tokio::test]
async fn parses_wikilink_with_block_ref() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;

    let widget_block = find_link(&vault_file, "Widget API#^pricing-block");
    assert_eq!(widget_block.type_, LinkType::BlockRef);

    let summary_block = find_link(&vault_file, "Acme Corp#^summary-block");
    assert_eq!(summary_block.type_, LinkType::BlockRef);

    Ok(())
}

#[tokio::test]
async fn parses_wikilink_with_alias() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;
    let alias = find_link(&vault_file, "John Smith");

    assert_eq!(alias.type_, LinkType::WikiLink);
    assert_eq!(alias.display_text.as_deref(), Some("John"));

    Ok(())
}
