use notesmith_config::DaemonLockfile;
use serde::Deserialize;
use std::{future::Future, io, path::PathBuf, process::Stdio, time::Duration};
use tokio::time::Instant;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:27183";
const DEFAULT_DAEMON_BIN: &str = "notesmith";
const START_COMMAND: [&str; 2] = ["daemon", "start"];

#[derive(Debug, Clone)]
pub struct DaemonSettings {
    pub daemon_url: String,
    pub daemon_bin: String,
    /// When set, use this path instead of `daemon_bin` for spawning.
    /// Populated from Tauri sidecar resolution at app startup.
    pub sidecar_path: Option<PathBuf>,
    /// When set, points the spawned local daemon at a directory of pre-bundled
    /// embedding-model files via `NOTESMITH_EMBED_MODEL_DIR`, so first-enable of
    /// embeddings is offline (ADR 0018 §9.2, #256 Part B). Populated from the
    /// Tauri resource dir at startup; `None` in dev/unbundled builds, which then
    /// fall back to the download-on-first-run path.
    pub model_dir: Option<PathBuf>,
    /// When set, points the spawned local daemon at a directory of a pre-bundled
    /// whisper.cpp GGML model via `NOTESMITH_WHISPER_MODEL_DIR`, so first-enable
    /// of transcription is offline (ADR 0023 §3). The daemon never transcribes
    /// itself, but the value is inherited by the colocated `notesmith transcribe`
    /// worker it spawns. `None` in dev/unbundled builds, which then fall back to
    /// the download-on-first-run path.
    pub whisper_model_dir: Option<PathBuf>,
    pub ping_timeout: Duration,
    pub startup_wait: Duration,
    pub startup_poll_interval: Duration,
    /// True when the active connection is a remote server (resolved from the
    /// persisted server list in `effective_settings`). When true, skip the
    /// local lockfile check and daemon launch — the remote daemon is managed
    /// independently and its lockfile is not on this machine.
    pub external_url: bool,
}

