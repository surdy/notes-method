use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
use std::{
    collections::BTreeMap,
    fs,
    process::{Command, Stdio},
};
use tempfile::TempDir;

mod common;

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
        agents: Default::default(),
        mcp: Default::default(),
    };
    if let Some(bind) = bind {
        config.daemon.bind = bind;
    }

    config
        .save_to(&config_root.join("notesmith").join("config.toml"))
        .unwrap();
}

#[tokio::test]
async fn top_level_reindex_auto_starts_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::create_dir_all(vault_root.join("Inbox")).unwrap();
    fs::write(vault_root.join("Inbox/Note.md"), "# Note\n").unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    let runtime_dir = temp_dir.path().join("runtime");

    // The daemon here is auto-started by `reindex` itself, so there is no child
    // to watch — retry the whole command if its port was taken first.
    let (output, bind) = common::retrying_on_free_port(|bind| {
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

        if output.status.success() {
            Ok(output)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    });
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

    let (mut child, _bind) = common::spawn_daemon_retrying(|bind| {
        Command::new(notesmith_bin())
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_CACHE_HOME", &cache_home)
            .env("HOME", temp_dir.path())
            .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
            .arg("daemon")
            .arg("start")
            .arg("--bind")
            .arg(bind)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    })
    .await;

    child.kill().unwrap();
    let _ = child.wait();
}

#[tokio::test]
async fn query_sql_uses_http_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::write(vault_root.join("Home.md"), "# Home\n").unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(
        &config_home,
        "work",
        &vault_root,
        Some("127.0.0.1:0".to_string()),
    );

    let (mut daemon, _bind) = common::spawn_daemon_retrying(|bind| {
        write_global_config(&config_home, "work", &vault_root, Some(bind.to_string()));
        Command::new(notesmith_bin())
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_CACHE_HOME", &cache_home)
            .env("HOME", temp_dir.path())
            .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
            .arg("daemon")
            .arg("start")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    })
    .await;

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

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(
        &config_home,
        "work",
        &vault_root,
        Some("127.0.0.1:0".to_string()),
    );

    let (mut daemon, _bind) = common::spawn_daemon_retrying(|bind| {
        write_global_config(&config_home, "work", &vault_root, Some(bind.to_string()));
        Command::new(notesmith_bin())
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_CACHE_HOME", &cache_home)
            .env("HOME", temp_dir.path())
            .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
            .arg("daemon")
            .arg("start")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    })
    .await;

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

/// `--url` and `NOTESMITH_URL` retarget daemon-backed commands at a remote
/// daemon, overriding the configured local bind without auto-starting locally.
#[tokio::test]
async fn search_targets_remote_daemon_via_url_override() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::write(
        vault_root.join("Home.md"),
        "# Home\n\nAcme landing zone and remoteonlyneedle.\n",
    )
    .unwrap();

    // The "remote" daemon we expect commands to reach.
    // A port nothing listens on, recorded as the *local* bind. If the override is
    // ignored, commands would target this dead address and fail.
    let dead_local_bind = common::free_port();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(
        &config_home,
        "work",
        &vault_root,
        Some(dead_local_bind.clone()),
    );

    let (mut daemon, remote_bind) = common::spawn_daemon_retrying(|bind| {
        Command::new(notesmith_bin())
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_CACHE_HOME", &cache_home)
            .env("HOME", temp_dir.path())
            .env("XDG_RUNTIME_DIR", temp_dir.path().join("runtime"))
            .arg("daemon")
            .arg("start")
            .arg("--bind")
            .arg(bind)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    })
    .await;

    let remote_url = format!("http://{remote_bind}");

    // 1. `--url` flag targets the remote daemon.
    let via_flag = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["--url", &remote_url, "search", "remoteonlyneedle"])
        .output()
        .unwrap();

    // 2. `NOTESMITH_URL` env var targets the remote daemon.
    let via_env = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("NOTESMITH_URL", &remote_url)
        .args(["search", "remoteonlyneedle"])
        .output()
        .unwrap();

    daemon.kill().unwrap();
    let _ = daemon.wait();

    assert!(
        via_flag.status.success(),
        "--url flag run failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&via_flag.stdout),
        String::from_utf8_lossy(&via_flag.stderr)
    );
    assert!(
        String::from_utf8_lossy(&via_flag.stdout).contains("Home.md"),
        "--url stdout: {}",
        String::from_utf8_lossy(&via_flag.stdout)
    );

    assert!(
        via_env.status.success(),
        "NOTESMITH_URL run failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&via_env.stdout),
        String::from_utf8_lossy(&via_env.stderr)
    );
    assert!(
        String::from_utf8_lossy(&via_env.stdout).contains("Home.md"),
        "NOTESMITH_URL stdout: {}",
        String::from_utf8_lossy(&via_env.stdout)
    );
}
