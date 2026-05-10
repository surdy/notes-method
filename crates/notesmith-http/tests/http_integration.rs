use std::{collections::HashMap, fs, net::SocketAddr, path::Path, path::PathBuf};

use notesmith_config::VaultConfig;
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

    let vault_config = VaultConfig::load_from_vault(root).unwrap_or_else(|_| VaultConfig {
        name: "test-vault".to_string(),
        inbox: Default::default(),
        daily: Default::default(),
        editor: Default::default(),
        git: Default::default(),
        hooks: Default::default(),
        homepage: None,
    });

    let template_engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), None);

    AppState {
        vaults: HashMap::from([(
            "test-vault".to_string(),
            VaultState {
                cache,
                search_index,
                engine,
                root: root.to_path_buf(),
                vault_config,
                template_engine,
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
            "customer": "Acme",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["path"], serde_json::json!("Inbox/My Note.md"));

    let written = fs::read_to_string(server.root.join("Inbox/My Note.md")).unwrap();
    assert!(written.contains("- [ ] Write tests for task engine [customer:: Acme]"));

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
async fn toggle_task_returns_unprocessable_for_invalid_transition() {
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
            "new_status": "todo", // InProgress → Todo is not allowed
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    server.server.abort();
}

// ── Inbox API tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn post_inbox_creates_note_with_timestamp_filename() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/inbox"))
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
async fn post_inbox_with_title_uses_title_in_filename() {
    let server = TestServer::empty().await;
    let client = reqwest::Client::new();

    let response = client
        .post(server.url("/api/v/test-vault/inbox"))
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
async fn get_inbox_lists_inbox_notes() {
    let server = TestServer::with_files(&[
        ("Inbox/2026-05-09 10-00-00 - Note One.md", "First note"),
        ("Inbox/2026-05-09 10-01-00 - Note Two.md", "Second note"),
        ("Other/Not Inbox.md", "Should not appear"),
    ])
    .await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/api/v/test-vault/inbox"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.json::<Vec<serde_json::Value>>().await.unwrap();
    assert!(
        body.len() >= 2,
        "expected at least 2 inbox notes, got {}",
        body.len()
    );
    // All returned notes should be in Inbox/
    for note in &body {
        let path = note["path"].as_str().unwrap();
        assert!(
            path.starts_with("Inbox/"),
            "expected Inbox/ path, got {path}"
        );
    }

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
