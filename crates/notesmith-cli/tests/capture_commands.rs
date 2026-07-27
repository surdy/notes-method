use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
use std::{
    collections::BTreeMap,
    fs,
    process::{Child, Command, Stdio},
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
    bind: String,
) {
    let config = GlobalConfig {
        daemon: notesmith_config::DaemonConfig {
            bind,
            auto_start: true,
        },
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

    config
        .save_to(&config_root.join("notesmith").join("config.toml"))
        .unwrap();
}

struct DaemonProcess {
    child: Child,
}

impl DaemonProcess {
    /// Spawn a daemon on a free port and wait until it is serving. Retries on a
    /// different port if the daemon loses the bind race (see `common`).
    /// Returns the process handle and the bind address it is serving on.
    async fn start(config_home: &std::path::Path, cache_home: &std::path::Path) -> (Self, String) {
        let home = config_home.parent().unwrap().to_path_buf();
        let runtime_dir = home.join("runtime");

        let (child, bind) = common::spawn_daemon_retrying(|bind| {
            // Rewritten per attempt so the CLI calls that follow reach the port
            // the daemon actually got.
            rewrite_daemon_bind(config_home, bind);
            Command::new(notesmith_bin())
                .env("XDG_CONFIG_HOME", config_home)
                .env("XDG_CACHE_HOME", cache_home)
                .env("HOME", &home)
                .env("XDG_RUNTIME_DIR", &runtime_dir)
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

        (Self { child }, bind)
    }
}

/// Point the global config at `bind`, preserving everything else.
fn rewrite_daemon_bind(config_home: &std::path::Path, bind: &str) {
    let path = config_home.join("notesmith").join("config.toml");
    let mut config = GlobalConfig::load_from(&path).unwrap();
    config.daemon.bind = bind.to_string();
    config.save_to(&path).unwrap();
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn capture_creates_note_via_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(&config_home, "work", &vault_root, "127.0.0.1:0".to_string());

    let (_daemon, _bind) = DaemonProcess::start(&config_home, &cache_home).await;

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["--format", "json", "capture", "Quick thought from CLI"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let path = json["path"].as_str().unwrap();
    assert!(
        path.starts_with("Inbox/"),
        "path should start with Inbox/: {path}"
    );
    assert!(path.ends_with(".md"), "path should end with .md: {path}");

    // Verify file exists on disk
    let file_path = vault_root.join(path);
    assert!(
        file_path.exists(),
        "file should exist at {}",
        file_path.display()
    );
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("Quick thought from CLI"));
}
