//! HTTP contract tests for the per-vault transcript endpoints
//! (`/api/v/{vault}/agent/threads...`). These assert the exact JSON response
//! shapes the desktop chat UI depends on (column/field names), per the repo's
//! frontend–backend contract-testing rule.

use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicUsize},
};

use chrono::Utc;
use notesmith_config::{VaultConfig, migration};
use notesmith_core::VaultEngine;
use notesmith_http::watcher::WatcherState;
use notesmith_http::{AppState, VaultState, serve_with_listener};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use serde_json::{Value, json};
use tempfile::TempDir;

fn build_vault_state(vault_name: &str, root: &Path) -> VaultState {
    let engine = NativeVaultEngine;
    let notes = engine.scan(root).unwrap();
    let vault_config = migration::load_and_migrate(root).unwrap_or_else(|_| VaultConfig {
        name: vault_name.to_string(),
        ..Default::default()
    });
    let cache = VaultCache::open_in_memory().unwrap();
    cache
        .reindex_with_periodic(vault_name, &notes, &vault_config.periodic)
        .unwrap();
    let search_index = SearchIndex::open_in_memory().unwrap();
    search_index.reindex(vault_name, &notes).unwrap();
    let template_engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), None);

    VaultState {
        cache: Arc::new(cache),
        search_index: Arc::new(search_index),
        engine,
        root: root.to_path_buf(),
        vault_config: arc_swap::ArcSwap::from_pointee(vault_config),
        watcher_state: WatcherState::new(),
        rebuilding: std::sync::atomic::AtomicBool::new(false),
        template_engine: Arc::new(template_engine),
        preview_signing_key: notesmith_ops::LocalOps::new_preview_signing_key(),
    }
}

fn build_state(vaults: &[(String, PathBuf)], config_path: PathBuf) -> AppState {
    let vaults = vaults
        .iter()
        .map(|(name, root)| (name.clone(), build_vault_state(name, root)))
        .collect();
    let (event_tx, _) = notesmith_http::create_event_channel();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    AppState {
        vaults,
        event_tx,
        event_buffer: Arc::new(notesmith_http::EventBuffer::new(
            notesmith_http::events::EVENT_BUFFER_CAPACITY,
        )),
        global_config_path: config_path,
        started_at: Utc::now(),
        sse_connection_count: Arc::new(AtomicUsize::new(0)),
        shutdown_tx,
        shutdown_rx,
        mcp_services: Default::default(),
        transcripts: Default::default(),
        permissions: Default::default(),
        vault_watchers: Default::default(),
    }
}

struct TestServer {
    _temp_dir: TempDir,
    address: SocketAddr,
}

impl TestServer {
    async fn start(vault_names: &[&str]) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let mut vaults = Vec::new();
        for name in vault_names {
            let root = temp_dir.path().join(name);
            fs::create_dir_all(root.join(".notesmith")).unwrap();
            fs::write(
                root.join(".notesmith/vault.toml"),
                format!("name = \"{name}\"\n"),
            )
            .unwrap();
            vaults.push((name.to_string(), root));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let state = build_state(&vaults, config_path);
        tokio::spawn(async move {
            serve_with_listener(listener, state).await.unwrap();
        });
        Self {
            _temp_dir: temp_dir,
            address,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }
}

#[tokio::test]
async fn thread_and_message_lifecycle_round_trips() {
    let server = TestServer::start(&["vaultA"]).await;
    let client = reqwest::Client::new();
    let base = "/api/v/vaultA/agent/threads";

    // Create a thread.
    let resp = client
        .post(server.url(base))
        .json(&json!({ "title": "Weekly plan", "agent": "copilot", "model": "gpt-5" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    let thread: Value = resp.json().await.unwrap();
    // Exact response shape the UI consumes.
    for field in [
        "id",
        "vault",
        "title",
        "agent",
        "model",
        "created_at",
        "updated_at",
    ] {
        assert!(thread.get(field).is_some(), "missing field {field}");
    }
    assert_eq!(thread["vault"], "vaultA");
    assert_eq!(thread["title"], "Weekly plan");
    assert_eq!(thread["agent"], "copilot");
    let thread_id = thread["id"].as_str().unwrap().to_string();

    // Append a user then an agent message.
    for (role, content) in [("user", "outline the week"), ("agent", "here you go")] {
        let resp = client
            .post(server.url(&format!("{base}/{thread_id}/messages")))
            .json(&json!({ "role": role, "content": content }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    }

    // List messages — assert ordering + field shape.
    let resp = client
        .get(server.url(&format!("{base}/{thread_id}/messages")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let messages: Value = resp.json().await.unwrap();
    let arr = messages.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    for field in ["id", "thread_id", "seq", "role", "content", "created_at"] {
        assert!(arr[0].get(field).is_some(), "missing field {field}");
    }
    assert_eq!(arr[0]["role"], "user");
    assert_eq!(arr[0]["seq"], 1);
    assert_eq!(arr[1]["role"], "agent");
    assert_eq!(arr[1]["seq"], 2);

    // List threads.
    let resp = client.get(server.url(base)).send().await.unwrap();
    let threads: Value = resp.json().await.unwrap();
    assert_eq!(threads.as_array().unwrap().len(), 1);

    // Rename.
    let resp = client
        .post(server.url(&format!("{base}/{thread_id}/rename")))
        .json(&json!({ "title": "Renamed" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let renamed: Value = resp.json().await.unwrap();
    assert_eq!(renamed["title"], "Renamed");

    // Delete.
    let resp = client
        .delete(server.url(&format!("{base}/{thread_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);
    let resp = client
        .get(server.url(&format!("{base}/{thread_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn threads_are_scoped_per_vault_over_http() {
    let server = TestServer::start(&["alpha", "beta"]).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(server.url("/api/v/alpha/agent/threads"))
        .json(&json!({ "title": "alpha secret" }))
        .send()
        .await
        .unwrap();
    let thread: Value = resp.json().await.unwrap();
    let id = thread["id"].as_str().unwrap().to_string();

    // Beta cannot see alpha's thread.
    let resp = client
        .get(server.url("/api/v/beta/agent/threads"))
        .send()
        .await
        .unwrap();
    let beta_threads: Value = resp.json().await.unwrap();
    assert_eq!(beta_threads.as_array().unwrap().len(), 0);

    // Beta cannot fetch or append to it.
    let resp = client
        .get(server.url(&format!("/api/v/beta/agent/threads/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    let resp = client
        .post(server.url(&format!("/api/v/beta/agent/threads/{id}/messages")))
        .json(&json!({ "role": "user", "content": "leak?" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_requests_return_4xx_not_5xx() {
    let server = TestServer::start(&["v"]).await;
    let client = reqwest::Client::new();

    // Empty title rejected.
    let resp = client
        .post(server.url("/api/v/v/agent/threads"))
        .json(&json!({ "title": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // Unknown role rejected as a deserialization 4xx (never a 500).
    let create = client
        .post(server.url("/api/v/v/agent/threads"))
        .json(&json!({ "title": "ok" }))
        .send()
        .await
        .unwrap();
    let id = create.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = client
        .post(server.url(&format!("/api/v/v/agent/threads/{id}/messages")))
        .json(&json!({ "role": "wizard", "content": "x" }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error(), "got {}", resp.status());
}
