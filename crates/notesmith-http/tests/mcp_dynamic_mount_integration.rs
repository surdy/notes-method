//! Integration tests for dynamic per-vault MCP-over-HTTP mounting (Phase 1, #160).
//!
//! Two properties are exercised end-to-end through a real rmcp client driven over
//! the stdio↔HTTP bridge into a live daemon:
//!
//! 1. **Dynamic mounting** — a vault inserted into the daemon's shared state
//!    *after* the server is already serving is reachable over `/mcp/<vault>`
//!    without a restart.
//! 2. **Strict-client `structuredContent` contract** — tools that return bare
//!    arrays/scalars (`search_notes`, `list_notes`, `list_tasks`, `query_sql`)
//!    surface an object-shaped `structuredContent`, so strict clients (e.g.
//!    Copilot) that deserialize it into a JSON object accept the result.

use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

use chrono::Utc;
use notesmith_config::{VaultConfig, migration};
use notesmith_core::VaultEngine;
use notesmith_http::watcher::WatcherState;
use notesmith_http::{AppState, SharedAppState, VaultState, serve_shared_with_listener};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use rmcp::{ServiceExt, model::CallToolRequestParams};
use tempfile::TempDir;
use tokio::sync::RwLock;

fn write_vault(root: &Path, name: &str) {
    std::fs::create_dir_all(root.join(".notesmith")).unwrap();
    std::fs::write(
        root.join(".notesmith/vault.toml"),
        format!("name = \"{name}\"\n\n[capture]\nfolder = \"Inbox\"\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("Welcome.md"),
        "---\ntype: note\n---\nWelcome to the searchable vault.\n\n- [ ] a pending task\n",
    )
    .unwrap();
}

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

fn empty_shared_state(config_root: &Path) -> SharedAppState {
    let (event_tx, _) = notesmith_http::create_event_channel();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    Arc::new(RwLock::new(AppState {
        vaults: Default::default(),
        event_tx,
        event_buffer: Arc::new(notesmith_http::EventBuffer::new(
            notesmith_http::events::EVENT_BUFFER_CAPACITY,
        )),
        global_config_path: config_root.join(".notesmith-dynamic-test-config.toml"),
        started_at: Utc::now(),
        sse_connection_count: Arc::new(AtomicUsize::new(0)),
        shutdown_tx,
        shutdown_rx,
        mcp_services: Default::default(),
        transcripts: Default::default(),
        permissions: Default::default(),
        vault_watchers: Default::default(),
    }))
}

async fn start(state: SharedAppState) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve_shared_with_listener(listener, state, false)
            .await
            .unwrap();
    });
    (address, server)
}

#[tokio::test]
async fn vault_added_after_startup_is_reachable_over_mcp_without_restart() {
    let temp_dir = TempDir::new().unwrap();
    // Daemon starts with no vaults at all.
    let state = empty_shared_state(temp_dir.path());
    let (address, server) = start(state.clone()).await;

    // A new vault appears at runtime (as `reconcile_vaults` would do on a
    // config change) — no server restart.
    let beta_root = temp_dir.path().join("beta");
    write_vault(&beta_root, "beta");
    state
        .write()
        .await
        .vaults
        .insert("beta".to_string(), build_vault_state("beta", &beta_root));

    let endpoint = format!("http://{address}/mcp/beta");
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
        "dynamically mounted vault should surface tools, got: {names:?}"
    );

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
        "search_notes should succeed against the dynamically mounted vault"
    );

    client.cancel().await.unwrap();
    bridge.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn strict_client_receives_object_structured_content() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().join("alpha");
    write_vault(&root, "alpha");

    let state = empty_shared_state(temp_dir.path());
    state
        .write()
        .await
        .vaults
        .insert("alpha".to_string(), build_vault_state("alpha", &root));
    let (address, server) = start(state.clone()).await;

    let endpoint = format!("http://{address}/mcp/alpha");
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let bridge = tokio::spawn(async move {
        notesmith_mcp::run_bridge(endpoint, server_io)
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();

    // Each of these tools returns a bare JSON array from ops, which the
    // object-wrapping fix nests under `results` so strict clients accept the
    // `structuredContent` as a JSON object.
    let array_tools = [
        ("search_notes", serde_json::json!({ "query": "searchable" })),
        ("list_notes", serde_json::json!({})),
        ("list_tasks", serde_json::json!({})),
    ];

    for (tool, args) in array_tools {
        let result = client
            .call_tool(CallToolRequestParams {
                name: tool.into(),
                arguments: Some(args.as_object().unwrap().clone()),
                meta: None,
                task: None,
            })
            .await
            .unwrap_or_else(|error| panic!("{tool} call must not error: {error}"));
        assert_ne!(result.is_error, Some(true), "{tool} should succeed");
        let structured = result
            .structured_content
            .unwrap_or_else(|| panic!("{tool} must return structuredContent"));
        assert!(
            structured.is_object(),
            "{tool} structuredContent must be a JSON object, got: {structured:?}"
        );
        assert!(
            structured.get("results").is_some(),
            "{tool} structuredContent should wrap its array under `results`, got: {structured:?}"
        );
    }

    // `get_note` and `query_sql` already return JSON objects; they must remain
    // unwrapped (no spurious `results` key).
    for (tool, args) in [
        ("get_note", serde_json::json!({ "path": "Welcome.md" })),
        (
            "query_sql",
            serde_json::json!({ "sql": "SELECT path FROM notes LIMIT 1" }),
        ),
    ] {
        let result = client
            .call_tool(CallToolRequestParams {
                name: tool.into(),
                arguments: Some(args.as_object().unwrap().clone()),
                meta: None,
                task: None,
            })
            .await
            .unwrap();
        let structured = result
            .structured_content
            .unwrap_or_else(|| panic!("{tool} must return structuredContent"));
        assert!(structured.is_object(), "{tool} must return an object");
        assert!(
            structured.get("results").is_none(),
            "{tool} already returns an object and must not be re-wrapped: {structured:?}"
        );
    }

    client.cancel().await.unwrap();
    bridge.await.unwrap();
    server.abort();
}
