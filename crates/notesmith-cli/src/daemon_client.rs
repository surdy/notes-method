use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use notesmith_config::{DaemonLockfile, GlobalConfig};
use reqwest::Url;

const DAEMON_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub fn daemon_url(config: &GlobalConfig) -> Result<Url> {
    Url::parse(&format!("http://{}/", config.daemon.bind))
        .with_context(|| format!("invalid daemon bind address: {}", config.daemon.bind))
}

pub async fn is_daemon_running(config: &GlobalConfig) -> bool {
    let Ok(base_url) = daemon_url(config) else {
        return false;
    };
    let Ok(status_url) = base_url.join("api/status") else {
        return false;
    };

    reqwest::Client::new()
        .get(status_url)
        .timeout(DAEMON_HEALTH_TIMEOUT)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

pub async fn ensure_daemon(config: &GlobalConfig) -> Result<Url> {
    let url = daemon_url(config)?;
    if is_daemon_running(config).await {
        return Ok(url);
    }

    if DaemonLockfile::read_active()?.is_some() {
        return wait_for_daemon(config).await;
    }

    if !config.daemon.auto_start {
        bail!(
            "could not reach the Notesmith daemon at {}. Start it with `notesmith daemon start`",
            config.daemon.bind
        );
    }

    eprintln!("Starting Notesmith daemon...");
    start_daemon_background(config)?;
    wait_for_daemon(config).await
}

fn daemon_start_command(config: &GlobalConfig) -> Result<Command> {
    let program = std::env::current_exe().context("could not determine current executable path")?;
    Ok(daemon_start_command_for(&program, config))
}

fn daemon_start_command_for(program: &Path, config: &GlobalConfig) -> Command {
    let mut command = Command::new(program);
    command
        .arg("daemon")
        .arg("start")
        .arg("--bind")
        .arg(&config.daemon.bind)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: `setsid` only mutates the child process state between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    command
}

fn start_daemon_background(config: &GlobalConfig) -> Result<()> {
    daemon_start_command(config)?
        .spawn()
        .context("failed to spawn daemon process")?;
    Ok(())
}

async fn wait_for_daemon(config: &GlobalConfig) -> Result<Url> {
    let url = daemon_url(config)?;
    let deadline = tokio::time::Instant::now() + DAEMON_STARTUP_TIMEOUT;

    loop {
        if is_daemon_running(config).await {
            return Ok(url);
        }

        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Notesmith daemon did not start within {}s. Check logs with `notesmith daemon start` for errors.",
                DAEMON_STARTUP_TIMEOUT.as_secs()
            );
        }

        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{daemon_start_command_for, daemon_url};
    use notesmith_config::GlobalConfig;
    use std::{ffi::OsStr, path::Path};

    #[test]
    fn daemon_url_uses_configured_bind_address() {
        let mut config = GlobalConfig::default();
        config.daemon.bind = "127.0.0.1:39000".to_string();

        let url = daemon_url(&config).unwrap();

        assert_eq!(url.as_str(), "http://127.0.0.1:39000/");
    }

    #[test]
    fn daemon_start_command_uses_bind_override() {
        let mut config = GlobalConfig::default();
        config.daemon.bind = "127.0.0.1:39000".to_string();

        let command = daemon_start_command_for(Path::new("/usr/local/bin/notesmith"), &config);
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            command.get_program(),
            OsStr::new("/usr/local/bin/notesmith")
        );
        assert_eq!(args, vec!["daemon", "start", "--bind", "127.0.0.1:39000"]);
    }
}
