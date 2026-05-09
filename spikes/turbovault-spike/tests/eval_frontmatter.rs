//! Test TurboVault frontmatter extraction against golden vault fixtures.

use std::path::PathBuf;

#[allow(dead_code)]
fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

#[test]
fn parses_daily_note_frontmatter() {
    // Parse golden-vault/Inbox/Daily/2025-01-15.md
    // Assert: type == "daily", date == "2025-01-15", tags contains "daily" and "wednesday"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_meeting_note_frontmatter() {
    // Parse golden-vault/Customers/Acme/Internal Meetings/2025-01-15 Internal Sync.md
    // Assert: type == "meeting", meeting-kind == "internal", customer == "[[Acme Corp]]"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_stream_note_frontmatter() {
    // Parse golden-vault/Customers/Acme/Streams/Migration to v2.md
    // Assert: type == "stream", status == "In Progress", priority == "P1"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_customer_note_frontmatter() {
    // Parse golden-vault/Customers/Acme/Acme Corp.md
    // Assert: type == "customer", state == "Active", customer == "[[Acme Corp]]"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_account_info_frontmatter() {
    // Assert: type == "account-info"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_glossary_frontmatter() {
    // Assert: type == "glossary"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_milestones_frontmatter() {
    // Assert: type == "milestones"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_generic_note_frontmatter() {
    // Assert: type == "note"
    todo!("Implement once API is confirmed")
}

#[test]
fn parses_dashboard_frontmatter() {
    // Assert: type == "dashboard"
    todo!("Implement once API is confirmed")
}

#[test]
fn preserves_unknown_frontmatter_keys() {
    // Our notes have custom keys like meeting-kind, customer (as wikilink), etc.
    // Assert they're preserved in the frontmatter map.
    todo!("Implement once API is confirmed")
}
