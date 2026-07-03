//! Integration test for the embedding observability endpoint (#244).
//!
//! Exercises the real Axum router end to end: a vault is embedded to its
//! canonical `embeddings.db` (under an isolated `XDG_DATA_HOME`), and
//! `GET /api/v/{vault}/embeddings/stats` is asserted for shape and values.
//! A second, never-embedded vault must report an empty-but-valid index rather
//! than erroring.

use std::fs;

use notesmith_embed::HashEmbedder;
use notesmith_http::{AppState, create_vault_state, embed_scheduler, serve_with_listener};
use tempfile::TempDir;

fn vault_with_note(root: &std::path::Path, name: &str, body: &str) {
    fs::create_dir_all(root.join(".notesmith")).unwrap();
    fs::write(
        root.join(".notesmith/vault.toml"),
        format!("name = \"{name}\"\n"),
    )
    .unwrap();
    fs::write(root.join("note.md"), body).unwrap();
}

#[tokio::test]
async fn embedding_stats_reports_vectors_and_empty_index() {
    let temp = TempDir::new().unwrap();
    let data_home = temp.path().join("data");
    let config_home = temp.path().join("config");
    unsafe {
        std::env::set_var("XDG_DATA_HOME", &data_home);
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
    }

    // Vault "embedded" gets a real embeddings.db written to its canonical path.
    let embedded_root = temp.path().join("embedded");
    vault_with_note(
        &embedded_root,
        "embedded",
        "# Note\n\nsome searchable content",
    );
    let db_path = notesmith_embed::embeddings_db_path("embedded").unwrap();
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let embedder = HashEmbedder::new(64);
    let report =
        embed_scheduler::run_embed_pass("embedded", &embedded_root, &db_path, &embedder).unwrap();
    assert_eq!(report.embedded, 1);

    // Vault "empty" is never embedded; its embeddings.db does not exist.
    let empty_root = temp.path().join("empty");
    vault_with_note(&empty_root, "empty", "# Empty\n\nnot embedded");

    let state = AppState {
        vaults: [
            (
                "embedded".to_string(),
                create_vault_state("embedded", &embedded_root).unwrap(),
            ),
            (
                "empty".to_string(),
                create_vault_state("empty", &empty_root).unwrap(),
            ),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    // Embedded vault: non-zero vectors, known dim/embedder, on-disk bytes.
    let body: serde_json::Value =
        reqwest::get(format!("http://{address}/api/v/embedded/embeddings/stats"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert!(body["vector_count"].as_i64().unwrap() >= 1);
    assert!(body["db_bytes"].as_u64().unwrap() > 0);
    assert_eq!(body["dim"].as_u64().unwrap(), 64);
    assert!(body["embedder_id"].as_str().unwrap().contains("hash"));
    assert!(body["last_ingest_at"].as_u64().is_some());
    assert!(body["p50_ms"].is_number());
    assert!(body["p95_ms"].is_number());

    // Empty vault: valid response, zero vectors, no dim/embedder.
    let empty: serde_json::Value =
        reqwest::get(format!("http://{address}/api/v/empty/embeddings/stats"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(empty["vector_count"].as_i64().unwrap(), 0);
    assert_eq!(empty["db_bytes"].as_u64().unwrap(), 0);
    assert!(empty["dim"].is_null());
    assert!(empty["embedder_id"].is_null());

    // Unknown vault: 404, not a 500.
    let missing = reqwest::get(format!("http://{address}/api/v/nope/embeddings/stats"))
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    server.abort();
}
