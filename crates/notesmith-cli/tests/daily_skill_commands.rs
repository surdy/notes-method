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

#[test]
fn skill_print_outputs_vault_skill_file() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::write(
        vault_root.join(".notesmith/skill.md"),
        "# Vault Skill\n\nUse this vault carefully.\n",
    )
    .unwrap();

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .args(["skill", "print"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("# Vault Skill"));
}

#[tokio::test]
async fn daily_agent_create_prints_prompt_via_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");
    fs::create_dir_all(vault_root.join(".notesmith/prompts")).unwrap();
    fs::write(
        vault_root.join(".notesmith/prompts/daily-note.md"),
        "# Daily Note Prompt\n\nToday's date: {{ today }}\n",
    )
    .unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(&config_home, "work", &vault_root, "127.0.0.1:0".to_string());

    let (_daemon, _bind) = DaemonProcess::start(&config_home, &cache_home).await;

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["daily", "agent-create", "--date", "2026-05-10"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Today's date: 2026-05-10"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[tokio::test]
async fn daily_agent_create_with_content_creates_note_via_daemon() {
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
        .args([
            "daily",
            "agent-create",
            "--date",
            "2026-05-10",
            "--content=---\ntype: daily\ndate: 2026-05-10\n---\n# 2026-05-10\n",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(vault_root.join("Inbox/Daily/2026-05-10.md").exists());
}

#[tokio::test]
async fn periodic_open_creates_weekly_note_via_daemon() {
    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("work");
    create_vault(&vault_root, "work");

    let mut config = VaultConfig::load_from_vault(&vault_root).unwrap();
    config.periodic.weekly = Some(notesmith_config::PeriodKindConfig {
        folder: "Weekly".to_string(),
        template: Some("weekly".to_string()),
        filename: "Week {{ week }}".to_string(),
        generate_at: None,
        timezone: None,
        catch_up: false,
    });
    config.save_to_vault(&vault_root).unwrap();

    fs::create_dir_all(vault_root.join("Assets/templates")).unwrap();
    fs::write(
        vault_root.join("Assets/templates/weekly.md.j2"),
        r#"---
notesmith:
  name: weekly
  description: Weekly note
  output_path: "ignored/{{ week }}.md"
---
# {{ period_key }}
{{ period_start }} → {{ period_end }}
"#,
    )
    .unwrap();

    let config_home = temp_dir.path().join("config-home");
    let cache_home = temp_dir.path().join("cache-home");
    write_global_config(&config_home, "work", &vault_root, "127.0.0.1:0".to_string());

    let (_daemon, _bind) = DaemonProcess::start(&config_home, &cache_home).await;

    let output = Command::new(notesmith_bin())
        .current_dir(&vault_root)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .args(["periodic", "open", "weekly", "--offset", "-1"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let target_date = chrono::Local::now().date_naive() - chrono::Duration::weeks(1);
    let week_key = target_date.format("%G-W%V").to_string();
    let expected_path = vault_root.join(format!("Weekly/Week {week_key}.md"));
    assert!(
        expected_path.exists(),
        "missing {}",
        expected_path.display()
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&week_key));
}
