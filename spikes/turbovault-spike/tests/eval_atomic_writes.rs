//! Test TurboVault atomic write correctness.

use std::path::{Path, PathBuf};

use anyhow::Result;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::{TempDir, tempdir_in};
use turbovault_core::{LinkType, ServerConfig, TaskPriority, VaultConfig};
use turbovault_vault::VaultManager;

#[allow(dead_code)]
fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

async fn temp_manager() -> Result<(TempDir, VaultManager)> {
    let temp_dir = tempdir_in(env!("CARGO_MANIFEST_DIR"))?;
    let mut config = ServerConfig::new();
    config.vaults.push(
        VaultConfig::builder("test", temp_dir.path())
            .as_default()
            .build()?,
    );

    let manager = VaultManager::new(config)?;
    manager.initialize().await?;

    Ok((temp_dir, manager))
}

#[tokio::test]
async fn write_read_roundtrip_preserves_content() -> Result<()> {
    let (_temp_dir, manager) = temp_manager().await?;
    let path = Path::new("Roundtrip.md");
    let content = "---\ntype: note\ntags:\n  - roundtrip\n---\n\n# Roundtrip\n\nLink [[Acme Corp]] and alias [[John Smith|John]].\n\n- [ ] Review release notes 📅 2025-01-20 🔼\n- [x] Send summary ✅ 2025-01-15\n\n> [!tip] Keep this intact\n> Round-trip should preserve every byte.\n";

    manager.write_file(path, content, None).await?;
    let read_back = manager.read_file(path).await?;

    assert_eq!(read_back, content);
    Ok(())
}

#[tokio::test]
async fn write_preserves_frontmatter() -> Result<()> {
    let (_temp_dir, manager) = temp_manager().await?;
    let path = Path::new("Frontmatter.md");
    let content = "---\ntype: meeting\nmeeting-kind: internal\ncustomer: \"[[Acme Corp]]\"\nstream: \"[[Migration to v2]]\"\n---\n\n# Frontmatter\n";

    manager.write_file(path, content, None).await?;
    let vault_file = manager.parse_file(path).await?;
    let frontmatter = vault_file
        .frontmatter
        .expect("written file should parse frontmatter");

    assert_eq!(
        frontmatter.data.get("type").and_then(Value::as_str),
        Some("meeting")
    );
    assert_eq!(
        frontmatter.data.get("meeting-kind").and_then(Value::as_str),
        Some("internal")
    );
    assert_eq!(
        frontmatter.data.get("customer").and_then(Value::as_str),
        Some("[[Acme Corp]]")
    );
    assert_eq!(
        frontmatter.data.get("stream").and_then(Value::as_str),
        Some("[[Migration to v2]]")
    );

    Ok(())
}

#[tokio::test]
async fn write_preserves_wikilinks_and_tasks() -> Result<()> {
    let (_temp_dir, manager) = temp_manager().await?;
    let path = Path::new("Links and Tasks.md");
    let content = "# Links and Tasks\n\nSee [[Acme Corp#Current Status]] and [[John Smith|John]].\n\n- [ ] Follow up with customer 📅 2025-01-20 🔼\n- [x] Send recap ✅ 2025-01-15\n";

    manager.write_file(path, content, None).await?;
    let vault_file = manager.parse_file(path).await?;

    let heading_ref = vault_file
        .links
        .iter()
        .find(|link| link.target == "Acme Corp#Current Status")
        .expect("heading ref should parse after write");
    assert_eq!(heading_ref.type_, LinkType::HeadingRef);

    let alias = vault_file
        .links
        .iter()
        .find(|link| link.target == "John Smith")
        .expect("aliased wikilink should parse after write");
    assert_eq!(alias.display_text.as_deref(), Some("John"));

    assert_eq!(vault_file.tasks.len(), 2);
    assert_eq!(vault_file.tasks[0].content, "Follow up with customer");
    assert_eq!(
        vault_file.tasks[0].due_date.map(|date| date.to_string()),
        Some("2025-01-20".to_string())
    );
    assert_eq!(vault_file.tasks[0].priority, TaskPriority::Medium);
    assert_eq!(vault_file.tasks[1].content, "Send recap");
    assert_eq!(
        vault_file.tasks[1].done_date.map(|date| date.to_string()),
        Some("2025-01-15".to_string())
    );

    Ok(())
}
