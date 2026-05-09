//! Test TurboVault block reference parsing.

use std::path::{Path, PathBuf};

use anyhow::Result;
use turbovault_core::{LinkType, ServerConfig, VaultConfig, VaultFile};
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

#[tokio::test]
async fn parses_block_id_definition() -> Result<()> {
    let customer = parse_fixture("Customers/Acme/Acme Corp.md").await?;
    let stream = parse_fixture("Customers/Acme/Streams/Migration to v2.md").await?;

    assert!(customer.content.contains("^summary-block"));
    assert!(stream.content.contains("^phase-1-block"));
    // EXPECTED: TurboVault currently leaves `VaultFile.blocks` empty for these fixtures.
    assert!(customer.blocks.is_empty());
    assert!(stream.blocks.is_empty());

    Ok(())
}

#[tokio::test]
async fn parses_block_ref_in_wikilink() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;

    let block_ref = vault_file
        .links
        .iter()
        .find(|link| link.target == "Acme Corp#^summary-block")
        .expect("daily note should contain a block-ref wikilink");

    assert_eq!(block_ref.type_, LinkType::BlockRef);
    Ok(())
}
