use notesmith_config::DaemonLockfile;
use std::{future::Future, io, path::PathBuf, process::Stdio, time::Duration};
use tokio::time::Instant;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:27183";
const DEFAULT_DAEMON_BIN: &str = "notesmith";
const START_COMMAND: [&str; 2] = ["daemon", "start"];

#[derive(Debug, Clone)]
pub struct DaemonSettings {
    pub daemon_url: String,
    pub daemon_bin: String,
    /// When set, use this path instead of `daemon_bin` for spawning.
    /// Populated from Tauri sidecar resolution at app startup.
    pub sidecar_path: Option<PathBuf>,
    pub ping_timeout: Duration,
    pub startup_wait: Duration,
    pub startup_poll_interval: Duration,
}

impl Default for DaemonSettings {
    fn default() -> Self {
        Self {
            daemon_url: std::env::var("NOTESMITH_DESKTOP_DAEMON_URL")
                .unwrap_or_else(|_| DEFAULT_DAEMON_URL.to_string()),
            daemon_bin: std::env::var("NOTESMITH_DESKTOP_DAEMON_BIN")
                .unwrap_or_else(|_| DEFAULT_DAEMON_BIN.to_string()),
            sidecar_path: None,
            ping_timeout: Duration::from_secs(2),
            startup_wait: Duration::from_secs(10),
            startup_poll_interval: Duration::from_millis(500),
        }
    }
}

impl DaemonSettings {
    pub fn ping_url(&self) -> String {
        format!("{}/ping", self.daemon_url.trim_end_matches('/'))
    }

    /// Returns the program to execute: sidecar path if available, otherwise the bin name from PATH.
    pub fn program(&self) -> &str {
        self.sidecar_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(&self.daemon_bin)
    }
}

pub struct DaemonSupervisor<P, L> {
    settings: DaemonSettings,
    probe: P,
    launch: L,
}

impl<P, L> DaemonSupervisor<P, L> {
    pub fn new(settings: DaemonSettings, probe: P, launch: L) -> Self {
        Self {
            settings,
            probe,
            launch,
        }
    }
}

impl<P, ProbeFuture, L, LaunchFuture> DaemonSupervisor<P, L>
where
    P: Fn(DaemonSettings) -> ProbeFuture,
    ProbeFuture: Future<Output = bool>,
    L: Fn(DaemonSettings) -> LaunchFuture,
    LaunchFuture: Future<Output = Result<(), DynError>>,
{
    pub async fn ensure_running(&self) -> Result<(), DynError> {
        if (self.probe)(self.settings.clone()).await {
            tracing::info!("notesmith daemon already running");
            return Ok(());
        }

        tracing::info!("notesmith daemon not running; launching");
        (self.launch)(self.settings.clone()).await?;

        let deadline = Instant::now() + self.settings.startup_wait;
        loop {
            if (self.probe)(self.settings.clone()).await {
                tracing::info!("notesmith daemon is ready");
                return Ok(());
            }

            if Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(self.settings.startup_poll_interval).await;
        }

        Err(io::Error::other(format!(
            "notesmith daemon failed to start within {:?}",
            self.settings.startup_wait
        ))
        .into())
    }
}

pub async fn is_daemon_running() -> bool {
    probe_daemon(DaemonSettings::default()).await
}

pub async fn ensure_daemon_running() -> Result<(), DynError> {
    ensure_daemon_running_with(DaemonSettings::default()).await
}

pub async fn ensure_daemon_running_with(settings: DaemonSettings) -> Result<(), DynError> {
    DaemonSupervisor::new(settings, probe_daemon, launch_daemon)
        .ensure_running()
        .await
}

async fn probe_daemon(settings: DaemonSettings) -> bool {
    let daemon_url = discover_daemon_url(&settings);
    let client = match reqwest::Client::builder()
        .timeout(settings.ping_timeout)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!("failed to build reqwest client for daemon ping: {error}");
            return false;
        }
    };

    client
        .get(format!("{}/ping", daemon_url.trim_end_matches('/')))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

fn discover_daemon_url(settings: &DaemonSettings) -> String {
    match DaemonLockfile::read_active() {
        Ok(Some(lockfile)) => daemon_url_for_port(&settings.daemon_url, lockfile.port),
        Ok(None) => settings.daemon_url.clone(),
        Err(error) => {
            tracing::warn!("failed to read daemon lockfile: {error}");
            settings.daemon_url.clone()
        }
    }
}

fn daemon_url_for_port(base_url: &str, port: u16) -> String {
    let fallback = format!("http://127.0.0.1:{port}");
    let Ok(mut url) = reqwest::Url::parse(base_url) else {
        return fallback;
    };

    if url.set_port(Some(port)).is_err() {
        return fallback;
    }

    url.to_string().trim_end_matches('/').to_string()
}

