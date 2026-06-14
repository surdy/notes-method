//! End-to-end tests for the stdio↔HTTP MCP bridge (Phase 3, #150).
//!
//! These drive a real rmcp client over an in-memory transport into
//! [`notesmith_mcp::run_bridge`], which forwards every request over HTTP to a
//! live daemon's per-vault MCP endpoint. This exercises the full proxy path:
//! client → bridge server → HTTP client transport → daemon → ops.

use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

use chrono::Utc;
use notesmith_config::{VaultConfig, migration};
use notesmith_core::VaultEngine;
use notesmith_http::watcher::WatcherState;
use notesmith_http::{AppState, VaultState, serve_with_listener};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use rmcp::{ServiceExt, model::CallToolRequestParams};
use tempfile::TempDir;

struct BridgeFixture {
    _temp_dir: TempDir,
    address: SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl BridgeFixture {
    async fn start() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("vault");
        std::fs::create_dir_all(root.join(".notesmith")).unwrap();
        std::fs::write(
            root.join(".notesmith/vault.toml"),
            "name = \"test-vault\"\n\n[capture]\nfolder = \"Inbox\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Welcome.md"),
            "---\ntype: note\n---\nWelcome to the searchable vault.\n",
        )
        .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = build_state("test-vault", &root);
        let server = tokio::spawn(async move {
            serve_with_listener(listener, state).await.unwrap();
        });

        Self {
            _temp_dir: temp_dir,
            address,
            server,
        }
    }

    fn endpoint(&self, suffix: &str) -> String {
        format!("http://{}{}", self.address, suffix)
    }
}

fn build_state(vault_name: &str, root: &Path) -> AppState {
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

    let vault_state = VaultState {
        cache: Arc::new(cache),
        search_index: Arc::new(search_index),
        engine,
        root: root.to_path_buf(),
        vault_config: arc_swap::ArcSwap::from_pointee(vault_config),
        watcher_state: WatcherState::new(),
        rebuilding: std::sync::atomic::AtomicBool::new(false),
        template_engine: Arc::new(template_engine),
    };

    let (event_tx, _) = notesmith_http::create_event_channel();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    AppState {
        vaults: [(vault_name.to_string(), vault_state)]
            .into_iter()
            .collect(),
        event_tx,
        event_buffer: Arc::new(notesmith_http::EventBuffer::new(
            notesmith_http::events::EVENT_BUFFER_CAPACITY,
        )),
        global_config_path: root.join(".notesmith-bridge-test-config.toml"),
        started_at: Utc::now(),
        sse_connection_count: Arc::new(AtomicUsize::new(0)),
        shutdown_tx,
        shutdown_rx,
        mcp_services: Default::default(),
    }
}

#[tokio::test]
async fn bridge_forwards_list_tools_and_call_tool_to_daemon() {
    let fixture = BridgeFixture::start().await;
    let endpoint = fixture.endpoint("/mcp/test-vault");

    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let bridge = tokio::spawn(async move {
        notesmith_mcp::run_bridge(endpoint, server_io)
            .await
            .unwrap();
    });

    let client = ().serve(client_io).await.unwrap();

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(
        names.contains(&"search_notes"),
        "bridge should surface the daemon's tools, got: {names:?}"
    );
    assert!(names.contains(&"create_note"));

    let result = client
        .call_tool(CallToolRequestParams {
            name: "search_notes".into(),
            arguments: Some(
                serde_json::json!({ "query": "searchable" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        })
        .await
        .unwrap();
    assert_ne!(
        result.is_error,
        Some(true),
        "search_notes should succeed through the bridge"
    );

    client.cancel().await.unwrap();
    bridge.await.unwrap();
    fixture.server.abort();
}

#[tokio::test]
async fn read_only_bridge_rejects_write_tools() {
    let fixture = BridgeFixture::start().await;
    let endpoint = fixture.endpoint("/mcp-ro/test-vault");

    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let bridge = tokio::spawn(async move {
        notesmith_mcp::run_bridge(endpoint, server_io)
            .await
            .unwrap();
    });

    let client = ().serve(client_io).await.unwrap();

    let result = client
        .call_tool(CallToolRequestParams {
            name: "create_note".into(),
            arguments: Some(
                serde_json::json!({ "title": "Should Fail" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        })
        .await
        .unwrap();

    assert_eq!(
        result.is_error,
        Some(true),
        "read-only bridge must reject write tools"
    );

    client.cancel().await.unwrap();
    bridge.await.unwrap();
    fixture.server.abort();
}

#[tokio::test]
async fn bridge_round_trips_write_then_read_tools() {
    let fixture = BridgeFixture::start().await;
    let endpoint = fixture.endpoint("/mcp/test-vault");

    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let bridge = tokio::spawn(async move {
        notesmith_mcp::run_bridge(endpoint, server_io)
            .await
            .unwrap();
    });

    let client = ().serve(client_io).await.unwrap();

    // Write a note through the read-write bridge.
    let created = client
        .call_tool(CallToolRequestParams {
            name: "create_note".into(),
            arguments: Some(
                serde_json::json!({ "title": "Bridge Roundtrip", "content": "hello from bridge" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        })
        .await
        .unwrap();
    assert_ne!(
        created.is_error,
        Some(true),
        "create_note should succeed through the read-write bridge"
    );
    let path = created
        .structured_content
        .as_ref()
        .and_then(|value| value.get("path"))
        .and_then(|value| value.as_str())
        .expect("create_note must return the new note path")
        .to_string();

    // Read it back through the same bridge.
    let fetched = client
        .call_tool(CallToolRequestParams {
            name: "get_note".into(),
            arguments: Some(
                serde_json::json!({ "path": path })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        })
        .await
        .unwrap();
    assert_ne!(fetched.is_error, Some(true), "get_note should succeed");
    let body = serde_json::to_string(&fetched.structured_content).unwrap();
    assert!(
        body.contains("hello from bridge"),
        "round-tripped note content should be readable, got: {body}"
    );

    client.cancel().await.unwrap();
    bridge.await.unwrap();
    fixture.server.abort();
}

#[tokio::test]
async fn bridge_errors_promptly_when_daemon_unreachable() {
    // Port 1 is reserved and refuses connections, so the bridge's initial MCP
    // handshake fails fast instead of hanging or panicking.
    let (_client_io, server_io) = tokio::io::duplex(64 * 1024);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        notesmith_mcp::run_bridge("http://127.0.0.1:1/mcp/test-vault".to_string(), server_io),
    )
    .await;

    let bridge_result = result.expect("bridge must not hang when the daemon is unreachable");
    assert!(
        bridge_result.is_err(),
        "bridge should surface an error when the daemon endpoint is unreachable"
    );
}
