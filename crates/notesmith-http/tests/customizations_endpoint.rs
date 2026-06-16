//! Integration test for the customization-discovery endpoint (issue #210).
//!
//! Exercises the real Axum router end to end: the vault's
//! `.notesmith/{agents,skills,instructions}/` folders and the global config dir
//! both supply files, and the `GET /api/v/{vault}/customizations` response is
//! asserted for shape and merge precedence (project overrides global by id).

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
async fn customizations_endpoint_merges_project_and_global() {
    let temp = TempDir::new().unwrap();

    // Isolate the global config dir so discovery does not touch the real one.
    let config_home = temp.path().join("config");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
    }
    // Global scope: one agent that the project will override, one global-only skill.
    write(
        &config_home.join("notesmith/agents/researcher.md"),
        "---\nname: Global Researcher\nbackend: claude\n---\nGlobal persona.",
    );
    write(
        &config_home.join("notesmith/skills/citations.md"),
        "---\nname: Citations\ndescription: cite sources\n---\nAlways cite sources.",
    );

    // Project scope: overrides `researcher`, adds an instruction.
    let vault_root = temp.path().join("vault");
    fs::create_dir_all(vault_root.join(".notesmith")).unwrap();
    fs::write(
        vault_root.join(".notesmith/vault.toml"),
        "name = \"test-vault\"\n",
    )
    .unwrap();
    write(
        &vault_root.join(".notesmith/agents/researcher.md"),
        "---\nname: Project Researcher\nbackend: copilot\nmodel: gpt-4o\n---\nProject persona.",
    );
    write(
        &vault_root.join(".notesmith/instructions/tone.md"),
        "---\nname: Tone\n---\nBe concise.",
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

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/customizations"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();

    let agents = body["agents"].as_array().expect("agents array");
    assert_eq!(
        agents.len(),
        1,
        "project agent overrides the global one by id"
    );
    assert_eq!(agents[0]["id"], "researcher");
    assert_eq!(agents[0]["name"], "Project Researcher");
    assert_eq!(agents[0]["backend"], "copilot");
    assert_eq!(agents[0]["model"], "gpt-4o");
    assert_eq!(agents[0]["source"], "project");

    let skills = body["skills"].as_array().expect("skills array");
    assert_eq!(skills.len(), 1, "global-only skill is surfaced");
    assert_eq!(skills[0]["id"], "citations");
    assert_eq!(skills[0]["source"], "global");

    let instructions = body["instructions"].as_array().expect("instructions array");
    assert_eq!(instructions.len(), 1);
    assert_eq!(instructions[0]["id"], "tone");
    assert_eq!(instructions[0]["source"], "project");

    server.abort();
}