async fn launch_daemon(settings: DaemonSettings) -> Result<(), DynError> {
    let program = settings.program().to_string();
    tracing::info!("launching daemon: {program} {:?}", START_COMMAND);
    let mut command = tokio::process::Command::new(&program);
    command
        .args(START_COMMAND)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        // SAFETY: The child process immediately detaches before exec so the daemon
        // can outlive the desktop shell.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tokio::sync::Mutex;

    use super::{DaemonSettings, DaemonSupervisor, DynError};

    fn test_settings() -> DaemonSettings {
        DaemonSettings {
            daemon_url: "http://127.0.0.1:27183".into(),
            daemon_bin: "notesmith".into(),
            sidecar_path: None,
            ping_timeout: std::time::Duration::from_millis(5),
            startup_wait: std::time::Duration::from_millis(30),
            startup_poll_interval: std::time::Duration::from_millis(5),
        }
    }

    #[tokio::test]
    async fn returns_without_launching_when_daemon_is_already_running() {
        let launches = Arc::new(AtomicUsize::new(0));
        let launches_for_probe = launches.clone();
        let launches_for_launch = launches.clone();

        let supervisor = DaemonSupervisor::new(
            test_settings(),
            move |_| {
                let launches = launches_for_probe.clone();
                async move {
                    assert_eq!(launches.load(Ordering::SeqCst), 0);
                    true
                }
            },
            move |_| {
                let launches = launches_for_launch.clone();
                async move {
                    launches.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), DynError>(())
                }
            },
        );

        supervisor.ensure_running().await.unwrap();
        assert_eq!(launches.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn launches_and_waits_until_daemon_reports_ready() {
        let launches = Arc::new(AtomicUsize::new(0));
        let probe_calls = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(Mutex::new(VecDeque::from([false, false, true])));

        let supervisor = DaemonSupervisor::new(
            test_settings(),
            {
                let probe_calls = probe_calls.clone();
                let responses = responses.clone();
                move |_| {
                    let probe_calls = probe_calls.clone();
                    let responses = responses.clone();
                    async move {
                        probe_calls.fetch_add(1, Ordering::SeqCst);
                        responses.lock().await.pop_front().unwrap_or(true)
                    }
                }
            },
            {
                let launches = launches.clone();
                move |_| {
                    let launches = launches.clone();
                    async move {
                        launches.fetch_add(1, Ordering::SeqCst);
                        Ok::<(), DynError>(())
                    }
                }
            },
        );

        supervisor.ensure_running().await.unwrap();

        assert_eq!(launches.load(Ordering::SeqCst), 1);
        assert!(
            probe_calls.load(Ordering::SeqCst) >= 3,
            "expected repeated probes until healthy"
        );
    }

    #[tokio::test]
    async fn returns_error_when_daemon_never_becomes_ready() {
        let launches = Arc::new(AtomicUsize::new(0));
        let probe_calls = Arc::new(AtomicUsize::new(0));

        let supervisor = DaemonSupervisor::new(
            test_settings(),
            {
                let probe_calls = probe_calls.clone();
                move |_| {
                    let probe_calls = probe_calls.clone();
                    async move {
                        probe_calls.fetch_add(1, Ordering::SeqCst);
                        false
                    }
                }
            },
            {
                let launches = launches.clone();
                move |_| {
                    let launches = launches.clone();
                    async move {
                        launches.fetch_add(1, Ordering::SeqCst);
                        Ok::<(), DynError>(())
                    }
                }
            },
        );

        let error = supervisor.ensure_running().await.unwrap_err();

        assert!(error.to_string().contains("failed to start"));
        assert_eq!(launches.load(Ordering::SeqCst), 1);
        assert!(probe_calls.load(Ordering::SeqCst) > 1);
    }

    #[test]
    fn program_returns_sidecar_path_when_set() {
        let mut settings = test_settings();
        settings.sidecar_path = Some(std::path::PathBuf::from(
            "/app/bin/notesmith-aarch64-apple-darwin",
        ));
        assert_eq!(
            settings.program(),
            "/app/bin/notesmith-aarch64-apple-darwin"
        );
    }

    #[test]
    fn program_falls_back_to_daemon_bin_when_no_sidecar() {
        let settings = test_settings();
        assert_eq!(settings.program(), "notesmith");
    }

    #[test]
    fn daemon_url_for_port_replaces_existing_port() {
        assert_eq!(
            super::daemon_url_for_port("http://127.0.0.1:27183", 39000),
            "http://127.0.0.1:39000"
        );
    }

    #[test]
    fn daemon_url_for_port_falls_back_for_invalid_base_url() {
        assert_eq!(
            super::daemon_url_for_port("not a url", 39000),
            "http://127.0.0.1:39000"
        );
    }
}
