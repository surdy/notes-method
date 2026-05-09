use std::{collections::HashMap, path::PathBuf};

use notesmith_core::VaultEngine;
use notesmith_http::{AppState, VaultState, serve_with_listener};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;

fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn build_test_state() -> AppState {
    let root = golden_vault();
    let engine = NativeVaultEngine;
    let notes = engine.scan(&root).unwrap();
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("test-vault", &notes).unwrap();
    let search_index = SearchIndex::open_in_memory().unwrap();
    search_index.reindex("test-vault", &notes).unwrap();

    AppState {
        vaults: HashMap::from([(
            "test-vault".to_string(),
            VaultState {
                cache,
                search_index,
                engine,
                root,
            },
        )]),
    }
}

#[tokio::test]
async fn get_ping_returns_ok_status() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, AppState::default())
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/ping"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({ "status": "ok" })
    );

    server.abort();
}

#[tokio::test]
async fn list_notes_returns_cached_notes_for_vault() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state())
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/notes"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert!(body.len() >= 20);
    assert!(body.iter().any(|note| {
        note.get("path")
            == Some(&serde_json::Value::String(
                "Customers/Acme/Acme Corp.md".into(),
            ))
    }));

    server.abort();
}

#[tokio::test]
async fn execute_sql_returns_query_result_json() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state())
            .await
            .unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/v/test-vault/query/sql"))
        .json(&serde_json::json!({
            "sql": "SELECT title FROM v_notes ORDER BY title LIMIT 3"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["columns"], serde_json::json!(["title"]));
    assert_eq!(body["row_count"], serde_json::json!(3));

    server.abort();
}

#[tokio::test]
async fn get_note_returns_full_note_metadata() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state())
            .await
            .unwrap();
    });

    let response = reqwest::get(format!(
        "http://{address}/api/v/test-vault/notes/Customers/Acme/Streams/Migration%20to%20v2.md"
    ))
    .await
    .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(
        body["path"],
        serde_json::json!("Customers/Acme/Streams/Migration to v2.md")
    );
    assert!(
        body["body"]
            .as_str()
            .unwrap()
            .contains("API endpoint migration")
    );
    assert!(body["tasks"].as_array().unwrap().len() >= 5);
    assert!(body["links"].as_array().unwrap().len() >= 3);
    assert!(body["inline_fields"].as_array().unwrap().len() >= 2);

    server.abort();
}

#[tokio::test]
async fn search_returns_matching_notes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state())
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/search?q=Acme"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert!(body.iter().any(|note| {
        note.get("path")
            == Some(&serde_json::Value::String(
                "Customers/Acme/Acme Corp.md".into(),
            ))
    }));

    server.abort();
}
