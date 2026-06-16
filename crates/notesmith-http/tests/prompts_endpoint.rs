//! Integration test for the static custom-prompts endpoint (issue #193).
//!
//! Exercises the real Axum router end to end: config-dir defaults are seeded,
//! the vault `_prompts/` folder supplies an override plus a new prompt, and the
//! `GET /api/v/{vault}/prompts` response is asserted for shape and merge
//! precedence (vault overrides a default by `name`).

use std::{fs, path::Path};

use notesmith_http::{AppState, create_vault_state, serve_with_listener};
use tempfile::TempDir;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[tokio::test]
async fn prompts_endpoint_returns_merged_defaults_and_vault_overrides() {
    let temp = TempDir::new().unwrap();

    // Isolate the daemon config dir so seeding does not touch the real one.
    let config_home = temp.path().join("config");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
    }
    let defaults_dir = notesmith_prompts::default_prompts_dir().unwrap();
    let seeded = notesmith_prompts::seed_default_prompts(&defaults_dir).unwrap();
    assert_eq!(seeded, notesmith_prompts::DEFAULT_PROMPTS.len());

    // A vault with one override (`summarize`) and one brand-new prompt.
    let vault_root = temp.path().join("vault");
    fs::create_dir_all(vault_root.join(".notesmith")).unwrap();
    fs::write(
        vault_root.join(".notesmith/vault.toml"),
        "name = \"test-vault\"\n",
    )
    .unwrap();
    write(
        &vault_root.join("_prompts/summarize.md"),
        "---\nname: summarize\ndescription: vault override\n---\nVault-specific summary instruction.",
    );
    write(
        &vault_root.join("_prompts/standup.md"),
        "---\nname: standup\ndescription: daily standup\n---\nDraft my standup update.",
    );

    let state = AppState {
        vaults: std::iter::once((
            "test-vault".to_string(),
            create_vault_state("test-vault", &vault_root).unwrap(),
        ))
        .collect(),
        ..Default::default()
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/prompts"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    let prompts = body
        .get("prompts")
        .and_then(|p| p.as_array())
        .expect("response should have a `prompts` array");

    // All defaults plus the one vault-only prompt.
    assert_eq!(prompts.len(), notesmith_prompts::DEFAULT_PROMPTS.len() + 1);

    let find = |name: &str| {
        prompts
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
            .unwrap_or_else(|| panic!("missing prompt {name}"))
    };

    // Vault override wins on the `summarize` name collision.
    let summarize = find("summarize");
    assert_eq!(summarize["source"], "vault");
    assert_eq!(summarize["description"], "vault override");
    assert_eq!(summarize["body"], "Vault-specific summary instruction.");

    // A non-overridden default keeps source = "default".
    let fix = find("fix");
    assert_eq!(fix["source"], "default");
    assert!(fix["body"].as_str().unwrap().contains("spelling"));

    // The vault-only prompt is present with source = "vault".
    let standup = find("standup");
    assert_eq!(standup["source"], "vault");

    // Every entry exposes the full contract shape.
    for prompt in prompts {
        assert!(prompt.get("name").and_then(|v| v.as_str()).is_some());
        assert!(prompt.get("description").is_some());
        assert!(prompt.get("body").and_then(|v| v.as_str()).is_some());
        let source = prompt.get("source").and_then(|v| v.as_str()).unwrap();
        assert!(source == "default" || source == "vault");
    }

    server.abort();
}
