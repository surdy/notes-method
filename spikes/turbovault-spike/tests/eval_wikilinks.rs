//! Test TurboVault wikilink parsing.

#[test]
fn parses_basic_wikilinks() {
    // [[Acme Corp]], [[Migration to v2]]
    todo!()
}

#[test]
fn parses_wikilink_with_heading() {
    // [[Acme Corp#Current Status]]
    todo!()
}

#[test]
fn parses_wikilink_with_block_ref() {
    // [[Widget API#^pricing-block]], [[Acme Corp#^summary-block]]
    todo!()
}

#[test]
fn parses_wikilink_with_alias() {
    // [[John Smith|John]], [[Jane Doe|Jane]]
    // EXPECTED TO FAIL based on research — alias parsing not implemented.
    todo!()
}
