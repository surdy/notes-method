use std::{collections::HashMap, fs, net::SocketAddr, path::Path, path::PathBuf};

use notesmith_core::VaultEngine;
use notesmith_http::{AppState, VaultState, serve_with_listener};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use tempfile::TempDir;

fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn build_test_state(root: &Path) -> AppState {
    let engine = NativeVaultEngine;
    let notes = engine.scan(root).unwrap();
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
                root: root.to_path_buf(),
            },
        )]),
    }
}

fn write_note(root: &Path, relative_path: &str, content: &str) {
    let absolute_path = root.join(relative_path);
    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(absolute_path, content).unwrap();
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
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("vault");
        fs::create_dir_all(&root).unwrap();
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

    server.abort();
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
    assert!(body["tasks"].as_array().unwrap().len() >= 5);
    assert!(body["links"].as_array().unwrap().len() >= 3);
    assert!(body["inline_fields"].as_array().unwrap().len() >= 2);

    server.abort();
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
