use std::{
    collections::BTreeMap,
    fs,
    net::SocketAddr,
    path::Path,
    path::PathBuf,
    sync::{Arc, atomic::AtomicUsize},
};

use chrono::Utc;
use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration, migration};
use notesmith_core::VaultEngine;
use notesmith_http::watcher::WatcherState;
use notesmith_http::{AppState, VaultState, serve_with_listener};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use tempfile::TempDir;

fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
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
        cache: std::sync::Arc::new(cache),
        search_index: std::sync::Arc::new(search_index),
        engine,
        root: root.to_path_buf(),
        vault_config: arc_swap::ArcSwap::from_pointee(vault_config),
        watcher_state: WatcherState::new(),
        rebuilding: std::sync::atomic::AtomicBool::new(false),
        template_engine: std::sync::Arc::new(template_engine),
    }
}

fn build_test_state(root: &Path) -> AppState {
    build_test_state_with_vaults(
        &[("test-vault".to_string(), root.to_path_buf())],
        root.join(".notesmith-http-test-config.toml"),
    )
}

fn build_test_state_with_vaults(
    vaults: &[(String, PathBuf)],
    global_config_path: PathBuf,
) -> AppState {
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
        global_config_path,
        started_at: Utc::now(),
        sse_connection_count: Arc::new(AtomicUsize::new(0)),
        shutdown_tx,
        shutdown_rx,
        mcp_services: Default::default(),
        transcripts: Default::default(),
    }
}

fn write_global_config(
    config_path: &Path,
    vaults: &[(String, PathBuf)],
    default_vault: Option<&str>,
) {
    let config = GlobalConfig {
        daemon: Default::default(),
        default_vault: default_vault.map(str::to_string),
        vaults: vaults
            .iter()
            .map(|(name, path)| (name.clone(), VaultRegistration { path: path.clone() }))
            .collect::<BTreeMap<_, _>>(),
        agents: Default::default(),
    };
    config.save_to(config_path).unwrap();
}

