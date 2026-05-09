//! Test TurboVault inline field extraction (Dataview-style).

#[test]
fn parses_inline_field_bracket_syntax() {
    // [discussed:: migration timeline], [owner:: me], [priority:: P1]
    // EXPECTED TO FAIL — inline fields not supported by TurboVault.
    todo!()
}

#[test]
fn parses_inline_field_in_body() {
    // [effort:: large], [risk:: medium]
    // EXPECTED TO FAIL.
    todo!()
}
