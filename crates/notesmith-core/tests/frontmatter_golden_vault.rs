//! Test that all golden vault frontmatter deserializes correctly into typed Frontmatter.

use notesmith_core::Frontmatter;
use std::fs;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn parse_frontmatter(content: &str) -> Option<Frontmatter> {
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("---")?;
    let yaml = &rest[..end];
    serde_yaml::from_str(yaml).ok()
}

fn read_and_parse(relative_path: &str) -> Frontmatter {
    let path = golden_vault().join(relative_path);
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    parse_frontmatter(&content)
        .unwrap_or_else(|| panic!("Failed to parse frontmatter from {relative_path}"))
}

#[test]
fn daily_note() {
    let fm = read_and_parse("Inbox/Daily/2025-01-15.md");
    assert!(matches!(fm, Frontmatter::Daily(_)));
}

#[test]
fn internal_meeting() {
    let fm = read_and_parse("Customers/Acme/Internal Meetings/2025-01-15 Internal Sync.md");
    assert!(matches!(fm, Frontmatter::Meeting(_)));
}

#[test]
fn external_meeting() {
    let fm = read_and_parse("Customers/Acme/External Meetings/2025-01-14 Customer Check-in.md");
    assert!(matches!(fm, Frontmatter::Meeting(_)));
}

#[test]
fn stream_note() {
    let fm = read_and_parse("Customers/Acme/Streams/Migration to v2.md");
    assert!(matches!(fm, Frontmatter::Stream(_)));
}

#[test]
fn customer_note() {
    let fm = read_and_parse("Customers/Acme/Acme Corp.md");
    assert!(matches!(fm, Frontmatter::Customer(_)));
}

#[test]
fn account_info_note() {
    let fm = read_and_parse("Customers/Acme/Account Info/Account Info.md");
    assert!(matches!(fm, Frontmatter::AccountInfo(_)));
}

#[test]
fn glossary_note() {
    let fm = read_and_parse("Customers/Acme/Account Info/Glossary.md");
    assert!(matches!(fm, Frontmatter::Glossary(_)));
}

#[test]
fn milestones_note() {
    let fm = read_and_parse("Customers/Acme/Account Info/Dates and Milestones.md");
    assert!(matches!(fm, Frontmatter::Milestones(_)));
}

#[test]
fn dashboard_note() {
    let fm = read_and_parse("Dashboards/Home.md");
    assert!(matches!(fm, Frontmatter::Dashboard(_)));
}

#[test]
fn generic_note() {
    let fm = read_and_parse("Inbox/Quick Note.md");
    assert!(matches!(fm, Frontmatter::Note(_)));
}

#[test]
fn contact_note() {
    let fm = read_and_parse("Customers/Acme/Contacts/John Smith.md");
    assert!(matches!(fm, Frontmatter::Contact(_)));
}

#[test]
fn globex_customer() {
    let fm = read_and_parse("Customers/Globex/Globex.md");
    assert!(matches!(fm, Frontmatter::Customer(_)));
}

#[test]
fn globex_stream() {
    let fm = read_and_parse("Customers/Globex/Streams/Platform Rollout.md");
    assert!(matches!(fm, Frontmatter::Stream(_)));
}
