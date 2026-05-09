//! Test TurboVault inline field extraction (Dataview-style).

use std::path::PathBuf;

use turbovault_parser::ParsedContent;

#[allow(dead_code)]
fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

#[test]
fn parses_inline_field_bracket_syntax() {
    let parsed = ParsedContent::parse(
        "Key points [discussed:: migration timeline], [owner:: me], [priority:: P1]",
    );

    // EXPECTED: Not detected by TurboVault.
    assert!(parsed.is_empty());
}

#[test]
fn parses_inline_field_in_body() {
    let parsed = ParsedContent::parse(
        "# Notes\n\nThis paragraph keeps [effort:: large] and [risk:: medium] as plain text.",
    );

    // EXPECTED: Not detected by TurboVault.
    assert_eq!(parsed.headings.len(), 1);
    assert!(parsed.wikilinks.is_empty());
    assert!(parsed.embeds.is_empty());
    assert!(parsed.markdown_links.is_empty());
    assert!(parsed.tags.is_empty());
    assert!(parsed.tasks.is_empty());
    assert!(parsed.callouts.is_empty());
}
