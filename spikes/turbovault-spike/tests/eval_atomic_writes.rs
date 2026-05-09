//! Test TurboVault atomic write correctness.

#[test]
fn write_read_roundtrip_preserves_content() {
    // Write a note with complex OFM syntax, read it back, assert identical.
    // Use a temp directory, not the golden vault.
    todo!()
}

#[test]
fn write_preserves_frontmatter() {
    // Write note with YAML frontmatter, read back, check frontmatter intact.
    todo!()
}

#[test]
fn write_preserves_wikilinks_and_tasks() {
    // Write note with wikilinks and tasks, read back, check they parse the same.
    todo!()
}