fn write_note(root: &Path, relative_path: &str, content: &str) {
    let absolute_path = root.join(relative_path);
    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(absolute_path, content).unwrap();
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

struct TestServer {
    _temp_dir: TempDir,
    root: PathBuf,
    address: SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl TestServer {
    async fn empty() -> Self {
        Self::with_files(&[]).await
    }

    async fn with_files(files: &[(&str, &str)]) -> Self {
        Self::with_config_and_files(
            "name = \"test-vault\"\n\n[capture]\nfolder = \"Inbox\"\n\n[daily]\nfolder = \"Inbox/Daily\"\n",
            files,
        )
        .await
    }

    async fn with_config_and_files(config: &str, files: &[(&str, &str)]) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("vault");
        fs::create_dir_all(root.join(".notesmith")).unwrap();
        fs::write(root.join(".notesmith/vault.toml"), config).unwrap();
        for (path, content) in files {
            write_note(&root, path, content);
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = build_test_state(&root);
        let server = tokio::spawn(async move {
            serve_with_listener(listener, state).await.unwrap();
        });

        Self {
            _temp_dir: temp_dir,
            root,
            address,
            server,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
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
async fn list_vaults_returns_all_registered_vaults() {
    let temp_dir = TempDir::new().unwrap();
    let first_root = temp_dir.path().join("alpha");
    let second_root = temp_dir.path().join("beta");
    fs::create_dir_all(&first_root).unwrap();
    fs::create_dir_all(&second_root).unwrap();

    let vaults = vec![
        ("alpha".to_string(), first_root.clone()),
        ("beta".to_string(), second_root.clone()),
    ];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &vaults, Some("beta"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&vaults, config_path);

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/app/vaults"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!([
            {
                "name": "alpha",
                "path": first_root.to_string_lossy(),
                "is_default": false
            },
            {
                "name": "beta",
                "path": second_root.to_string_lossy(),
                "is_default": true
            }
        ])
    );

    server.abort();
}

#[tokio::test]
async fn add_vault_with_valid_path_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let existing_root = temp_dir.path().join("alpha");
    let new_root = temp_dir.path().join("beta");
    fs::create_dir_all(&existing_root).unwrap();
    fs::create_dir_all(&new_root).unwrap();

    let registered_vaults = vec![("alpha".to_string(), existing_root.clone())];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered_vaults, Some("alpha"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered_vaults, config_path.clone());

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/app/vaults"))
        .json(&serde_json::json!({
            "name": "beta",
            "path": new_root,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let config = GlobalConfig::load_from(&config_path).unwrap();
    assert_eq!(
        config
            .vault("beta")
            .map(|registration| registration.path.clone()),
        Some(temp_dir.path().join("beta"))
    );
    assert!(temp_dir.path().join("beta/.notesmith").exists());

    server.abort();
}

#[tokio::test]
async fn add_vault_with_duplicate_name_returns_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let existing_root = temp_dir.path().join("alpha");
    fs::create_dir_all(&existing_root).unwrap();

    let registered_vaults = vec![("alpha".to_string(), existing_root.clone())];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered_vaults, Some("alpha"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered_vaults, config_path);

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/app/vaults"))
        .json(&serde_json::json!({
            "name": "alpha",
            "path": existing_root,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    server.abort();
}

#[tokio::test]
async fn add_vault_with_create_flag_creates_missing_directory() {
    let temp_dir = TempDir::new().unwrap();
    let existing_root = temp_dir.path().join("alpha");
    let new_root = temp_dir.path().join("nested/beta");
    fs::create_dir_all(&existing_root).unwrap();

    let registered_vaults = vec![("alpha".to_string(), existing_root.clone())];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered_vaults, Some("alpha"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered_vaults, config_path.clone());

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/app/vaults"))
        .json(&serde_json::json!({
            "name": "beta",
            "path": new_root,
            "create": true,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    assert!(new_root.exists());
    assert!(new_root.join(".notesmith").exists());

    server.abort();
}

#[tokio::test]
async fn add_vault_without_create_flag_rejects_missing_directory() {
    let temp_dir = TempDir::new().unwrap();
    let existing_root = temp_dir.path().join("alpha");
    let new_root = temp_dir.path().join("does-not-exist");
    fs::create_dir_all(&existing_root).unwrap();

    let registered_vaults = vec![("alpha".to_string(), existing_root.clone())];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered_vaults, Some("alpha"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered_vaults, config_path);

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/app/vaults"))
        .json(&serde_json::json!({
            "name": "beta",
            "path": new_root,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!new_root.exists());

    server.abort();
}

#[tokio::test]
async fn add_vault_emits_vaults_changed_event() {
    use tokio::time::{Duration, timeout};

    let temp_dir = TempDir::new().unwrap();
    let existing_root = temp_dir.path().join("alpha");
    let new_root = temp_dir.path().join("beta");
    fs::create_dir_all(&existing_root).unwrap();
    fs::create_dir_all(&new_root).unwrap();

    let registered_vaults = vec![("alpha".to_string(), existing_root.clone())];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered_vaults, Some("alpha"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered_vaults, config_path);

    // Subscribe to events before the change.
    let mut rx = state.event_tx.subscribe();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/app/vaults"))
        .json(&serde_json::json!({
            "name": "beta",
            "path": new_root,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("event arrived in time")
        .expect("event channel ok");

    assert_eq!(event.event_type.as_str(), "vaults.changed");
    assert_eq!(event.vault, "beta");

    server.abort();
}

#[tokio::test]
async fn remove_default_vault_promotes_another_to_default() {
    let temp_dir = TempDir::new().unwrap();
    let alpha = temp_dir.path().join("alpha");
    let beta = temp_dir.path().join("beta");
    fs::create_dir_all(&alpha).unwrap();
    fs::create_dir_all(&beta).unwrap();

    let registered = vec![
        ("alpha".to_string(), alpha.clone()),
        ("beta".to_string(), beta.clone()),
    ];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered, Some("alpha"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered, config_path.clone());

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .delete(format!("http://{address}/api/app/vaults/alpha"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    let config = GlobalConfig::load_from(&config_path).unwrap();
    assert!(config.vault("alpha").is_none());
    assert!(config.vault("beta").is_some());
    assert_eq!(config.default_vault.as_deref(), Some("beta"));

    server.abort();
}

#[tokio::test]
async fn remove_last_vault_clears_default() {
    let temp_dir = TempDir::new().unwrap();
    let solo = temp_dir.path().join("solo");
    fs::create_dir_all(&solo).unwrap();

    let registered = vec![("solo".to_string(), solo.clone())];
    let config_path = temp_dir.path().join("config/notesmith/config.toml");
    write_global_config(&config_path, &registered, Some("solo"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state_with_vaults(&registered, config_path.clone());

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .delete(format!("http://{address}/api/app/vaults/solo"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    let config = GlobalConfig::load_from(&config_path).unwrap();
    assert!(config.vaults.is_empty());
    assert_eq!(config.default_vault, None);

    server.abort();
}

#[tokio::test]
async fn list_notes_returns_cached_notes_for_vault() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
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
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
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
    assert_eq!(body["truncated"], serde_json::json!(false));

    server.abort();
}

#[tokio::test]
async fn get_sidebar_config_returns_configured_views() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/sidebar-config"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let etag = response
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(etag.starts_with('"') && etag.ends_with('"'));

    let body = response.json::<serde_json::Value>().await.unwrap();
    let views = body["config"]["views"].as_array().unwrap();
    assert_eq!(views.len(), 1, "expected one configured view (Triage)");
    assert_eq!(views[0]["name"], "Triage");
    assert_eq!(body["path"], ".notesmith/sidebar.yaml");
    assert!(body["warnings"].is_object());
    assert!(body["hash"].as_str().unwrap().len() > 10);

    server.abort();
}

#[tokio::test]
async fn get_sidebar_config_returns_empty_views_without_config_file() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/sidebar-config"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<serde_json::Value>().await.unwrap();
    let views = body["config"]["views"].as_array().unwrap();
    assert!(
        views.is_empty(),
        "expected empty views when no sidebar.yaml exists"
    );
    assert_eq!(body["hash"], "");
    assert_eq!(body["path"], ".notesmith/sidebar.yaml");
    assert!(body["warnings"].is_object());

    server.abort();
}

#[tokio::test]
async fn put_sidebar_config_succeeds_with_correct_if_match() {
    let server = TestServer::empty().await;
    write_sidebar_config(
        &server.root,
        "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n",
    );

    let get_response = reqwest::get(server.url("/api/v/test-vault/sidebar-config"))
        .await
        .unwrap();
    let get_body = get_response.json::<serde_json::Value>().await.unwrap();
    let hash = get_body["hash"].as_str().unwrap();

    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "views": [
            {
                "id": "customers",
                "name": "Customers",
                "icon": "🏢",
                "sections": [
                    {
                        "type": "custom-folders",
                        "label": "Key Accounts",
                        "folders": ["Customers"]
                    }
                ],
                "badge_query": "SELECT COUNT(*) FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' WHERE note_type.value = 'customer'"
            }
        ]
    });

    let response = client
        .put(server.url("/api/v/test-vault/sidebar-config"))
        .header("if-match", format!("\"{hash}\""))
        .json(&new_config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let etag = response
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(etag.starts_with('"') && etag.ends_with('"'));

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["config"]["views"][0]["name"], "Customers");
    assert_eq!(body["path"], ".notesmith/sidebar.yaml");
    assert!(body["warnings"].is_object());
    assert!(body["hash"].as_str().unwrap().len() > 10);

    let saved = fs::read_to_string(server.root.join(".notesmith/sidebar.yaml")).unwrap();
    assert!(saved.contains("id: customers"));

    server.server.abort();
}

#[tokio::test]
async fn put_sidebar_config_returns_409_on_stale_if_match() {
    let server = TestServer::empty().await;
    write_sidebar_config(
        &server.root,
        "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n",
    );

    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "views": [
            {
                "id": "customers",
                "name": "Customers",
                "icon": "🏢",
                "sections": [],
                "badge_query": null
            }
        ]
    });

    let response = client
        .put(server.url("/api/v/test-vault/sidebar-config"))
        .header("if-match", "\"stale-hash-value\"")
        .json(&new_config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "conflict");
    assert!(body["config"].is_object());
    assert!(body["hash"].as_str().is_some());
    assert!(body["warnings"].is_object());

    server.server.abort();
}

#[tokio::test]
async fn put_sidebar_config_returns_428_without_if_match() {
    let server = TestServer::empty().await;
    write_sidebar_config(
        &server.root,
        "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n",
    );

    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "views": [
            {
                "id": "customers",
                "name": "Customers",
                "icon": "🏢",
                "sections": [],
                "badge_query": null
            }
        ]
    });

    let response = client
        .put(server.url("/api/v/test-vault/sidebar-config"))
        .json(&new_config)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        reqwest::StatusCode::PRECONDITION_REQUIRED
    );

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "if_match_required");

    server.server.abort();
}

#[tokio::test]
async fn put_sidebar_config_returns_422_with_invalid_data() {
    let server = TestServer::empty().await;
    write_sidebar_config(
        &server.root,
        "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n",
    );

    let get_response = reqwest::get(server.url("/api/v/test-vault/sidebar-config"))
        .await
        .unwrap();
    let get_body = get_response.json::<serde_json::Value>().await.unwrap();
    let hash = get_body["hash"].as_str().unwrap();

    let client = reqwest::Client::new();
    let bad_config = serde_json::json!({
        "views": [
            {
                "id": "work",
                "name": "Work",
                "icon": "💼",
                "sections": [
                    {
                        "type": "recently-viewed",
                        "label": "",
                        "limit": 0,
                        "mode": "both"
                    }
                ],
                "badge_query": null
            },
            {
                "id": "work",
                "name": "Duplicate",
                "icon": "📌",
                "sections": [
                    {
                        "type": "custom-folders",
                        "label": "Folders",
                        "folders": []
                    }
                ],
                "badge_query": null
            }
        ]
    });

    let response = client
        .put(server.url("/api/v/test-vault/sidebar-config"))
        .header("if-match", format!("\"{hash}\""))
        .json(&bad_config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "validation_failed");
    let errors = body["errors"].as_object().unwrap();
    assert!(errors.contains_key("views[0].sections[0].label"));
    assert!(errors.contains_key("views[0].sections[0].limit"));
    assert!(errors.contains_key("views[1].id"));
    assert!(errors.contains_key("views[1].sections[0].folders"));

    server.server.abort();
}

#[tokio::test]
async fn put_sidebar_config_rejects_disallowed_origin() {
    let server = TestServer::empty().await;
    write_sidebar_config(
        &server.root,
        "views:\n  - id: work\n    name: Work\n    icon: \"💼\"\n",
    );

    let client = reqwest::Client::new();
    let config = serde_json::json!({
        "views": [
            {
                "id": "customers",
                "name": "Customers",
                "icon": "🏢",
                "sections": [],
                "badge_query": null
            }
        ]
    });

    let response = client
        .put(server.url("/api/v/test-vault/sidebar-config"))
        .header("origin", "https://evil.example.com")
        .header("if-match", "\"somehash\"")
        .json(&config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "origin_not_allowed");

    server.server.abort();
}

#[tokio::test]
async fn get_folders_returns_sorted_relative_paths_without_hidden_directories() {
    let server = TestServer::empty().await;
    fs::create_dir_all(server.root.join("Customers/Acme")).unwrap();
    fs::create_dir_all(server.root.join("Projects/Active")).unwrap();
    fs::create_dir_all(server.root.join(".notesmith/private")).unwrap();
    fs::create_dir_all(server.root.join("Customers/.secret")).unwrap();

    let response = reqwest::get(server.url("/api/v/test-vault/folders"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<String>>().await.unwrap();
    assert_eq!(
        body,
        vec![
            "Customers".to_string(),
            "Customers/Acme".to_string(),
            "Projects".to_string(),
            "Projects/Active".to_string(),
        ]
    );

    server.server.abort();
}

#[tokio::test]
async fn get_note_returns_full_note_metadata() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
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
    assert!(body["hash"].as_str().is_some_and(|hash| hash.len() > 10));
    assert!(body["tasks"].as_array().unwrap().len() >= 5);
    assert!(body["links"].as_array().unwrap().len() >= 3);
    assert!(body["inline_fields"].as_array().unwrap().len() >= 2);

    server.abort();
}

#[tokio::test]
async fn get_note_html_renders_markdown_without_frontmatter() {
    let server = TestServer::with_files(&[(
        "Inbox/Rendered.md",
        "---\nstatus: draft\n---\n# Heading\n\nLine one\nLine two\n\n[[Target]]\n\n> [!info] Title\n> body\n",
    )])
    .await;

    let response = reqwest::get(server.url("/api/v/test-vault/html/Inbox/Rendered.md"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.text().await.unwrap();
    assert!(body.contains("<h1>Heading</h1>"), "body was: {body}");
    assert!(!body.contains("status: draft"), "body was: {body}");
    assert!(body.contains("<br"), "body was: {body}");
    assert!(
        body.contains(r#"<a class="wikilink" data-target="Target">Target</a>"#),
        "body was: {body}"
    );
    assert!(
        body.contains(r#"<div class="callout callout-info" data-callout="info">"#),
        "body was: {body}"
    );

    server.server.abort();
}

#[tokio::test]
async fn get_note_html_respects_strict_line_breaks_config() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().join("vault");
    fs::create_dir_all(root.join(".notesmith")).unwrap();
    fs::write(
        root.join(".notesmith/vault.toml"),
        "name = \"test-vault\"\n\n[editor]\nstrict_line_breaks = true\n",
    )
    .unwrap();
    write_note(&root, "Inbox/Rendered.md", "Line one\nLine two\n");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state(&root);
    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let response = reqwest::get(format!(
        "http://{address}/api/v/test-vault/html/Inbox/Rendered.md"
    ))
    .await
    .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.text().await.unwrap();
    assert!(!body.contains("<br"), "body was: {body}");

    server.abort();
}

#[tokio::test]
async fn get_note_html_with_inline_styles_returns_portable_document() {
    let server = TestServer::with_files(&[(
        "Inbox/Rendered.md",
        "---\nstatus: draft\n---\n# Heading\n\n[[Target|Alias]]\n",
    )])
    .await;

    let response =
        reqwest::get(server.url("/api/v/test-vault/html/Inbox/Rendered.md?inline_styles=true"))
            .await
            .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.text().await.unwrap();
    assert!(body.contains("<html"), "body was: {body}");
    assert!(body.contains("<style>"), "body was: {body}");
    assert!(!body.contains("status: draft"), "body was: {body}");
    assert!(
        body.contains(r#"<a href="Target">Alias</a>"#),
        "body was: {body}"
    );
    assert!(!body.contains("class=\"wikilink\""), "body was: {body}");

    server.server.abort();
}

#[tokio::test]
async fn search_returns_matching_notes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
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

#[tokio::test]
async fn create_note_returns_path_and_hash() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "Example",
            "content": "Hello world",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/Example.md"));
    assert!(body["hash"].as_str().unwrap().len() > 10);
    assert!(server.root.join("Inbox/Example.md").exists());

    server.server.abort();
}

#[tokio::test]
async fn hook_fires_on_note_create() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().join("vault");
    fs::create_dir_all(&root).unwrap();

    let hook_script = root.join("hook.sh");
    write_executable(&hook_script, "#!/bin/sh\ncat > hook-fired.json\n");
    fs::create_dir_all(root.join(".notesmith")).unwrap();
    fs::write(
        root.join(".notesmith").join("vault.toml"),
        "name = \"test-vault\"\n\n[capture]\nfolder = \"Inbox\"\n\n[hooks]\non_note_create = \"hook.sh\"\n",
    )
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = build_test_state(&root);
    let hook_rx = state.event_tx.subscribe();
    let vault = state.vaults.get("test-vault").unwrap();
    let hook_listener = notesmith_http::start_hook_listener(
        hook_rx,
        vec![notesmith_http::hooks::HookVaultContext {
            vault_name: "test-vault".to_string(),
            vault_root: vault.root.clone(),
            hooks_config: vault.vault_config.load().hooks.clone(),
        }],
        notesmith_hooks::HookRunner::default(),
    );

    let server = tokio::spawn(async move {
        serve_with_listener(listener, state).await.unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "Hook Test",
            "content": "Hook me"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let marker = root.join("hook-fired.json");
    for _ in 0..20 {
        if marker.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(marker.exists());
    let payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(marker).unwrap()).unwrap();
    assert_eq!(payload["event"], serde_json::json!("on_note_create"));
    assert_eq!(payload["vault"], serde_json::json!("test-vault"));
    assert_eq!(payload["path"], serde_json::json!("Inbox/Hook Test.md"));

    hook_listener.abort();
    server.abort();
}

#[tokio::test]
async fn create_note_conflict_when_exists() {
    let server = TestServer::with_files(&[("Inbox/Example.md", "# Existing\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "Example",
            "content": "Hello world",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/Example.md"));

    server.server.abort();
}

#[tokio::test]
async fn put_note_replaces_content() {
    let server = TestServer::with_files(&[("Inbox/Example.md", "# Old body\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .put(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .json(&serde_json::json!({
            "content": "---\ntitle: Example\n---\nNew body",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let get_response = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .send()
        .await
        .unwrap();
    let body = get_response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["body"], serde_json::json!("New body\n"));

    server.server.abort();
}

#[tokio::test]
async fn put_note_conflict_detection() {
    let original_content = "# Old body\n";
    let server = TestServer::with_files(&[("Inbox/Example.md", original_content)]).await;
    let client = reqwest::Client::new();

    let response = client
        .put(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .json(&serde_json::json!({
            "content": "# Replaced\n",
            "expected_hash": "wrong-hash",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["expected"], serde_json::json!("wrong-hash"));
    assert_eq!(
        body["actual"],
        serde_json::json!(
            blake3::hash(original_content.as_bytes())
                .to_hex()
                .to_string()
        )
    );

    server.server.abort();
}

#[tokio::test]
async fn patch_note_merges_frontmatter() {
    let server =
        TestServer::with_files(&[("Inbox/Example.md", "---\nstatus: draft\n---\nPatch me\n")])
            .await;
    let client = reqwest::Client::new();

    let response = client
        .patch(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .json(&serde_json::json!({
            "frontmatter": {
                "owner": "me"
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let get_response = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .send()
        .await
        .unwrap();
    let body = get_response.json::<serde_json::Value>().await.unwrap();
    let raw_frontmatter = body["raw_frontmatter"].as_str().unwrap();
    assert!(raw_frontmatter.contains("status: draft"));
    assert!(raw_frontmatter.contains("owner: me"));

    server.server.abort();
}

#[tokio::test]
async fn delete_note_removes_file() {
    let server = TestServer::with_files(&[("Inbox/Example.md", "# Delete me\n")]).await;
    let client = reqwest::Client::new();

    let delete_response = client
        .delete(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .send()
        .await
        .unwrap();

    assert_eq!(delete_response.status(), reqwest::StatusCode::NO_CONTENT);
    assert!(!server.root.join("Inbox/Example.md").exists());

    let get_response = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}

#[tokio::test]
async fn append_note_adds_content() {
    let server =
        TestServer::with_files(&[("Inbox/Example.md", "---\nkind: note\n---\nFirst line\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes-append/Inbox/Example.md"))
        .json(&serde_json::json!({
            "content": "Second line"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let get_response = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .send()
        .await
        .unwrap();
    let body = get_response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["body"], serde_json::json!("First line\nSecond line\n"));

    server.server.abort();
}

#[tokio::test]
async fn move_note_changes_path() {
    let server = TestServer::with_files(&[("Inbox/Example.md", "# Move me\n")]).await;
    let client = reqwest::Client::new();

    let move_response = client
        .post(server.url("/api/v/test-vault/notes-move/Inbox/Example.md"))
        .json(&serde_json::json!({
            "destination": "Archive/Example.md"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(move_response.status(), reqwest::StatusCode::OK);
    let body = move_response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["from"], serde_json::json!("Inbox/Example.md"));
    assert_eq!(body["to"], serde_json::json!("Archive/Example.md"));

    let old_response = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Example.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(old_response.status(), reqwest::StatusCode::NOT_FOUND);

    let new_response = client
        .get(server.url("/api/v/test-vault/notes/Archive/Example.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(new_response.status(), reqwest::StatusCode::OK);

    server.server.abort();
}

#[tokio::test]
async fn rename_note_renames_file_and_rewrites_wikilinks() {
    let server = TestServer::with_files(&[
        ("Inbox/Old Name.md", "# Old\n"),
        ("Inbox/Other.md", "see [[Old Name]] and [[Old Name|alias]]"),
        ("Sub/Embed.md", "![[Old Name#section]]"),
    ])
    .await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes-rename/Inbox/Old Name.md"))
        .json(&serde_json::json!({ "name": "New Name" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["from"], "Inbox/Old Name.md");
    assert_eq!(body["to"], "Inbox/New Name.md");
    assert_eq!(body["references_rewritten"], 3);

    let old_resp = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Old Name.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(old_resp.status(), reqwest::StatusCode::NOT_FOUND);

    let new_resp = client
        .get(server.url("/api/v/test-vault/notes/Inbox/New Name.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(new_resp.status(), reqwest::StatusCode::OK);

    let other_body = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Other.md"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(
        other_body["body"],
        serde_json::json!("see [[New Name]] and [[New Name|alias]]")
    );

    let embed_body = client
        .get(server.url("/api/v/test-vault/notes/Sub/Embed.md"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(
        embed_body["body"],
        serde_json::json!("![[New Name#section]]")
    );

    server.server.abort();
}

#[tokio::test]
async fn rename_note_returns_409_on_collision() {
    let server =
        TestServer::with_files(&[("Inbox/Foo.md", "# Foo\n"), ("Inbox/Bar.md", "# Bar\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes-rename/Inbox/Foo.md"))
        .json(&serde_json::json!({ "name": "Bar" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    server.server.abort();
}

#[tokio::test]
async fn rename_note_returns_400_on_invalid_name() {
    let server = TestServer::with_files(&[("Inbox/Foo.md", "# Foo\n")]).await;
    let client = reqwest::Client::new();

    for bad in ["", "   ", "a/b", "a\\b", "name:with:colons", "?"] {
        let response = client
            .post(server.url("/api/v/test-vault/notes-rename/Inbox/Foo.md"))
            .json(&serde_json::json!({ "name": bad }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "expected 400 for name {bad:?}"
        );
    }

    server.server.abort();
}

#[tokio::test]
async fn rename_note_returns_404_when_missing() {
    let server = TestServer::with_files(&[]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes-rename/Inbox/Missing.md"))
        .json(&serde_json::json!({ "name": "New" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}

#[tokio::test]
async fn rename_note_strips_md_suffix_from_user_input() {
    let server = TestServer::with_files(&[("Foo.md", "# Foo\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/notes-rename/Foo.md"))
        .json(&serde_json::json!({ "name": "Bar.md" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["to"], "Bar.md");

    server.server.abort();
}

#[tokio::test]
async fn rename_folder_syncs_same_name_folder_note() {
    let server = TestServer::with_files(&[
        ("Customers/Acme/Acme.md", "# Acme\n"),
        ("Customers/Acme/Child.md", "# Child\n"),
    ])
    .await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/folders-rename/Customers/Acme"))
        .json(&serde_json::json!({ "name": "Globex" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["from"], serde_json::json!("Customers/Acme"));
    assert_eq!(body["to"], serde_json::json!("Customers/Globex"));
    assert_eq!(
        body["folder_note_from"],
        serde_json::json!("Customers/Acme/Acme.md")
    );
    assert_eq!(
        body["folder_note_to"],
        serde_json::json!("Customers/Globex/Globex.md")
    );
    assert!(!server.root.join("Customers/Acme").exists());
    assert!(server.root.join("Customers/Globex/Globex.md").exists());
    assert!(server.root.join("Customers/Globex/Child.md").exists());
    assert!(!server.root.join("Customers/Globex/Acme.md").exists());

    server.server.abort();
}

#[tokio::test]
async fn rename_folder_without_folder_note_moves_folder_contents() {
    let server = TestServer::with_files(&[("Projects/Alpha/Brief.md", "# Brief\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/folders-rename/Projects/Alpha"))
        .json(&serde_json::json!({ "name": "Beta" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["from"], serde_json::json!("Projects/Alpha"));
    assert_eq!(body["to"], serde_json::json!("Projects/Beta"));
    assert_eq!(body["folder_note_from"], serde_json::Value::Null);
    assert_eq!(body["folder_note_to"], serde_json::Value::Null);
    assert!(!server.root.join("Projects/Alpha").exists());
    assert!(server.root.join("Projects/Beta/Brief.md").exists());

    server.server.abort();
}

#[tokio::test]
async fn rename_folder_blocks_destination_collision() {
    let server = TestServer::with_files(&[
        ("Customers/Acme/Acme.md", "# Acme\n"),
        ("Customers/Globex/Other.md", "# Existing\n"),
    ])
    .await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/folders-rename/Customers/Acme"))
        .json(&serde_json::json!({ "name": "Globex" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    assert!(server.root.join("Customers/Acme/Acme.md").exists());
    assert!(server.root.join("Customers/Globex/Other.md").exists());

    server.server.abort();
}

#[tokio::test]
async fn rename_folder_blocks_folder_note_filename_collision_inside_source() {
    let server = TestServer::with_files(&[
        ("Customers/Acme/Acme.md", "# Folder note\n"),
        ("Customers/Acme/Globex.md", "# Existing unrelated note\n"),
    ])
    .await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/folders-rename/Customers/Acme"))
        .json(&serde_json::json!({ "name": "Globex" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    assert!(server.root.join("Customers/Acme/Acme.md").exists());
    assert!(server.root.join("Customers/Acme/Globex.md").exists());
    assert!(!server.root.join("Customers/Globex").exists());

    server.server.abort();
}

#[tokio::test]
async fn rename_folder_rejects_unsafe_paths_and_names() {
    let server = TestServer::with_files(&[("Customers/Acme/Acme.md", "# Acme\n")]).await;
    let client = reqwest::Client::new();

    let unsafe_source = client
        .post(server.url("/api/v/test-vault/folders-rename/Customers%5CAcme"))
        .json(&serde_json::json!({ "name": "Globex" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unsafe_source.status(), reqwest::StatusCode::BAD_REQUEST);

    let unsafe_name = client
        .post(server.url("/api/v/test-vault/folders-rename/Customers/Acme"))
        .json(&serde_json::json!({ "name": "../Globex" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unsafe_name.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(server.root.join("Customers/Acme/Acme.md").exists());

    server.server.abort();
}

#[tokio::test]
async fn save_pipeline_stamps_created_updated() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let create_response = client
        .post(server.url("/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "Stamped",
            "content": "Body",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(create_response.status(), reqwest::StatusCode::CREATED);

    let get_response = client
        .get(server.url("/api/v/test-vault/notes/Inbox/Stamped.md"))
        .send()
        .await
        .unwrap();
    let body = get_response.json::<serde_json::Value>().await.unwrap();
    let raw_frontmatter = body["raw_frontmatter"].as_str().unwrap();
    assert!(raw_frontmatter.contains("created: "));
    assert!(raw_frontmatter.contains("updated: "));

    server.server.abort();
}

// ── Task API tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tasks_returns_tasks_from_golden_vault() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/tasks"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert!(body.len() >= 10, "expected ≥10 tasks, got {}", body.len());
    // Every task has the expected fields
    for task in &body {
        assert!(task["task_hash"].as_str().is_some());
        assert!(task["note_path"].as_str().is_some());
        assert!(task["status"].as_str().is_some());
        assert!(task["text"].as_str().is_some());
    }

    server.abort();
}

#[tokio::test]
async fn list_tasks_filters_by_status() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let response = reqwest::get(format!(
        "http://{address}/api/v/test-vault/tasks?status=done"
    ))
    .await
    .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert!(!body.is_empty(), "expected at least one done task");
    for task in &body {
        assert_eq!(task["status"], serde_json::json!("done"));
    }

    server.abort();
}

#[tokio::test]
async fn create_task_appends_to_note() {
    let note_content = "# My Note\n\nSome content here.\n";
    let server = TestServer::with_files(&[("Inbox/My Note.md", note_content)]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/tasks"))
        .json(&serde_json::json!({
            "note_path": "Inbox/My Note.md",
            "description": "Write tests for task engine",
            "status_char": "/",
            "fields": {
                "customer": "Acme",
                "priority": "high",
            },
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/My Note.md"));

    let written = fs::read_to_string(server.root.join("Inbox/My Note.md")).unwrap();
    assert!(
        written.contains("- [/] Write tests for task engine [customer:: Acme] [priority:: high]")
    );

    server.server.abort();
}

#[tokio::test]
async fn list_tasks_filters_by_field_and_returns_generic_fields() {
    let server = TestServer::with_files(&[(
        "Inbox/Tasks.md",
        "# Tasks\n\n- [ ] Follow up [customer:: Acme] [stream:: Migration]\n- [ ] Review roadmap [customer:: Globex]\n",
    )])
    .await;

    let response = reqwest::get(server.url("/api/v/test-vault/tasks?field=customer%3DAcme"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["text"], serde_json::json!("Follow up"));
    assert_eq!(body[0]["fields"]["customer"], serde_json::json!("Acme"));
    assert_eq!(body[0]["fields"]["stream"], serde_json::json!("Migration"));

    server.server.abort();
}

#[tokio::test]
async fn toggle_task_status_rewrites_task_line() {
    let task_line = "- [ ] Fix the bug";
    let note_content = format!("# Tasks\n\n{task_line}\n");
    let server = TestServer::with_files(&[("Inbox/Tasks.md", &note_content)]).await;
    let client = reqwest::Client::new();

    // Compute hash using the same blake3 logic as the parser
    let task_hash = notesmith_tasks::task_content_hash(task_line);

    let response = client
        .post(server.url("/api/v/test-vault/tasks/toggle"))
        .json(&serde_json::json!({
            "note_path": "Inbox/Tasks.md",
            "task_hash": task_hash,
            "new_status": "in_progress",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let written = fs::read_to_string(server.root.join("Inbox/Tasks.md")).unwrap();
    assert!(
        written.contains("- [/] Fix the bug"),
        "expected [/] in:\n{written}"
    );

    server.server.abort();
}

#[tokio::test]
async fn toggle_task_accepts_status_alias() {
    let task_line = "- [ ] Fix the bug";
    let note_content = format!("# Tasks\n\n{task_line}\n");
    let server = TestServer::with_files(&[("Inbox/Tasks.md", &note_content)]).await;
    let client = reqwest::Client::new();

    let task_hash = notesmith_tasks::task_content_hash(task_line);

    let response = client
        .post(server.url("/api/v/test-vault/tasks/toggle"))
        .json(&serde_json::json!({
            "note_path": "Inbox/Tasks.md",
            "task_hash": task_hash,
            "status": "in_progress",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let written = fs::read_to_string(server.root.join("Inbox/Tasks.md")).unwrap();
    assert!(
        written.contains("- [/] Fix the bug"),
        "expected [/] in:\n{written}"
    );

    server.server.abort();
}

#[tokio::test]
async fn toggle_task_returns_not_found_for_bad_hash() {
    let server = TestServer::with_files(&[("Inbox/Tasks.md", "- [ ] Some task\n")]).await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/tasks/toggle"))
        .json(&serde_json::json!({
            "note_path": "Inbox/Tasks.md",
            "task_hash": "deadbeefdeadbeef",
            "new_status": "done",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}

#[tokio::test]
async fn toggle_task_allows_rewriting_to_any_status_char() {
    let task_line = "- [/] In progress task";
    let note_content = format!("{task_line}\n");
    let server = TestServer::with_files(&[("Inbox/Tasks.md", &note_content)]).await;
    let client = reqwest::Client::new();

    let task_hash = notesmith_tasks::task_content_hash(task_line);

    let response = client
        .post(server.url("/api/v/test-vault/tasks/toggle"))
        .json(&serde_json::json!({
            "note_path": "Inbox/Tasks.md",
            "task_hash": task_hash,
            "new_status": "todo",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let written = fs::read_to_string(server.root.join("Inbox/Tasks.md")).unwrap();
    assert!(written.contains("- [ ] In progress task"));

    server.server.abort();
}

// ── Capture API tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn post_capture_creates_note_with_timestamp_filename() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/capture"))
        .json(&serde_json::json!({
            "text": "Buy milk and eggs",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    let path = body["path"].as_str().unwrap();
    assert!(
        path.starts_with("Inbox/"),
        "path should start with Inbox/: {path}"
    );
    assert!(path.ends_with(".md"), "path should end with .md: {path}");
    assert!(body["hash"].as_str().unwrap().len() > 10);

    // Verify file exists on disk with expected content
    let file_path = server.root.join(path);
    assert!(
        file_path.exists(),
        "file should exist at {}",
        file_path.display()
    );
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("Buy milk and eggs"));

    server.server.abort();
}

#[tokio::test]
async fn post_capture_with_title_uses_title_in_filename() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/capture"))
        .json(&serde_json::json!({
            "text": "Some detailed content here",
            "title": "Grocery List",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    let path = body["path"].as_str().unwrap();
    assert!(
        path.contains("Grocery List"),
        "path should contain title slug: {path}"
    );
    assert!(path.starts_with("Inbox/"));

    let file_path = server.root.join(path);
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("Some detailed content here"));

    server.server.abort();
}

#[tokio::test]
async fn get_inbox_returns_404() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/api/v/test-vault/inbox"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}

// ── Template API tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn get_templates_lists_all() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let response = reqwest::get(format!("http://{address}/api/v/test-vault/templates"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert_eq!(body.len(), 9, "expected 9 templates, got {}", body.len());
    let names: Vec<&str> = body.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"generic-note"));
    assert!(names.contains(&"daily-note"));
    assert!(names.contains(&"stream"));
    for template in &body {
        assert!(template["prompts"].as_array().is_some());
        assert!(template["description"].as_str().is_some());
    }

    server.abort();
}

#[tokio::test]
async fn post_render_template_returns_content() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "http://{address}/api/v/test-vault/templates/generic-note/render"
        ))
        .json(&serde_json::json!({
            "prompts": { "title": "Test Note" }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/test-note.md"));
    assert!(body["content"].as_str().unwrap().contains("# Test Note"));

    server.abort();
}

#[tokio::test]
async fn post_render_missing_prompt_returns_422() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "http://{address}/api/v/test-vault/templates/generic-note/render"
        ))
        .json(&serde_json::json!({ "prompts": {} }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert!(
        body["missing"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("title"))
    );

    server.abort();
}

#[tokio::test]
async fn post_render_unknown_template_returns_404() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = golden_vault();

    let server = tokio::spawn(async move {
        serve_with_listener(listener, build_test_state(&root))
            .await
            .unwrap();
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "http://{address}/api/v/test-vault/templates/nonexistent/render"
        ))
        .json(&serde_json::json!({ "prompts": {} }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.abort();
}

#[tokio::test]
async fn post_instantiate_creates_note() {
    let server = TestServer::empty().await;
    let templates_src = golden_vault().join("Assets").join("templates");
    let templates_dst = server.root.join("Assets").join("templates");
    fs::create_dir_all(&templates_dst).unwrap();
    for entry in fs::read_dir(&templates_src).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), templates_dst.join(entry.file_name())).unwrap();
    }

    let client = reqwest::Client::new();
    let response = client
        .post(server.url("/api/v/test-vault/templates/generic-note/instantiate"))
        .json(&serde_json::json!({
            "prompts": { "title": "Created Note" }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/created-note.md"));
    assert!(server.root.join("Inbox/created-note.md").exists());

    let content = fs::read_to_string(server.root.join("Inbox/created-note.md")).unwrap();
    assert!(content.contains("# Created Note"));

    server.server.abort();
}

#[tokio::test]
async fn post_instantiate_missing_prompt_returns_422() {
    let server = TestServer::empty().await;
    let templates_src = golden_vault().join("Assets").join("templates");
    let templates_dst = server.root.join("Assets").join("templates");
    fs::create_dir_all(&templates_dst).unwrap();
    for entry in fs::read_dir(&templates_src).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), templates_dst.join(entry.file_name())).unwrap();
    }

    let client = reqwest::Client::new();
    let response = client
        .post(server.url("/api/v/test-vault/templates/generic-note/instantiate"))
        .json(&serde_json::json!({ "prompts": {} }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    server.server.abort();
}

// ── Routing integration tests ─────────────────────────────────────────────────

fn write_routing_config(root: &Path) {
    let config = r#"version: 1
defaults:
  on_exists: skip
rules:
  - id: external-meeting
    when:
      all:
        - field.type: meeting
        - field.meeting-kind: external
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/External Meetings/{{ filename }}"
      set_fields:
        status: filed
      remove_tags: [inbox]
  - id: note-customer
    when:
      all:
        - field.type: note
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/{{ filename }}"
  - id: note-general
    when:
      field.type: note
    then:
      move_to: "General/{{ filename }}"
"#;
    write_note(root, ".notesmith/routing.yaml", config);
}

#[tokio::test]
async fn route_preview_shows_destination() {
    let server = TestServer::with_files(&[(
        "Inbox/standup.md",
        "---\ntype: meeting\nmeeting-kind: external\ncustomer: \"[[Acme Corp]]\"\ndate: 2025-03-15\n---\n# Standup\n",
    )])
    .await;
    write_routing_config(&server.root);

    let client = reqwest::Client::new();
    let response = client
        .post(server.url("/api/v/test-vault/route/preview"))
        .json(&serde_json::json!({ "path": "Inbox/standup.md" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["rule_id"], "external-meeting");
    assert_eq!(
        body["destination"],
        "Customers/Acme Corp/External Meetings/standup.md"
    );

    server.server.abort();
}

#[tokio::test]
async fn route_apply_moves_note_and_stamps_archive() {
    let server =
        TestServer::with_files(&[("Inbox/idea.md", "---\ntype: note\n---\n# My Idea\n")]).await;
    write_routing_config(&server.root);

    let client = reqwest::Client::new();
    let response = client
        .post(server.url("/api/v/test-vault/route/apply"))
        .json(&serde_json::json!({ "paths": ["Inbox/idea.md"] }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["routed"], 1);

    let results = body["results"].as_array().unwrap();
    assert_eq!(results[0]["from"], "Inbox/idea.md");
    assert_eq!(results[0]["to"], "General/idea.md");
    assert_eq!(results[0]["rule_id"], "note-general");

    // Verify file was moved and stamped
    assert!(!server.root.join("Inbox/idea.md").exists());
    let content = fs::read_to_string(server.root.join("General/idea.md")).unwrap();
    assert!(content.contains("archived: true"));
    assert!(content.contains("archived-at:"));
    assert!(content.contains("# My Idea"));

    server.server.abort();
}

#[tokio::test]
async fn route_preview_returns_conflict_for_archived_note() {
    let server = TestServer::with_files(&[(
        "Inbox/old.md",
        "---\ntype: note\narchived: true\n---\n# Old\n",
    )])
    .await;
    write_routing_config(&server.root);

    let client = reqwest::Client::new();
    let response = client
        .post(server.url("/api/v/test-vault/route/preview"))
        .json(&serde_json::json!({ "path": "Inbox/old.md" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    server.server.abort();
}

// ── Daily integration tests ──────────────────────────────────────────────────

fn copy_templates(server: &TestServer) {
    let templates_src = golden_vault().join("Assets").join("templates");
    let templates_dst = server.root.join("Assets").join("templates");
    fs::create_dir_all(&templates_dst).unwrap();
    for entry in fs::read_dir(&templates_src).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), templates_dst.join(entry.file_name())).unwrap();
    }
}

#[tokio::test]
async fn post_daily_creates_note() {
    let server = TestServer::empty().await;
    copy_templates(&server);
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/daily/2025-01-15"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/Daily/2025-01-15.md"));
    assert_eq!(body["created"], serde_json::json!(true));
    assert!(server.root.join("Inbox/Daily/2025-01-15.md").exists());

    let content = fs::read_to_string(server.root.join("Inbox/Daily/2025-01-15.md")).unwrap();
    assert!(content.contains("# 2025-01-15"));
    assert!(content.contains("date: 2025-01-15"));

    server.server.abort();
}

#[tokio::test]
async fn post_daily_idempotent() {
    let server = TestServer::empty().await;
    copy_templates(&server);
    let client = reqwest::Client::new();

    let first = client
        .post(server.url("/api/v/test-vault/daily/2025-03-20"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), reqwest::StatusCode::CREATED);

    let second = client
        .post(server.url("/api/v/test-vault/daily/2025-03-20"))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), reqwest::StatusCode::OK);
    let body = second.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["created"], serde_json::json!(false));

    server.server.abort();
}

#[tokio::test]
async fn get_daily_returns_content() {
    let server = TestServer::empty().await;
    copy_templates(&server);
    let client = reqwest::Client::new();

    // Create the note first
    client
        .post(server.url("/api/v/test-vault/daily/2025-02-10"))
        .send()
        .await
        .unwrap();

    let response = client
        .get(server.url("/api/v/test-vault/daily/2025-02-10"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/Daily/2025-02-10.md"));
    assert!(body["content"].as_str().unwrap().contains("# 2025-02-10"));

    server.server.abort();
}

#[tokio::test]
async fn post_daily_invalid_date_returns_400() {
    let server = TestServer::empty().await;
    copy_templates(&server);
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/daily/not-a-date"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    server.server.abort();
}

#[tokio::test]
async fn agent_create_daily_prompt_mode() {
    let recent_date = (chrono::Local::now().date_naive() - chrono::Days::new(1))
        .format("%Y-%m-%d")
        .to_string();
    let prompt_template = r#"---
context_queries:
  - name: open_tasks
    sql: "SELECT t.text, due.value AS due, customer.value AS customer, t.note_path FROM v_tasks t LEFT JOIN v_task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due' LEFT JOIN v_task_fields customer ON customer.vault_name = t.vault_name AND customer.task_id = t.id AND customer.key = 'customer' WHERE t.status_group = 'open' ORDER BY due.value IS NULL, due.value ASC LIMIT 20"
  - name: recent_meetings
    sql: "SELECT n.title, customer.value AS customer, date.value AS date FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' LEFT JOIN v_fields customer ON customer.vault_name = n.vault_name AND customer.note_path = n.path AND customer.key = 'customer' LEFT JOIN v_fields date ON date.vault_name = n.vault_name AND date.note_path = n.path AND date.key = 'date' WHERE note_type.value = 'meeting' AND date.value >= date('now', '-7 days') ORDER BY date.value DESC LIMIT 10"
  - name: inbox_count
    sql: "SELECT COUNT(*) as count FROM notes WHERE path LIKE 'Inbox/%'"
---

# Daily Note Prompt

Today's date: {{ today }}

## Open Tasks
{{ open_tasks }}

## Recent Meetings
{{ recent_meetings }}

## Inbox Status
{{ inbox_count }}
"#;
    let meeting_note = format!(
        "---\ntype: meeting\ncustomer: Acme\nmeeting-kind: external\ndate: {recent_date}\nattendees:\n  - Jane Doe\nstream: Migration\n---\n# Recent Meeting\n\nReviewed follow-ups.\n"
    );
    let server = TestServer::with_files(&[
        (".notesmith/prompts/daily-note.md", prompt_template),
        (
            "Inbox/Tasks.md",
            "---\ntype: note\n---\n- [ ] Follow up with customer [customer:: Acme] 📅 2026-05-10\n",
        ),
        ("Customers/Acme/Recent Meeting.md", &meeting_note),
    ])
    .await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/daily/agent-create"))
        .json(&serde_json::json!({ "date": "2026-05-10" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<serde_json::Value>().await.unwrap();
    let prompt = body["prompt"].as_str().unwrap();
    assert_eq!(body["date"], serde_json::json!("2026-05-10"));
    assert!(prompt.contains("Today's date: 2026-05-10"));
    assert!(prompt.contains("| text | due | customer | note_path |"));
    assert!(prompt.contains("Follow up with customer"));
    assert!(prompt.contains("| title | customer | date |"));
    assert!(prompt.contains("Recent Meeting"));
    assert!(prompt.contains("| count |"));

    server.server.abort();
}

#[tokio::test]
async fn agent_create_daily_write_mode() {
    let server = TestServer::with_files(&[]).await;
    let client = reqwest::Client::new();
    let content = "---\ntype: daily\ndate: 2026-05-10\n---\n# 2026-05-10\n\nGenerated by agent.\n";

    let response = client
        .post(server.url("/api/v/test-vault/daily/agent-create"))
        .json(&serde_json::json!({
            "date": "2026-05-10",
            "content": content,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/Daily/2026-05-10.md"));
    assert_eq!(body["created"], serde_json::json!(true));
    assert_eq!(
        fs::read_to_string(server.root.join("Inbox/Daily/2026-05-10.md")).unwrap(),
        content
    );

    server.server.abort();
}

#[tokio::test]
async fn agent_create_daily_write_conflict() {
    let server = TestServer::with_files(&[]).await;
    let client = reqwest::Client::new();

    let first = client
        .post(server.url("/api/v/test-vault/daily/agent-create"))
        .json(&serde_json::json!({
            "date": "2026-05-10",
            "content": "---\ntype: daily\ndate: 2026-05-10\n---\n# First\n",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), reqwest::StatusCode::CREATED);

    let second = client
        .post(server.url("/api/v/test-vault/daily/agent-create"))
        .json(&serde_json::json!({
            "date": "2026-05-10",
            "content": "---\ntype: daily\ndate: 2026-05-10\n---\n# Second\n",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(second.status(), reqwest::StatusCode::CONFLICT);

    server.server.abort();
}

#[tokio::test]
async fn get_daily_missing_returns_404() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/api/v/test-vault/daily/2099-01-01"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}

#[tokio::test]
async fn get_current_periodic_note_creates_weekly_note() {
    let server = TestServer::with_config_and_files(
        r#"name = "test-vault"

[capture]
folder = "Inbox"

[periodic.weekly]
folder = "Weekly"
template = "weekly"
filename = "Week {{ week }}"
"#,
        &[(
            "Assets/templates/weekly.md.j2",
            r#"---
notesmith:
  name: weekly
  description: Weekly note
  output_path: "ignored/{{ week }}.md"
---
# {{ period_key }}
{{ period_start }} → {{ period_end }}
"#,
        )],
    )
    .await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/api/v/test-vault/periodic/weekly/current?offset=-1"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    let target_date = chrono::Local::now().date_naive() - chrono::Duration::weeks(1);
    let week_key = target_date.format("%G-W%V").to_string();
    assert_eq!(body["created"], serde_json::json!(true));
    assert_eq!(body["period_kind"], serde_json::json!("weekly"));
    assert_eq!(body["period_key"], serde_json::json!(week_key.clone()));
    assert_eq!(
        body["path"],
        serde_json::json!(format!("Weekly/Week {week_key}.md"))
    );
    assert!(
        server
            .root
            .join(format!("Weekly/Week {week_key}.md"))
            .exists()
    );

    server.server.abort();
}

#[tokio::test]
async fn list_periodic_notes_filters_range() {
    let server = TestServer::with_config_and_files(
        r#"name = "test-vault"

[capture]
folder = "Inbox"

[periodic.weekly]
folder = "Weekly"
template = "weekly"
filename = "Week {{ week }}"
"#,
        &[
            ("Weekly/Week 2026-W21.md", "# 2026-W21\n"),
            ("Weekly/Week 2026-W22.md", "# 2026-W22\n"),
            ("Weekly/Week 2026-W30.md", "# 2026-W30\n"),
        ],
    )
    .await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/api/v/test-vault/periodic/weekly/list?from=2026-05-18&to=2026-05-31"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!([
            {
                "path": "Weekly/Week 2026-W21.md",
                "period_kind": "weekly",
                "period_key": "2026-W21",
                "period_start": "2026-05-18",
                "period_end": "2026-05-24"
            },
            {
                "path": "Weekly/Week 2026-W22.md",
                "period_kind": "weekly",
                "period_key": "2026-W22",
                "period_start": "2026-05-25",
                "period_end": "2026-05-31"
            }
        ])
    );

    server.server.abort();
}

// ── SSE event stream tests ──────────────────────────────────────────────────

#[tokio::test]
async fn sse_vault_not_found_returns_404() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/api/v/nonexistent/events"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    server.server.abort();
}

#[tokio::test]
async fn sse_receives_note_created_event() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();
    let sse_url = server.url("/api/v/test-vault/events");

    // Collect SSE chunks in a background task
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);
    let sse_task = tokio::spawn(async move {
        use futures::StreamExt;
        let response = client.get(&sse_url).send().await.unwrap();
        let mut stream = response.bytes_stream();
        while let Some(Ok(chunk)) = stream.next().await {
            let text = String::from_utf8_lossy(&chunk).to_string();
            let _ = tx.send(text).await;
        }
    });

    // Give SSE connection time to establish
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Create a note via API
    let client2 = reqwest::Client::new();
    let resp = client2
        .post(server.url("/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "SSE Test Note",
            "content": "Testing SSE events"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);

    // Wait for event
    let event_text = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout waiting for SSE event")
        .unwrap();

    assert!(
        event_text.contains("note.created"),
        "Expected note.created event, got: {event_text}"
    );
    assert!(
        event_text.contains("SSE Test Note"),
        "Expected note path in event, got: {event_text}"
    );

    sse_task.abort();
    server.server.abort();
}

#[tokio::test]
async fn sse_note_updated_event_includes_hash_matching_put_response() {
    let server = TestServer::empty().await;

    // Seed an existing note via API.
    let client = reqwest::Client::new();
    let create_resp = client
        .post(server.url("/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "Hash Echo Test",
            "content": "initial body"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), reqwest::StatusCode::CREATED);
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let note_path = created["path"].as_str().unwrap().to_string();

    // Connect SSE *after* the create so we only see the upcoming update.
    let sse_url = server.url("/api/v/test-vault/events");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);
    let sse_client = reqwest::Client::new();
    let sse_task = tokio::spawn(async move {
        use futures::StreamExt;
        let response = sse_client.get(&sse_url).send().await.unwrap();
        let mut stream = response.bytes_stream();
        while let Some(Ok(chunk)) = stream.next().await {
            let text = String::from_utf8_lossy(&chunk).to_string();
            let _ = tx.send(text).await;
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let put_resp = client
        .put(server.url(&format!("/api/v/test-vault/notes/{note_path}")))
        .json(&serde_json::json!({
            "content": "updated body"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);
    let put_body: serde_json::Value = put_resp.json().await.unwrap();
    let response_hash = put_body["hash"].as_str().unwrap().to_string();
    assert!(
        !response_hash.is_empty(),
        "PUT response must include a hash"
    );

    // Collect SSE chunks until we see a note.updated payload.
    let mut buffered = String::new();
    let event_payload = loop {
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
            .await
            .expect("timeout waiting for note.updated SSE event")
            .unwrap();
        buffered.push_str(&chunk);
        if let Some(payload) = extract_note_updated_payload(&buffered) {
            break payload;
        }
    };

    let parsed: serde_json::Value = serde_json::from_str(&event_payload)
        .unwrap_or_else(|err| panic!("SSE payload was not valid JSON ({err}): {event_payload}"));
    assert_eq!(parsed["type"], "note.updated");
    assert_eq!(parsed["path"], note_path);
    assert_eq!(
        parsed["hash"].as_str().unwrap(),
        response_hash,
        "event hash must match the PUT response hash to enable client-side dedup"
    );

    sse_task.abort();
    server.server.abort();
}

/// Scan a buffered SSE chunk stream for the first `data: { ... }` payload whose
/// event type is `note.updated`. Returns the JSON-encoded payload, if any.
fn extract_note_updated_payload(buffer: &str) -> Option<String> {
    for line in buffer.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if !payload.starts_with('{') {
            continue;
        }
        if payload.contains("\"type\":\"note.updated\"") {
            return Some(payload.to_string());
        }
    }
    None
}

#[tokio::test]
async fn sse_filters_events_by_vault() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();
    let sse_url = server.url("/api/v/test-vault/events");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);
    let sse_task = tokio::spawn(async move {
        use futures::StreamExt;
        let response = client.get(&sse_url).send().await.unwrap();
        let mut stream = response.bytes_stream();
        while let Some(Ok(chunk)) = stream.next().await {
            let text = String::from_utf8_lossy(&chunk).to_string();
            let _ = tx.send(text).await;
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Create a note in test-vault and confirm we get the event
    let client2 = reqwest::Client::new();
    let resp = client2
        .post(server.url("/api/v/test-vault/notes"))
        .json(&serde_json::json!({
            "title": "Vault Filter Test",
            "content": "Should arrive"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);

    let event_text = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout waiting for SSE event")
        .unwrap();

    assert!(event_text.contains("test-vault"));

    sse_task.abort();
    server.server.abort();
}

// ── Capabilities API tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn get_capabilities_returns_expected_fields() {
    let server = TestServer::empty().await;

    let response = reqwest::get(server.url("/api/capabilities")).await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["deployment_mode"], "desktop");
    assert_eq!(body["can_edit_global_config"], true);
    assert_eq!(body["can_edit_vault_config"], true);
    assert_eq!(body["can_open_local_paths"], true);
    assert!(body["restart_required_fields"].as_array().unwrap().len() >= 1);

    server.server.abort();
}

// ── Vault config API tests ─────────────────────────────────────────────────────

fn write_vault_config(root: &Path, toml_content: &str) {
    let config_dir = root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("vault.toml"), toml_content).unwrap();
}

fn write_sidebar_config(root: &Path, yaml_content: &str) {
    let config_dir = root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("sidebar.yaml"), yaml_content).unwrap();
}

#[tokio::test]
async fn get_vault_config_returns_config_with_etag() {
    let server = TestServer::empty().await;
    let toml_content = "name = \"test-vault\"\n";
    write_vault_config(&server.root, toml_content);

    let response = reqwest::get(server.url("/api/v/test-vault/config"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let etag = response
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(etag.starts_with('"') && etag.ends_with('"'));

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["config"]["name"], "test-vault");
    assert_eq!(body["config"]["editor"]["strict_line_breaks"], false);
    assert_eq!(body["config"]["editor"]["show_line_numbers"], true);
    assert_eq!(body["config"]["editor"]["hide_duplicate_h1"], true);
    assert_eq!(body["config"]["editor"]["paste_url_image_whitelist"], "");
    assert!(body["hash"].as_str().unwrap().len() > 10);
    assert_eq!(body["path"], ".notesmith/vault.toml");
    assert!(body["warnings"].is_object());

    server.server.abort();
}

#[tokio::test]
async fn get_vault_config_returns_404_for_unknown_vault() {
    let server = TestServer::empty().await;

    let response = reqwest::get(server.url("/api/v/nonexistent/config"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}

#[tokio::test]
async fn put_vault_config_succeeds_with_correct_if_match() {
    let server = TestServer::empty().await;
    let toml_content = "name = \"test-vault\"\n";
    write_vault_config(&server.root, toml_content);

    // First GET to obtain the hash
    let get_response = reqwest::get(server.url("/api/v/test-vault/config"))
        .await
        .unwrap();
    let get_body = get_response.json::<serde_json::Value>().await.unwrap();
    let hash = get_body["hash"].as_str().unwrap();

    // PUT with correct If-Match
    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "name": "test-vault",
        "capture": { "folder": "MyInbox", "template": "generic-note" },
        "daily": { "folder": "Inbox/Daily", "template": "daily-note", "catch_up": false },
        "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
        "git": { "enabled": false },
        "hooks": {}
    });

    let response = client
        .put(server.url("/api/v/test-vault/config"))
        .header("if-match", format!("\"{hash}\""))
        .json(&new_config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let etag = response
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(etag.starts_with('"') && etag.ends_with('"'));

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["config"]["capture"]["folder"], "MyInbox");
    assert!(body["hash"].as_str().unwrap().len() > 10);

    server.server.abort();
}

#[tokio::test]
async fn put_vault_config_returns_409_on_stale_if_match() {
    let server = TestServer::empty().await;
    let toml_content = "name = \"test-vault\"\n";
    write_vault_config(&server.root, toml_content);

    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "name": "test-vault",
        "capture": { "folder": "Inbox", "template": "generic-note" },
        "daily": { "folder": "Inbox/Daily", "template": "daily-note", "catch_up": false },
        "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
        "git": { "enabled": false },
        "hooks": {}
    });

    let response = client
        .put(server.url("/api/v/test-vault/config"))
        .header("if-match", "\"stale-hash-value\"")
        .json(&new_config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "conflict");
    assert!(body["config"].is_object());
    assert!(body["hash"].as_str().is_some());

    server.server.abort();
}

#[tokio::test]
async fn put_vault_config_returns_428_without_if_match() {
    let server = TestServer::empty().await;
    write_vault_config(&server.root, "name = \"test-vault\"\n");

    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "name": "test-vault",
        "capture": { "folder": "Inbox", "template": "generic-note" },
        "daily": { "folder": "Inbox/Daily", "template": "daily-note", "catch_up": false },
        "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
        "git": { "enabled": false },
        "hooks": {}
    });

    let response = client
        .put(server.url("/api/v/test-vault/config"))
        .json(&new_config)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        reqwest::StatusCode::PRECONDITION_REQUIRED
    );

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "if_match_required");

    server.server.abort();
}

#[tokio::test]
async fn put_vault_config_returns_422_with_invalid_data() {
    let server = TestServer::empty().await;
    let toml_content = "name = \"test-vault\"\n";
    write_vault_config(&server.root, toml_content);

    let get_response = reqwest::get(server.url("/api/v/test-vault/config"))
        .await
        .unwrap();
    let get_body = get_response.json::<serde_json::Value>().await.unwrap();
    let hash = get_body["hash"].as_str().unwrap();

    let client = reqwest::Client::new();
    let bad_config = serde_json::json!({
        "name": "test-vault",
        "capture": { "folder": "Inbox", "template": "generic-note" },
        "daily": {
            "folder": "Inbox/Daily",
            "template": "daily-note",
            "generate_at": "25:99",
            "timezone": "Mars/Olympus",
            "catch_up": false
        },
        "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
        "git": { "enabled": false, "auto_commit_every": "banana" },
        "hooks": {}
    });

    let response = client
        .put(server.url("/api/v/test-vault/config"))
        .header("if-match", format!("\"{hash}\""))
        .json(&bad_config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "validation_failed");
    let errors = body["errors"].as_object().unwrap();
    assert!(errors.contains_key("daily.generate_at"));
    assert!(errors.contains_key("daily.timezone"));
    assert!(errors.contains_key("git.auto_commit_every"));

    server.server.abort();
}

#[tokio::test]
async fn put_vault_config_rejects_disallowed_origin() {
    let server = TestServer::empty().await;
    write_vault_config(&server.root, "name = \"test-vault\"\n");

    let client = reqwest::Client::new();
    let config = serde_json::json!({
        "name": "test-vault",
        "capture": { "folder": "Inbox", "template": "generic-note" },
        "daily": { "folder": "Inbox/Daily", "template": "daily-note", "catch_up": false },
        "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
        "git": { "enabled": false },
        "hooks": {}
    });

    let response = client
        .put(server.url("/api/v/test-vault/config"))
        .header("origin", "https://evil.example.com")
        .header("if-match", "\"somehash\"")
        .json(&config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["error"], "origin_not_allowed");

    server.server.abort();
}

#[tokio::test]
async fn get_after_put_reflects_changes() {
    let server = TestServer::empty().await;
    write_vault_config(&server.root, "name = \"test-vault\"\n");

    // GET to obtain hash
    let get_response = reqwest::get(server.url("/api/v/test-vault/config"))
        .await
        .unwrap();
    let get_body = get_response.json::<serde_json::Value>().await.unwrap();
    let hash = get_body["hash"].as_str().unwrap();

    // PUT with new capture folder
    let client = reqwest::Client::new();
    let new_config = serde_json::json!({
        "name": "test-vault",
        "capture": { "folder": "CustomInbox", "template": "generic-note" },
        "daily": { "folder": "Inbox/Daily", "template": "daily-note", "catch_up": false },
        "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
        "git": { "enabled": false },
        "hooks": {}
    });

    let put_response = client
        .put(server.url("/api/v/test-vault/config"))
        .header("if-match", format!("\"{hash}\""))
        .json(&new_config)
        .send()
        .await
        .unwrap();
    assert_eq!(put_response.status(), reqwest::StatusCode::OK);

    // GET again — should reflect changes
    let get_response2 = reqwest::get(server.url("/api/v/test-vault/config"))
        .await
        .unwrap();
    assert_eq!(get_response2.status(), reqwest::StatusCode::OK);

    let body2 = get_response2.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body2["config"]["capture"]["folder"], "CustomInbox");

    server.server.abort();
}

#[tokio::test]
async fn get_fields_returns_registry_json() {
    let server = TestServer::with_files(&[
        (
            ".notesmith/fields.toml",
            r#"
version = 1

[fields.status]
type = "enum"
description = "Customer status"
values = ["active", "paused", "closed"]

[fields.customer]
type = "string"
suggest_from = "SELECT DISTINCT value FROM v_fields WHERE key = 'customer' ORDER BY value"
"#,
        ),
        (
            "Customers/Acme.md",
            "---\ntype: customer\ncustomer: Acme\nstatus: active\n---\nAcme\n",
        ),
        (
            "Customers/Globex.md",
            "---\ntype: customer\ncustomer: Globex\nstatus: paused\n---\nGlobex\n",
        ),
    ])
    .await;

    let response = reqwest::get(server.url("/api/v/test-vault/fields"))
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["version"], 1);
    assert_eq!(body["fields"]["status"]["type"], "enum");
    assert_eq!(body["fields"]["status"]["description"], "Customer status");
    assert_eq!(
        body["fields"]["status"]["values"],
        serde_json::json!(["active", "paused", "closed"])
    );
    assert_eq!(
        body["fields"]["customer"]["suggest_from"],
        "SELECT DISTINCT value FROM v_fields WHERE key = 'customer' ORDER BY value"
    );

    server.server.abort();
}

#[tokio::test]
async fn suggest_field_values_uses_registry_values_and_cache_queries() {
    let server = TestServer::with_files(&[
        (
            ".notesmith/fields.toml",
            r#"
version = 1

[fields.status]
type = "enum"
values = ["active", "paused", "closed"]

[fields.customer]
type = "string"
suggest_from = "SELECT DISTINCT value FROM v_fields WHERE key = 'customer' ORDER BY value"
"#,
        ),
        (
            "Customers/Acme.md",
            "---\ntype: customer\ncustomer: Acme\nstatus: active\n---\nAcme\n",
        ),
        (
            "Customers/Globex.md",
            "---\ntype: customer\ncustomer: Globex\nstatus: paused\n---\nGlobex\n",
        ),
    ])
    .await;

    let status_values = reqwest::get(server.url("/api/v/test-vault/fields/status/suggest?q=pa"))
        .await
        .unwrap();
    assert_eq!(status_values.status(), reqwest::StatusCode::OK);
    assert_eq!(
        status_values.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!(["paused"])
    );

    let customer_values = reqwest::get(server.url("/api/v/test-vault/fields/customer/suggest?q=A"))
        .await
        .unwrap();
    assert_eq!(customer_values.status(), reqwest::StatusCode::OK);
    assert_eq!(
        customer_values.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!(["Acme"])
    );

    server.server.abort();
}

fn mcp_initialize_body() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "notesmith-test", "version": "0" }
        }
    })
}

async fn mcp_initialize(client: &reqwest::Client, url: String) -> reqwest::Response {
    client
        .post(url)
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&mcp_initialize_body())
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn daemon_mounts_read_write_mcp_endpoint_per_vault() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = mcp_initialize(&client, server.url("/mcp/test-vault")).await;

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(
        response.headers().contains_key("mcp-session-id"),
        "read-write MCP endpoint should establish an MCP session"
    );

    server.server.abort();
}

#[tokio::test]
async fn daemon_mounts_read_only_mcp_endpoint_per_vault() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = mcp_initialize(&client, server.url("/mcp-ro/test-vault")).await;

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(
        response.headers().contains_key("mcp-session-id"),
        "read-only MCP endpoint should establish an MCP session"
    );

    server.server.abort();
}

#[tokio::test]
async fn daemon_does_not_mount_mcp_for_unknown_vault() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = mcp_initialize(&client, server.url("/mcp/does-not-exist")).await;

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.server.abort();
}
