//! Test TurboVault callout parsing.

#[test]
fn parses_callout_type() {
    // > [!tip], > [!note], > [!warning], etc.
    todo!()
}

#[test]
fn parses_callout_title() {
    // > [!tip] Key Insight
    todo!()
}

#[test]
fn parses_callout_multiline_content() {
    // Multiple lines after the callout header
    // EXPECTED TO FAIL — only first line parsed per research.
    todo!()
}

#[test]
fn parses_callout_foldability() {
    // > [!note]+ (expanded) and > [!note]- (collapsed)
    todo!()
}