impl Default for DaemonSettings {
    fn default() -> Self {
        // The base targets the local daemon; the active connection (local vs a
        // saved remote server) is resolved from the persisted server list in
        // `effective_settings`, which overrides `daemon_url`/`external_url`.
        Self {
            daemon_url: DEFAULT_DAEMON_URL.to_string(),
            daemon_bin: std::env::var("NOTESMITH_DESKTOP_DAEMON_BIN")
                .ok()
                .map(|bin| bin.trim().to_string())
                .filter(|bin| !bin.is_empty())
                .unwrap_or_else(|| DEFAULT_DAEMON_BIN.to_string()),
            sidecar_path: None,
            model_dir: None,
            whisper_model_dir: None,
            ping_timeout: Duration::from_secs(2),
            startup_wait: Duration::from_secs(10),
            startup_poll_interval: Duration::from_millis(500),
            external_url: false,
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
        // Preserve external_url — callers like daemon_settings_for_lockfile only
        // adjust the port, they don't change whether the daemon is remote.
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

async fn resolve_supervised_version(
    settings: DaemonSettings,
    child: Option<tokio::process::Child>,
) -> SupervisedStartup {
    let current_child = child;
    match fetch_daemon_status(settings.clone()).await {
        Ok(status) => {
            reconcile_supervised_version(
                settings,
                status.version,
                current_child,
                shutdown_daemon,
                wait_for_supervised_daemon_exit,
                launch_daemon_supervised,
                fetch_daemon_status,
            )
            .await
        }
        Err(error) => {
            tracing::error!("failed to fetch daemon status: {error}");
            SupervisedStartup {
                state: DaemonState::Unreachable,
                child: current_child,
                upgraded_daemon: false,
            }
        }
    }
}

async fn reconcile_supervised_version<
    C,
    Shutdown,
    ShutdownFuture,
    Wait,
    WaitFuture,
    Launch,
    LaunchFuture,
    Status,
    StatusFuture,
>(
    settings: DaemonSettings,
    running: String,
    child: Option<C>,
    shutdown: Shutdown,
    wait_for_exit: Wait,
    launch: Launch,
    fetch_status: Status,
) -> SupervisedStartup<C>
where
    Shutdown: Fn(DaemonSettings) -> ShutdownFuture,
    ShutdownFuture: Future<Output = Result<(), DynError>>,
    Wait: Fn(C, Duration) -> WaitFuture,
    WaitFuture: Future<Output = Result<(), DynError>>,
    Launch: Fn(DaemonSettings) -> LaunchFuture,
    LaunchFuture: Future<Output = Result<C, DynError>>,
    Status: Fn(DaemonSettings) -> StatusFuture,
    StatusFuture: Future<Output = Result<DaemonStatus, DynError>>,
{
    let bundled = env!("CARGO_PKG_VERSION").to_string();
    match compare_version_status(&running, &bundled) {
        VersionStatus::Match => SupervisedStartup {
            state: DaemonState::Ready,
            child,
            upgraded_daemon: false,
        },
        VersionStatus::Different => SupervisedStartup {
            state: DaemonState::VersionMismatch { running, bundled },
            child,
            upgraded_daemon: false,
        },
        VersionStatus::Outdated => {
            let Some(child) = child else {
                return SupervisedStartup {
                    state: DaemonState::VersionMismatch { running, bundled },
                    child: None,
                    upgraded_daemon: false,
                };
            };

            if let Err(error) = shutdown(settings.clone()).await {
                tracing::error!("failed to shut down outdated daemon: {error}");
                return SupervisedStartup {
                    state: DaemonState::VersionMismatch { running, bundled },
                    child: Some(child),
                    upgraded_daemon: false,
                };
            }

            if let Err(error) = wait_for_exit(child, settings.startup_wait).await {
                tracing::error!("failed waiting for outdated daemon to exit: {error}");
                return SupervisedStartup {
                    state: DaemonState::Unreachable,
                    child: None,
                    upgraded_daemon: false,
                };
            }

            let new_child = match launch(settings.clone()).await {
                Ok(child) => child,
                Err(error) => {
                    tracing::error!("failed to relaunch daemon after upgrade shutdown: {error}");
                    return SupervisedStartup {
                        state: DaemonState::Unreachable,
                        child: None,
                        upgraded_daemon: false,
                    };
                }
            };

            match wait_for_status(settings.clone(), &fetch_status).await {
                Ok(status) if status.version == bundled => SupervisedStartup {
                    state: DaemonState::Ready,
                    child: Some(new_child),
                    upgraded_daemon: true,
                },
                Ok(status) => SupervisedStartup {
                    state: DaemonState::VersionMismatch {
                        running: status.version,
                        bundled,
                    },
                    child: Some(new_child),
                    upgraded_daemon: false,
                },
                Err(error) => {
                    tracing::error!("relaunched daemon never became ready: {error}");
                    SupervisedStartup {
                        state: DaemonState::Unreachable,
                        child: Some(new_child),
                        upgraded_daemon: false,
                    }
                }
            }
        }
    }
}

async fn wait_for_status<Status, StatusFuture>(
    settings: DaemonSettings,
    fetch_status: &Status,
) -> Result<DaemonStatus, DynError>
where
    Status: Fn(DaemonSettings) -> StatusFuture,
    StatusFuture: Future<Output = Result<DaemonStatus, DynError>>,
{
    let deadline = Instant::now() + settings.startup_wait;

    loop {
        match fetch_status(settings.clone()).await {
            Ok(status) => return Ok(status),
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => {}
        }

        if Instant::now() >= deadline {
            return Err(io::Error::other("daemon status probe timed out").into());
        }

        tokio::time::sleep(settings.startup_poll_interval).await;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionStatus {
    Match,
    Outdated,
    Different,
}

pub struct SupervisedStartup<C = tokio::process::Child> {
    pub state: DaemonState,
    pub child: Option<C>,
    pub upgraded_daemon: bool,
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
        if !self.settings.external_url
            && let Some((settings, lockfile)) = self.read_lockfile_settings()
        {
            if self.wait_until_ready(settings.clone()).await {
                return self.check_version(settings).await;
            }

            return DaemonState::PortConflict { pid: lockfile.pid };
        }

        if (self.probe)(self.settings.clone()).await {
            return self.check_version(self.settings.clone()).await;
        }

        if self.settings.external_url {
            tracing::warn!(
                "external daemon at {} is unreachable",
                self.settings.daemon_url
            );
            return DaemonState::Unreachable;
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

pub async fn orchestrate_startup_supervised(settings: &DaemonSettings) -> SupervisedStartup {
    let settings = settings.clone();

    // When the active connection is a remote server, the local lockfile belongs
    // to a different process.  Skip the lockfile check and skip launching —
    // just probe the configured URL.
    if !settings.external_url {
        match read_active_lockfile() {
            Ok(Some(lockfile)) => {
                let settings = daemon_settings_for_lockfile(&settings, &lockfile);
                if wait_for_status_ready(settings.clone()).await {
                    return resolve_supervised_version(settings, None).await;
                }

                return SupervisedStartup {
                    state: DaemonState::PortConflict { pid: lockfile.pid },
                    child: None,
                    upgraded_daemon: false,
                };
            }
            Ok(None) => {}
            Err(error) => tracing::warn!("failed to read daemon lockfile: {error}"),
        }
    }

    if probe_daemon(settings.clone()).await {
        return resolve_supervised_version(settings, None).await;
    }

    if settings.external_url {
        tracing::warn!("external daemon at {} is unreachable", settings.daemon_url);
        return SupervisedStartup {
            state: DaemonState::Unreachable,
            child: None,
            upgraded_daemon: false,
        };
    }

    match launch_daemon_supervised(settings.clone()).await {
        Ok(mut child) => {
            if wait_for_status_ready(settings.clone()).await {
                resolve_supervised_version(settings, Some(child)).await
            } else {
                let _ = child.start_kill();
                let _ = child.wait().await;
                SupervisedStartup {
                    state: DaemonState::Unreachable,
                    child: None,
                    upgraded_daemon: false,
                }
            }
        }
        Err(error) => {
            tracing::error!("failed to launch notesmith daemon: {error}");
            SupervisedStartup {
                state: DaemonState::Unreachable,
                child: None,
                upgraded_daemon: false,
            }
        }
    }
}

pub fn resolve_daemon_settings(settings: &DaemonSettings) -> DaemonSettings {
    if settings.external_url {
        return settings.clone();
    }
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
    spawn_daemon(settings, true).await?;
    Ok(())
}

pub async fn launch_daemon_supervised(
    settings: DaemonSettings,
) -> Result<tokio::process::Child, DynError> {
    spawn_daemon(settings, false).await
}

pub async fn wait_for_daemon_status(settings: &DaemonSettings) -> bool {
    wait_for_status_ready(settings.clone()).await
}

async fn shutdown_daemon(settings: DaemonSettings) -> Result<(), DynError> {
    reqwest::Client::builder()
        .timeout(settings.ping_timeout)
        .build()?
        .post(format!(
            "{}/admin/shutdown",
            settings.daemon_url.trim_end_matches('/')
        ))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn wait_for_supervised_daemon_exit(
    mut child: tokio::process::Child,
    timeout: Duration,
) -> Result<(), DynError> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(_status)) => Ok(()),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => {
            child.start_kill()?;
            child.wait().await?;
            Ok(())
        }
    }
}

async fn spawn_daemon(
    settings: DaemonSettings,
    detach: bool,
) -> Result<tokio::process::Child, DynError> {
    let program = settings.program().to_string();
    tracing::info!("launching daemon: {program} {:?}", START_COMMAND);
    let mut command = tokio::process::Command::new(&program);
    command
        .args(START_COMMAND)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Point the local daemon's embed worker at the app-bundled model so enabling
    // embeddings is offline/instant (ADR 0018 §9.2, #256 Part B). Unset in
    // unbundled/dev builds, where the daemon falls back to downloading the model.
    if let Some(model_dir) = settings.model_dir.as_deref() {
        command.env("NOTESMITH_EMBED_MODEL_DIR", model_dir);
    }

    // Point the colocated transcription worker (spawned by the daemon's
    // transcribe scheduler, which inherits this environment) at the app-bundled
    // whisper model so enabling transcription is offline/instant (ADR 0023 §3).
    // Unset in unbundled/dev builds, where the worker downloads the model.
    if let Some(whisper_model_dir) = settings.whisper_model_dir.as_deref() {
        command.env("NOTESMITH_WHISPER_MODEL_DIR", whisper_model_dir);
    }

    #[cfg(unix)]
    if detach {
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

    command.spawn().map_err(Into::into)
}

async fn wait_for_status_ready(settings: DaemonSettings) -> bool {
    let deadline = Instant::now() + settings.startup_wait;
    loop {
        if fetch_daemon_status(settings.clone()).await.is_ok() {
            return true;
        }

        if Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(settings.startup_poll_interval).await;
    }
}

fn compare_version_status(running: &str, bundled: &str) -> VersionStatus {
    if running == bundled {
        return VersionStatus::Match;
    }

    match (
        parse_semver_components(running),
        parse_semver_components(bundled),
    ) {
        (Some(running), Some(bundled)) if running < bundled => VersionStatus::Outdated,
        _ => VersionStatus::Different,
    }
}

fn parse_semver_components(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version
        .split(['-', '+'])
        .next()?
        .split('.')
        .map(|part| part.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next()??;

    if parts.next().is_some() {
        return None;
    }

    Some((major, minor, patch))
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

    use super::{
        DaemonSettings, DaemonState, DaemonStatus, DaemonSupervisor, DynError, StartupOrchestrator,
        VersionStatus,
    };

    fn test_settings() -> DaemonSettings {
        DaemonSettings {
            daemon_url: "http://127.0.0.1:27183".into(),
            daemon_bin: "notesmith".into(),
            sidecar_path: None,
            model_dir: None,
            whisper_model_dir: None,
            ping_timeout: std::time::Duration::from_millis(5),
            startup_wait: std::time::Duration::from_millis(30),
            startup_poll_interval: std::time::Duration::from_millis(5),
            external_url: false,
        }
    }

    fn external_test_settings() -> DaemonSettings {
        DaemonSettings {
            daemon_url: "https://notesmith.example.com".into(),
            external_url: true,
            ..test_settings()
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

    #[test]
    fn compare_version_status_identifies_outdated_daemon() {
        assert_eq!(
            super::compare_version_status("0.0.1", env!("CARGO_PKG_VERSION")),
            VersionStatus::Outdated
        );
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
    async fn app_owned_outdated_daemon_is_restarted() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let launches = Arc::new(AtomicUsize::new(0));

        let outcome = super::reconcile_supervised_version(
            test_settings(),
            "0.0.1".to_string(),
            Some("old-child"),
            {
                let shutdowns = shutdowns.clone();
                move |_| {
                    let shutdowns = shutdowns.clone();
                    async move {
                        shutdowns.fetch_add(1, Ordering::SeqCst);
                        Ok::<(), DynError>(())
                    }
                }
            },
            {
                let waits = waits.clone();
                move |child, timeout| {
                    let waits = waits.clone();
                    async move {
                        waits.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(child, "old-child");
                        assert_eq!(timeout, test_settings().startup_wait);
                        Ok::<(), DynError>(())
                    }
                }
            },
            {
                let launches = launches.clone();
                move |_| {
                    let launches = launches.clone();
                    async move {
                        launches.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, DynError>("new-child")
                    }
                }
            },
            |_| async {
                Ok::<_, DynError>(DaemonStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
            },
        )
        .await;

        assert_eq!(outcome.state, DaemonState::Ready);
        assert_eq!(outcome.child, Some("new-child"));
        assert!(outcome.upgraded_daemon);
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(waits.load(Ordering::SeqCst), 1);
        assert_eq!(launches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn user_owned_outdated_daemon_returns_version_mismatch() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let launches = Arc::new(AtomicUsize::new(0));

        let outcome = super::reconcile_supervised_version(
            test_settings(),
            "0.0.1".to_string(),
            None::<&'static str>,
            {
                let shutdowns = shutdowns.clone();
                move |_| {
                    let shutdowns = shutdowns.clone();
                    async move {
                        shutdowns.fetch_add(1, Ordering::SeqCst);
                        Ok::<(), DynError>(())
                    }
                }
            },
            |_, _| async { Ok::<(), DynError>(()) },
            {
                let launches = launches.clone();
                move |_| {
                    let launches = launches.clone();
                    async move {
                        launches.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, DynError>("new-child")
                    }
                }
            },
            |_| async {
                Ok::<_, DynError>(DaemonStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
            },
        )
        .await;

        assert_eq!(
            outcome.state,
            DaemonState::VersionMismatch {
                running: "0.0.1".to_string(),
                bundled: env!("CARGO_PKG_VERSION").to_string(),
            }
        );
        assert!(outcome.child.is_none());
        assert!(!outcome.upgraded_daemon);
        assert_eq!(shutdowns.load(Ordering::SeqCst), 0);
        assert_eq!(launches.load(Ordering::SeqCst), 0);
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

    #[tokio::test]
    async fn orchestrate_startup_skips_lockfile_when_external_url_set() {
        // When the active connection is a remote server, a stale local lockfile
        // must not cause a PortConflict — the external daemon is unreachable
        // here, so we expect Unreachable (not PortConflict).
        let launches = Arc::new(AtomicUsize::new(0));
        let orchestrator = StartupOrchestrator::new(
            external_test_settings(),
            // Lockfile has a stale PID — should be ignored
            || Ok(Some(sample_lockfile(36794, 27183))),
            // External daemon is down
            |_| async { false },
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
        // Must NOT try to launch a local daemon when external_url is set
        assert_eq!(launches.load(Ordering::SeqCst), 0);
    }
}
