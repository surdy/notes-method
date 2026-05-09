use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
use std::{
    collections::BTreeMap,
    fs,
    process::{Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

fn create_vault(root: &std::path::Path, name: &str) {
    let config = VaultConfig {
        name: name.to_string(),
        homepage: None,
        inbox: Default::default(),
        daily: Default::default(),
        editor: Default::default(),
        git: Default::default(),
        hooks: Default::default(),
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
        .arg("daemon")
        .arg("start")
        .arg("--bind")
        .arg(bind.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let client = reqwest::Client::new();
    let mut last_error = None;
    for _ in 0..20 {
        match client.get(format!("http://{bind}/ping")).send().await {
            Ok(response) if response.status().is_success() => {
                child.kill().unwrap();
                let _ = child.wait();
                return;
            }
            Ok(response) => {
                last_error = Some(format!("unexpected status {}", response.status()));
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    child.kill().unwrap();
    let output = child.wait_with_output().unwrap();
    panic!(
        "daemon did not start: {:?}\nstdout: {}\nstderr: {}",
        last_error,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
        .arg("daemon")
        .arg("start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let client = reqwest::Client::new();
    for _ in 0..20 {
        if client
            .get(format!("http://{bind}/ping"))
            .send()
            .await
            .is_ok()
        {
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
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    daemon.kill().unwrap();
    let output = daemon.wait_with_output().unwrap();
    panic!(
        "daemon did not become ready\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
        .arg("daemon")
        .arg("start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let client = reqwest::Client::new();
    for _ in 0..20 {
        if client
            .get(format!("http://{bind}/ping"))
            .send()
            .await
            .is_ok()
        {
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
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    daemon.kill().unwrap();
    let output = daemon.wait_with_output().unwrap();
    panic!(
        "daemon did not become ready\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
