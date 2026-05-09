//! Test TurboVault callout parsing.

use std::{fs, path::PathBuf};

use anyhow::Result;
use turbovault_core::CalloutType;
use turbovault_parser::{ParseOptions, ParsedContent};

#[allow(dead_code)]
fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn daily_note() -> Result<String> {
    Ok(fs::read_to_string(
        golden_vault().join("Inbox/Daily/2025-01-15.md"),
    )?)
}

#[test]
fn parses_callout_type() -> Result<()> {
    let parsed = ParsedContent::parse(&daily_note()?);
    let callout = parsed
        .callouts
        .first()
        .expect("daily note should contain a callout");

    assert_eq!(callout.type_, CalloutType::Tip);
    Ok(())
}

#[test]
fn parses_callout_title() -> Result<()> {
    let parsed = ParsedContent::parse(&daily_note()?);
    let callout = parsed
        .callouts
        .first()
        .expect("daily note should contain a callout");

    assert_eq!(callout.title.as_deref(), Some("Key Insight"));
    Ok(())
}

#[test]
fn parses_callout_multiline_content() -> Result<()> {
    let content = daily_note()?;
    let parsed_default = ParsedContent::parse(&content);
    let parsed_full =
        ParsedContent::parse_with_options(&content, ParseOptions::all().with_full_callouts());

    // EXPECTED: Default callout parsing leaves multi-line content empty.
    assert_eq!(parsed_default.callouts[0].content, "");
    assert_eq!(
        parsed_full.callouts[0].content,
        "The migration timeline is ahead of schedule.\nWe should start planning Phase 2.\nThis is a multi-line callout to test parsing."
    );

    Ok(())
}

#[test]
fn parses_callout_foldability() {
    let content = "> [!note]+ Expanded\n> Line 1\n\n> [!note]- Collapsed\n> Line 2";
    let parsed = ParsedContent::parse(content);

    assert_eq!(parsed.callouts.len(), 2);
    assert!(parsed.callouts[0].is_foldable);
    assert!(parsed.callouts[1].is_foldable);
    assert_eq!(parsed.callouts[0].title.as_deref(), Some("Expanded"));
    assert_eq!(parsed.callouts[1].title.as_deref(), Some("Collapsed"));
}
