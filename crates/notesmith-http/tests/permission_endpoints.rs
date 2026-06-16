//! HTTP contract tests for the per-vault agent permission-grant endpoints
//! (`/api/v/{vault}/agent/permissions...`). These assert the exact JSON shapes
//! the desktop chat UI depends on and the per-vault isolation guarantee, per
//! the repo's frontend–backend contract-testing rule (issue #189).

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
async fn grant_list_and_revoke_round_trip() {
    let server = TestServer::start(&["vaultA"]).await;
    let client = reqwest::Client::new();
    let base = "/api/v/vaultA/agent/permissions";

    // Initially empty.
    let resp = client.get(server.url(base)).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let listed: Value = resp.json().await.unwrap();
    assert_eq!(listed, json!([]));

    // Persist a grant.
    let resp = client
        .post(server.url(base))
        .json(&json!({ "tool": "create_note" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);

    // GET returns it (exact shape: a flat array of tool-name strings).
    let resp = client.get(server.url(base)).send().await.unwrap();
    let listed: Vec<String> = resp.json().await.unwrap();
    assert_eq!(listed, vec!["create_note".to_string()]);

    // A repeated grant is idempotent.
    client
        .post(server.url(base))
        .json(&json!({ "tool": "create_note" }))
        .send()
        .await
        .unwrap();
    let listed: Vec<String> = client
        .get(server.url(base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed, vec!["create_note".to_string()]);

    // DELETE removes it.
    let resp = client
        .delete(server.url(&format!("{base}/create_note")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);
    let listed: Vec<String> = client
        .get(server.url(base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(listed.is_empty());
}

#[tokio::test]
async fn grants_are_isolated_per_vault() {
    let server = TestServer::start(&["vaultA", "vaultB"]).await;
    let client = reqwest::Client::new();

    client
        .post(server.url("/api/v/vaultA/agent/permissions"))
        .json(&json!({ "tool": "create_note" }))
        .send()
        .await
        .unwrap();

    // Vault B never sees vault A's grant.
    let listed: Vec<String> = client
        .get(server.url("/api/v/vaultB/agent/permissions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(listed.is_empty());

    let listed: Vec<String> = client
        .get(server.url("/api/v/vaultA/agent/permissions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed, vec!["create_note".to_string()]);
}

#[tokio::test]
async fn empty_tool_is_rejected() {
    let server = TestServer::start(&["vaultA"]).await;
    let client = reqwest::Client::new();
    let resp = client
        .post(server.url("/api/v/vaultA/agent/permissions"))
        .json(&json!({ "tool": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}
