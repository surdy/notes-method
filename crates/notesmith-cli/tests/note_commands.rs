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
        agent: Default::default(),
        default_vault: Some(vault_name.to_string()),
        vaults: BTreeMap::from([(
            vault_name.to_string(),
            VaultRegistration {
                path: vault_root.to_path_buf(),
            },
        )]),
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
async fn note_create_and_get_use_http_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    let bind = bind_address();
    write_global_config(&config_home, "work", &vault_root, bind.clone());

    let _daemon = DaemonProcess::start(&config_home, &cache_home, bind).await;

    let create_output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args([
            "--format",
            "json",
            "note",
            "create",
            "Example",
            "--content",
            "Hello from CLI",
        ])
        .output()
        .unwrap();

    assert!(
        create_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&create_output.stdout),
        String::from_utf8_lossy(&create_output.stderr)
    );

    let create_json: serde_json::Value = serde_json::from_slice(&create_output.stdout).unwrap();
    assert_eq!(create_json["path"], serde_json::json!("Inbox/Example.md"));

    let get_output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["note", "get", "Inbox/Example.md"])
        .output()
        .unwrap();

    assert!(
        get_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&get_output.stdout),
        String::from_utf8_lossy(&get_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&get_output.stdout),
        "Hello from CLI\n"
    );
}

#[tokio::test]
async fn note_put_from_stdin_and_append_use_http_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::create_dir_all(vault_root.join("Inbox")).unwrap();
    fs::write(vault_root.join("Inbox/Example.md"), "# Old body\n").unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    let bind = bind_address();
    write_global_config(&config_home, "work", &vault_root, bind.clone());

    let _daemon = DaemonProcess::start(&config_home, &cache_home, bind).await;

    let mut put_command = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["note", "put", "Inbox/Example.md", "--from-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = put_command.stdin.as_mut().unwrap();
        stdin.write_all(b"Replaced from stdin").unwrap();
    }
    let put_output = put_command.wait_with_output().unwrap();

    assert!(
        put_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&put_output.stdout),
        String::from_utf8_lossy(&put_output.stderr)
    );

    let append_output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["note", "append", "Inbox/Example.md", "Second line"])
        .output()
        .unwrap();

    assert!(
        append_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&append_output.stdout),
        String::from_utf8_lossy(&append_output.stderr)
    );

    let get_output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["note", "get", "Inbox/Example.md"])
        .output()
        .unwrap();

    assert!(
        get_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&get_output.stdout),
        String::from_utf8_lossy(&get_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&get_output.stdout),
        "Replaced from stdin\nSecond line\n"
    );
}

#[tokio::test]
async fn note_move_and_delete_use_http_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::create_dir_all(vault_root.join("Inbox")).unwrap();
    fs::write(vault_root.join("Inbox/Example.md"), "# Move me\n").unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    let bind = bind_address();
    write_global_config(&config_home, "work", &vault_root, bind.clone());

    let _daemon = DaemonProcess::start(&config_home, &cache_home, bind).await;

    let move_output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args([
            "--format",
            "json",
            "note",
            "move",
            "Inbox/Example.md",
            "Archive/Example.md",
        ])
        .output()
        .unwrap();

    assert!(
        move_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&move_output.stdout),
        String::from_utf8_lossy(&move_output.stderr)
    );

    let move_json: serde_json::Value = serde_json::from_slice(&move_output.stdout).unwrap();
    assert_eq!(move_json["from"], serde_json::json!("Inbox/Example.md"));
    assert_eq!(move_json["to"], serde_json::json!("Archive/Example.md"));
    assert!(vault_root.join("Archive/Example.md").exists());

    let delete_output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["note", "delete", "Archive/Example.md"])
        .output()
        .unwrap();

    assert!(
        delete_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&delete_output.stdout),
        String::from_utf8_lossy(&delete_output.stderr)
    );
    assert!(!vault_root.join("Archive/Example.md").exists());
}
