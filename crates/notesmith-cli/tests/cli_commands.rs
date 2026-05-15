use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
use std::{
    collections::BTreeMap,
    fs,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

/// Maximum time to wait for the daemon to respond to /ping.
/// Debug builds can take 2-3s to start (vault scan, index build, watcher setup).
const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(10);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(100);

fn create_vault(root: &std::path::Path, name: &str) {
    let config = VaultConfig {
        name: name.to_string(),
        capture: notesmith_config::CaptureConfig {
            folder: "Inbox".to_string(),
            template: "generic-note".to_string(),
        },
        daily: notesmith_config::DailyConfig {
            folder: "Inbox/Daily".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    let config_dir = root.join(".notesmith");
    fs::create_dir_all(&config_dir).unwrap();
    config.save_to(&config_dir.join("vault.toml")).unwrap();
}

fn notesmith_bin() -> String {
    std::env::var("CARGO_BIN_EXE_notesmith").unwrap()
}

fn write_global_config(
    config_root: &std::path::Path,
    vault_name: &str,
    vault_root: &std::path::Path,
    bind: Option<String>,
) {
    let mut config = GlobalConfig {
        daemon: Default::default(),
        default_vault: Some(vault_name.to_string()),
        vaults: BTreeMap::from([(
            vault_name.to_string(),
            VaultRegistration {
                path: vault_root.to_path_buf(),
            },
        )]),
    };
    if let Some(bind) = bind {
        config.daemon.bind = bind;
    }

    config
        .save_to(&config_root.join("notesmith").join("config.toml"))
        .unwrap();
}

/// Wait for a spawned daemon to respond to /ping, or panic with diagnostics.
async fn wait_for_daemon(child: &mut Child, bind: &std::net::SocketAddr) {
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + DAEMON_READY_TIMEOUT;
    let mut last_error = None;

    while tokio::time::Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => {
                panic!("daemon exited early with {status}");
            }
            Ok(None) => {}
            Err(e) => panic!("failed to check daemon status: {e}"),
        }
        match client.get(format!("http://{bind}/ping")).send().await {
            Ok(resp) if resp.status().is_success() => return,
            Ok(resp) => last_error = Some(format!("unexpected status {}", resp.status())),
            Err(e) => last_error = Some(e.to_string()),
        }
        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
    }

    let _ = child.kill();
    panic!(
        "daemon did not become ready within {DAEMON_READY_TIMEOUT:?}: {:?}",
        last_error,
    );
}

#[test]
fn vault_reindex_creates_cache_file() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::create_dir_all(vault_root.join("Inbox")).unwrap();
    fs::write(vault_root.join("Inbox/Note.md"), "# Note\n").unwrap();

    let cache_home = temp_dir.path().join("cache-home");

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["vault", "reindex"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(cache_home.join("notesmith/work/cache.sqlite").exists());
    assert!(cache_home.join("notesmith/work/tantivy").exists());
}

#[tokio::test]
async fn top_level_reindex_auto_starts_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::create_dir_all(vault_root.join("Inbox")).unwrap();
    fs::write(vault_root.join("Inbox/Note.md"), "# Note\n").unwrap();

    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    let runtime_dir = temp_dir.path().join("runtime");
    write_global_config(&config_home, "work", &vault_root, Some(bind.to_string()));

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", temp_dir.path())
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .args(["reindex", "--cache-only"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Reindexed 1 notes for work"),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let response = reqwest::Client::new()
        .get(format!("http://{bind}/ping"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());

    let shutdown = reqwest::Client::new()
        .post(format!("http://{bind}/admin/shutdown"))
        .send()
        .await
        .unwrap();
    assert!(shutdown.status().is_success());
}

#[tokio::test]
async fn daemon_start_serves_ping_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::write(vault_root.join("Home.md"), "# Home\n").unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(&config_home, "work", &vault_root, None);

    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);

    let mut child = Command::new(notesmith_bin())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", temp_dir.path())
        .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
        .arg("daemon")
        .arg("start")
        .arg("--bind")
        .arg(bind.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    wait_for_daemon(&mut child, &bind).await;

    child.kill().unwrap();
    let _ = child.wait();
}

#[tokio::test]
async fn query_sql_uses_http_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::write(vault_root.join("Home.md"), "# Home\n").unwrap();

    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(&config_home, "work", &vault_root, Some(bind.to_string()));

    let mut daemon = Command::new(notesmith_bin())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", temp_dir.path())
        .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
        .arg("daemon")
        .arg("start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    wait_for_daemon(&mut daemon, &bind).await;

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args([
            "--format",
            "json",
            "query",
            "sql",
            "SELECT title FROM v_notes ORDER BY title LIMIT 1",
        ])
        .output()
        .unwrap();

    daemon.kill().unwrap();
    let _ = daemon.wait();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["title"]));
    assert_eq!(json["row_count"], serde_json::json!(1));
}

#[tokio::test]
async fn search_uses_http_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::write(
        vault_root.join("Home.md"),
        "# Home\n\nAcme landing zone and searchonlyneedle.\n",
    )
    .unwrap();

    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(&config_home, "work", &vault_root, Some(bind.to_string()));

    let mut daemon = Command::new(notesmith_bin())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", temp_dir.path())
        .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
        .arg("daemon")
        .arg("start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    wait_for_daemon(&mut daemon, &bind).await;

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["search", "searchonlyneedle"])
        .output()
        .unwrap();

    daemon.kill().unwrap();
    let _ = daemon.wait();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Home.md"));
    assert!(stdout.contains("Home"));
}
