use notesmith_config::DaemonLockfile;
use serde::Deserialize;
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

    pub fn status_url(&self) -> String {
        format!("{}/api/status", self.daemon_url.trim_end_matches('/'))
    }

    pub fn with_daemon_url(&self, daemon_url: impl Into<String>) -> Self {
        let mut settings = self.clone();
        settings.daemon_url = daemon_url.into();
        settings
    }

    /// Returns the program to execute: sidecar path if available, otherwise the bin name from PATH.
    pub fn program(&self) -> &str {
        self.sidecar_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(&self.daemon_bin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonState {
    Ready,
    VersionMismatch { running: String, bundled: String },
    Unreachable,
    PortConflict { pid: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    pub version: String,
}

pub struct DaemonSupervisor<P, L, R = fn() -> Result<Option<DaemonLockfile>, DynError>> {
    settings: DaemonSettings,
    probe: P,
    launch: L,
    read_lockfile: R,
}

impl<P, L> DaemonSupervisor<P, L, fn() -> Result<Option<DaemonLockfile>, DynError>> {
    pub fn new(settings: DaemonSettings, probe: P, launch: L) -> Self {
        Self {
            settings,
            probe,
            launch,
            read_lockfile: no_lockfile,
        }
    }
}

impl<P, L, R> DaemonSupervisor<P, L, R> {
    pub fn with_lockfile_reader<R2>(self, read_lockfile: R2) -> DaemonSupervisor<P, L, R2> {
        DaemonSupervisor {
            settings: self.settings,
            probe: self.probe,
            launch: self.launch,
            read_lockfile,
        }
    }
}

impl<P, ProbeFuture, L, LaunchFuture, R> DaemonSupervisor<P, L, R>
where
    P: Fn(DaemonSettings) -> ProbeFuture,
    ProbeFuture: Future<Output = bool>,
    L: Fn(DaemonSettings) -> LaunchFuture,
    LaunchFuture: Future<Output = Result<(), DynError>>,
    R: Fn() -> Result<Option<DaemonLockfile>, DynError>,
{
    pub async fn ensure_running(&self) -> Result<(), DynError> {
        if let Some((settings, lockfile)) = self.read_lockfile_settings()? {
            if self.wait_until_ready(settings).await {
                tracing::info!("notesmith daemon discovered via lockfile");
                return Ok(());
            }

            return Err(io::Error::other(format!(
                "notesmith daemon pid {} from lockfile did not respond",
                lockfile.pid
            ))
            .into());
        }

        if (self.probe)(self.settings.clone()).await {
            tracing::info!("notesmith daemon already running");
            return Ok(());
        }

        tracing::info!("notesmith daemon not running; launching");
        (self.launch)(self.settings.clone()).await?;

        if self.wait_until_ready(self.settings.clone()).await {
            tracing::info!("notesmith daemon is ready");
            return Ok(());
        }

        Err(io::Error::other(format!(
            "notesmith daemon failed to start within {:?}",
            self.settings.startup_wait
        ))
        .into())
    }

    fn read_lockfile_settings(&self) -> Result<Option<(DaemonSettings, DaemonLockfile)>, DynError> {
        match (self.read_lockfile)() {
            Ok(Some(lockfile)) => Ok(Some((
                daemon_settings_for_lockfile(&self.settings, &lockfile),
                lockfile,
            ))),
            Ok(None) => Ok(None),
            Err(error) => {
                tracing::warn!("failed to read daemon lockfile: {error}");
                Ok(None)
            }
        }
    }

    async fn wait_until_ready(&self, settings: DaemonSettings) -> bool {
        let deadline = Instant::now() + settings.startup_wait;
        loop {
            if (self.probe)(settings.clone()).await {
                return true;
            }

            if Instant::now() >= deadline {
                return false;
            }

            tokio::time::sleep(settings.startup_poll_interval).await;
        }
    }
}

pub struct StartupOrchestrator<R, P, L, S> {
    settings: DaemonSettings,
    read_lockfile: R,
    probe: P,
    launch: L,
    fetch_status: S,
}

impl<R, P, L, S> StartupOrchestrator<R, P, L, S> {
    pub fn new(
        settings: DaemonSettings,
        read_lockfile: R,
        probe: P,
        launch: L,
        fetch_status: S,
    ) -> Self {
        Self {
            settings,
            read_lockfile,
            probe,
            launch,
            fetch_status,
        }
    }
}

impl<R, P, ProbeFuture, L, LaunchFuture, S, StatusFuture> StartupOrchestrator<R, P, L, S>
where
    R: Fn() -> Result<Option<DaemonLockfile>, DynError>,
    P: Fn(DaemonSettings) -> ProbeFuture,
    ProbeFuture: Future<Output = bool>,
    L: Fn(DaemonSettings) -> LaunchFuture,
    LaunchFuture: Future<Output = Result<(), DynError>>,
    S: Fn(DaemonSettings) -> StatusFuture,
    StatusFuture: Future<Output = Result<DaemonStatus, DynError>>,
{
    pub async fn orchestrate_startup(&self) -> DaemonState {
        if let Some((settings, lockfile)) = self.read_lockfile_settings() {
            if self.wait_until_ready(settings.clone()).await {
                return self.check_version(settings).await;
            }

            return DaemonState::PortConflict { pid: lockfile.pid };
        }

        if (self.probe)(self.settings.clone()).await {
            return self.check_version(self.settings.clone()).await;
        }

        if let Err(error) = (self.launch)(self.settings.clone()).await {
            tracing::error!("failed to launch notesmith daemon: {error}");
            return DaemonState::Unreachable;
        }

        if self.wait_until_ready(self.settings.clone()).await {
            return self.check_version(self.settings.clone()).await;
        }

        DaemonState::Unreachable
    }

    fn read_lockfile_settings(&self) -> Option<(DaemonSettings, DaemonLockfile)> {
        match (self.read_lockfile)() {
            Ok(Some(lockfile)) => Some((
                daemon_settings_for_lockfile(&self.settings, &lockfile),
                lockfile,
            )),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!("failed to read daemon lockfile: {error}");
                None
            }
        }
    }

    async fn wait_until_ready(&self, settings: DaemonSettings) -> bool {
        let deadline = Instant::now() + settings.startup_wait;
        loop {
            if (self.probe)(settings.clone()).await {
                return true;
            }

            if Instant::now() >= deadline {
                return false;
            }

            tokio::time::sleep(settings.startup_poll_interval).await;
        }
    }

    async fn check_version(&self, settings: DaemonSettings) -> DaemonState {
        let bundled = env!("CARGO_PKG_VERSION").to_string();
        match (self.fetch_status)(settings).await {
            Ok(status) if status.version == bundled => DaemonState::Ready,
            Ok(status) => DaemonState::VersionMismatch {
                running: status.version,
                bundled,
            },
            Err(error) => {
                tracing::error!("failed to fetch daemon status: {error}");
                DaemonState::Unreachable
            }
        }
    }
}

pub async fn is_daemon_running() -> bool {
    probe_daemon(resolve_daemon_settings(&DaemonSettings::default())).await
}

pub async fn ensure_daemon_running() -> Result<(), DynError> {
    ensure_daemon_running_with(DaemonSettings::default()).await
}

pub async fn ensure_daemon_running_with(settings: DaemonSettings) -> Result<(), DynError> {
    DaemonSupervisor::new(settings, probe_daemon, launch_daemon)
        .with_lockfile_reader(read_active_lockfile)
        .ensure_running()
        .await
}

pub async fn orchestrate_startup(settings: &DaemonSettings) -> DaemonState {
    StartupOrchestrator::new(
        settings.clone(),
        read_active_lockfile,
        probe_daemon,
        launch_daemon,
        fetch_daemon_status,
    )
    .orchestrate_startup()
    .await
}

pub fn resolve_daemon_settings(settings: &DaemonSettings) -> DaemonSettings {
    match read_active_lockfile() {
        Ok(Some(lockfile)) => daemon_settings_for_lockfile(settings, &lockfile),
        Ok(None) => settings.clone(),
        Err(error) => {
            tracing::warn!("failed to read daemon lockfile: {error}");
            settings.clone()
        }
    }
}

pub fn resolve_daemon_url(settings: &DaemonSettings) -> String {
    resolve_daemon_settings(settings).daemon_url
}

async fn probe_daemon(settings: DaemonSettings) -> bool {
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
        .get(settings.ping_url())
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
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

fn daemon_settings_for_lockfile(
    settings: &DaemonSettings,
    lockfile: &DaemonLockfile,
) -> DaemonSettings {
    settings.with_daemon_url(daemon_url_for_port(&settings.daemon_url, lockfile.port))
}

fn no_lockfile() -> Result<Option<DaemonLockfile>, DynError> {
    Ok(None)
}

fn read_active_lockfile() -> Result<Option<DaemonLockfile>, DynError> {
    DaemonLockfile::read_active().map_err(Into::into)
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    version: String,
}

async fn fetch_daemon_status(settings: DaemonSettings) -> Result<DaemonStatus, DynError> {
    let client = reqwest::Client::builder()
        .timeout(settings.ping_timeout)
        .build()?;
    let response = client
        .get(settings.status_url())
        .send()
        .await?
        .error_for_status()?;
    let status: StatusResponse = response.json().await?;
    Ok(DaemonStatus {
        version: status.version,
    })
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
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use chrono::Utc;
    use notesmith_config::DaemonLockfile;
    use tokio::sync::Mutex;

    use super::{DaemonSettings, DaemonState, DaemonSupervisor, DynError, StartupOrchestrator};

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

    fn sample_lockfile(pid: u32, port: u16) -> DaemonLockfile {
        DaemonLockfile {
            pid,
            port,
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: Utc::now(),
            binary_path: PathBuf::from("/Applications/Notesmith.app/Contents/MacOS/notesmith"),
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

    #[test]
    fn status_url_appends_status_route() {
        let settings = test_settings();
        assert_eq!(settings.status_url(), "http://127.0.0.1:27183/api/status");
    }

    #[tokio::test]
    async fn orchestrate_startup_returns_ready_when_versions_match() {
        let orchestrator = StartupOrchestrator::new(
            test_settings(),
            || Ok(Some(sample_lockfile(4242, 39000))),
            |settings: DaemonSettings| async move {
                assert_eq!(settings.daemon_url, "http://127.0.0.1:39000");
                true
            },
            |_| async { Ok::<(), DynError>(()) },
            |settings: DaemonSettings| async move {
                assert_eq!(settings.status_url(), "http://127.0.0.1:39000/api/status");
                Ok(super::DaemonStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
            },
        );

        assert_eq!(orchestrator.orchestrate_startup().await, DaemonState::Ready);
    }

    #[tokio::test]
    async fn orchestrate_startup_returns_version_mismatch_when_daemon_differs() {
        let orchestrator = StartupOrchestrator::new(
            test_settings(),
            || Ok(None),
            |_| async { true },
            |_| async { Ok::<(), DynError>(()) },
            |_| async {
                Ok(super::DaemonStatus {
                    version: "9.9.9".to_string(),
                })
            },
        );

        assert_eq!(
            orchestrator.orchestrate_startup().await,
            DaemonState::VersionMismatch {
                running: "9.9.9".to_string(),
                bundled: env!("CARGO_PKG_VERSION").to_string(),
            }
        );
    }

    #[tokio::test]
    async fn orchestrate_startup_returns_unreachable_when_launch_never_becomes_ready() {
        let launches = Arc::new(AtomicUsize::new(0));
        let probe_calls = Arc::new(AtomicUsize::new(0));
        let orchestrator = StartupOrchestrator::new(
            test_settings(),
            || Ok(None),
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
            |_| async {
                Ok(super::DaemonStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
            },
        );

        assert_eq!(
            orchestrator.orchestrate_startup().await,
            DaemonState::Unreachable
        );
        assert_eq!(launches.load(Ordering::SeqCst), 1);
        assert!(probe_calls.load(Ordering::SeqCst) > 1);
    }

    #[tokio::test]
    async fn orchestrate_startup_uses_lockfile_port_for_probe_before_launch() {
        let probed_urls = Arc::new(Mutex::new(Vec::<String>::new()));
        let launches = Arc::new(AtomicUsize::new(0));
        let orchestrator = StartupOrchestrator::new(
            test_settings(),
            || Ok(Some(sample_lockfile(9876, 40123))),
            {
                let probed_urls = probed_urls.clone();
                move |settings: DaemonSettings| {
                    let probed_urls = probed_urls.clone();
                    async move {
                        probed_urls.lock().await.push(settings.daemon_url);
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
            |_| async {
                Ok(super::DaemonStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
            },
        );

        assert_eq!(
            orchestrator.orchestrate_startup().await,
            DaemonState::PortConflict { pid: 9876 }
        );
        assert_eq!(launches.load(Ordering::SeqCst), 0);
        let probed_urls = probed_urls.lock().await.clone();
        assert!(!probed_urls.is_empty(), "expected at least one probe");
        assert!(
            probed_urls
                .iter()
                .all(|url| url == "http://127.0.0.1:40123"),
            "expected all probes to use the lockfile port"
        );
    }
}
