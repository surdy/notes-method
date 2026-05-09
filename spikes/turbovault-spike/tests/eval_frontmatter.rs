//! Test TurboVault frontmatter extraction against golden vault fixtures.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;
use turbovault_core::{Frontmatter, ServerConfig, VaultConfig, VaultFile};
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

fn frontmatter(vault_file: &VaultFile) -> &Frontmatter {
    vault_file
        .frontmatter
        .as_ref()
        .expect("fixture should contain frontmatter")
}

fn frontmatter_str<'a>(frontmatter: &'a Frontmatter, key: &str) -> Option<&'a str> {
    frontmatter.data.get(key).and_then(Value::as_str)
}

#[tokio::test]
async fn parses_daily_note_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Daily/2025-01-15.md").await?;
    let frontmatter = frontmatter(&vault_file);

    assert_eq!(frontmatter_str(frontmatter, "type"), Some("daily"));
    assert_eq!(frontmatter_str(frontmatter, "date"), Some("2025-01-15"));
    assert_eq!(
        frontmatter.tags(),
        vec!["daily".to_string(), "wednesday".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn parses_meeting_note_frontmatter() -> Result<()> {
    let vault_file =
        parse_fixture("Customers/Acme/Internal Meetings/2025-01-15 Internal Sync.md").await?;
    let frontmatter = frontmatter(&vault_file);

    assert_eq!(frontmatter_str(frontmatter, "type"), Some("meeting"));
    assert_eq!(
        frontmatter_str(frontmatter, "meeting-kind"),
        Some("internal")
    );
    assert_eq!(
        frontmatter_str(frontmatter, "customer"),
        Some("[[Acme Corp]]")
    );

    Ok(())
}

#[tokio::test]
async fn parses_stream_note_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Customers/Acme/Streams/Migration to v2.md").await?;
    let frontmatter = frontmatter(&vault_file);

    assert_eq!(frontmatter_str(frontmatter, "type"), Some("stream"));
    assert_eq!(frontmatter_str(frontmatter, "status"), Some("In Progress"));
    assert_eq!(frontmatter_str(frontmatter, "priority"), Some("P1"));

    Ok(())
}

#[tokio::test]
async fn parses_customer_note_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Customers/Acme/Acme Corp.md").await?;
    let frontmatter = frontmatter(&vault_file);

    assert_eq!(frontmatter_str(frontmatter, "type"), Some("customer"));
    assert_eq!(frontmatter_str(frontmatter, "state"), Some("Active"));
    assert_eq!(
        frontmatter_str(frontmatter, "customer"),
        Some("[[Acme Corp]]")
    );

    Ok(())
}

#[tokio::test]
async fn parses_account_info_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Customers/Acme/Account Info/Account Info.md").await?;
    assert_eq!(
        frontmatter_str(frontmatter(&vault_file), "type"),
        Some("account-info")
    );
    Ok(())
}

#[tokio::test]
async fn parses_glossary_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Customers/Acme/Account Info/Glossary.md").await?;
    assert_eq!(
        frontmatter_str(frontmatter(&vault_file), "type"),
        Some("glossary")
    );
    Ok(())
}

#[tokio::test]
async fn parses_milestones_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Customers/Acme/Account Info/Dates and Milestones.md").await?;
    assert_eq!(
        frontmatter_str(frontmatter(&vault_file), "type"),
        Some("milestones")
    );
    Ok(())
}

#[tokio::test]
async fn parses_generic_note_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Inbox/Quick Note.md").await?;
    assert_eq!(
        frontmatter_str(frontmatter(&vault_file), "type"),
        Some("note")
    );
    Ok(())
}

#[tokio::test]
async fn parses_dashboard_frontmatter() -> Result<()> {
    let vault_file = parse_fixture("Dashboards/Home.md").await?;
    assert_eq!(
        frontmatter_str(frontmatter(&vault_file), "type"),
        Some("dashboard")
    );
    Ok(())
}

#[tokio::test]
async fn preserves_unknown_frontmatter_keys() -> Result<()> {
    let meeting =
        parse_fixture("Customers/Acme/Internal Meetings/2025-01-15 Internal Sync.md").await?;
    let meeting_frontmatter = frontmatter(&meeting);
    assert_eq!(
        frontmatter_str(meeting_frontmatter, "meeting-kind"),
        Some("internal")
    );
    assert_eq!(
        frontmatter_str(meeting_frontmatter, "customer"),
        Some("[[Acme Corp]]")
    );
    assert_eq!(
        frontmatter_str(meeting_frontmatter, "stream"),
        Some("[[Migration to v2]]")
    );

    let stream = parse_fixture("Customers/Acme/Streams/Migration to v2.md").await?;
    let stream_frontmatter = frontmatter(&stream);
    assert_eq!(
        frontmatter_str(stream_frontmatter, "status"),
        Some("In Progress")
    );
    assert_eq!(frontmatter_str(stream_frontmatter, "priority"), Some("P1"));
    assert_eq!(frontmatter_str(stream_frontmatter, "owner"), Some("me"));

    Ok(())
}
