//! Integration test for the Relevant Notes endpoint (issue #201).
//!
//! Exercises the real Axum router end to end: a small linked vault is indexed
//! and `GET /api/v/{vault}/related/{path}` is asserted for shape and ranking.
//! With no `embeddings.db` present the endpoint must degrade to graph-only
//! ranking (`embeddings_used: false`) rather than erroring.

use std::fs;

use notesmith_http::{AppState, create_vault_state, serve_with_listener};
use tempfile::TempDir;

fn vault_note(root: &std::path::Path, name: &str, rel: &str, body: &str) {
    fs::create_dir_all(root.join(".notesmith")).unwrap();
    fs::write(
        root.join(".notesmith/vault.toml"),
        format!("name = \"{name}\"\n"),
    )
    .unwrap();
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

#[tokio::test]
async fn related_endpoint_ranks_linked_notes() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("graph");
    vault_note(
        &root,
        "graph",
        "Hub.md",
        "---\ntype: note\n---\nSee [[Spoke A]] and [[Spoke B]].",
    );
    vault_note(
        &root,
        "graph",
        "Spoke A.md",
        "---\ntype: note\n---\n[[Hub]]",
    );
    vault_note(&root, "graph", "Spoke B.md", "---\ntype: note\n---\nLeaf");
    vault_note(
        &root,
        "graph",
        "Cousin.md",
        "---\ntype: note\n---\nAlso [[Spoke A]]",
    );

    let state = AppState {
        vaults: [(
            "graph".to_string(),
            create_vault_state("graph", &root).unwrap(),
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let body: serde_json::Value = reqwest::get(format!(
        "http://{address}/api/v/graph/related/Hub.md?limit=5"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

    assert_eq!(body["path"], "Hub.md");
    assert_eq!(body["embeddings_used"], false);
    let related = body["related"].as_array().unwrap();
    let paths: Vec<&str> = related
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["Spoke A.md", "Spoke B.md", "Cousin.md"]);
    // Every result carries the scoring signals the panel renders.
    for entry in related {
        assert!(entry["score"].is_number());
        assert!(entry["directly_linked"].is_boolean());
        assert!(entry["shared_neighbors"].is_number());
    }

    // Unknown note: 404, not a 500.
    let missing = reqwest::get(format!("http://{address}/api/v/graph/related/Nope.md"))
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    // Unknown vault: 404.
    let no_vault = reqwest::get(format!("http://{address}/api/v/nope/related/Hub.md"))
        .await
        .unwrap();
    assert_eq!(no_vault.status(), reqwest::StatusCode::NOT_FOUND);

    server.abort();
}
