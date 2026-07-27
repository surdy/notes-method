//! The shipped kit and the `golden-vault` fixture must not drift apart.
//!
//! This is what makes the fixture's whole test suite meaningful for users:
//! `golden-vault` proves the schema works (routing destinations, template
//! rendering, periodic matching, executable dashboard SQL), and these
//! assertions prove the bytes `notesmith kit apply` writes are those same
//! bytes. Change one side without the other and this fails.

use notesmith_kit::Kit;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn kit_file(relative: &str) -> &'static str {
    Kit::builtin("work-notes")
        .unwrap()
        .files()
        .iter()
        .find(|(path, _)| *path == relative)
        .unwrap_or_else(|| panic!("kit is missing {relative}"))
        .1
}

fn fixture_file(relative: &str) -> String {
    let path = golden_vault().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn config_and_templates_are_byte_identical_to_the_fixture() {
    let mut compared = 0;

    for (relative, contents) in Kit::builtin("work-notes").unwrap().files() {
        // vault.toml carries a name placeholder; dashboards are deliberately
        // generic (the fixture's carry Acme-specific sections).
        if *relative == ".notesmith/vault.toml" || relative.starts_with("Dashboards/") {
            continue;
        }
        compared += 1;
        assert_eq!(
            *contents,
            fixture_file(relative),
            "{relative} has drifted between kits/work-notes and golden-vault"
        );
    }

    assert!(
        compared >= 12,
        "expected to compare the config and nine templates, compared {compared}"
    );
}

#[test]
fn vault_toml_matches_the_fixture_apart_from_the_vault_name() {
    let rendered = kit_file(".notesmith/vault.toml").replace("{{ vault_name }}", "golden-vault");

    assert_eq!(
        rendered,
        fixture_file(".notesmith/vault.toml"),
        "vault.toml has drifted (beyond the substituted vault name)"
    );
}

#[test]
fn the_kit_ships_every_template_the_fixture_has() {
    let kit_templates: Vec<&str> = Kit::builtin("work-notes")
        .unwrap()
        .files()
        .iter()
        .filter_map(|(path, _)| path.strip_prefix(".notesmith/templates/"))
        .collect();

    let mut fixture_templates: Vec<String> =
        std::fs::read_dir(golden_vault().join(".notesmith/templates"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
    fixture_templates.sort();

    let mut kit_templates: Vec<String> = kit_templates.iter().map(|s| s.to_string()).collect();
    kit_templates.sort();

    assert_eq!(kit_templates, fixture_templates);
}
