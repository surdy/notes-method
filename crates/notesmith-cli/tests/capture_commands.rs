use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
use std::{
    collections::BTreeMap,
    fs,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

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
    async fn start(
        config_home: &std::path::Path,
        cache_home: &std::path::Path,
        bind: String,
    ) -> Self {
        let child = Command::new(notesmith_bin())
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_CACHE_HOME", cache_home)
            .env("HOME", config_home.parent().unwrap())
            .env(
                "XDG_RUNTIME_DIR",
                config_home.parent().unwrap().join("runtime"),
            )
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
                return Self { child };
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let output = child.wait_with_output().unwrap();
        panic!(
            "daemon did not become ready\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn bind_address() -> String {
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);
    bind.to_string()
}

#[tokio::test]
async fn capture_creates_note_via_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    let bind = bind_address();
    write_global_config(&config_home, "work", &vault_root, bind.clone());

    let _daemon = DaemonProcess::start(&config_home, &cache_home, bind).await;

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
