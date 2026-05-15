#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use notesmith_tauri::daemon::{self, DaemonSettings, DaemonState, DynError};
use tauri::{
    AppHandle, Manager, RunEvent, Runtime, UriSchemeContext, Url, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_notification::NotificationExt;
use tokio::process::Child;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

const MAIN_WINDOW_LABEL: &str = "main";
const SPLASH_WINDOW_LABEL: &str = "startup-splash";
const FALLBACK_WINDOW_LABEL: &str = "startup-fallback";
const TRAY_ID: &str = "notesmith-tray";
const MENU_OPEN: &str = "open";
const MENU_CAPTURE: &str = "capture";
const MENU_HIDE: &str = "hide";
const MENU_QUIT: &str = "quit";
const WAKE_EVENT_SCRIPT: &str = "window.dispatchEvent(new Event('notesmith://wake'));";
const DAEMON_CRASH_WINDOW: Duration = Duration::from_secs(60);
const DAEMON_CRASH_THRESHOLD: usize = 2;
const DAEMON_CRASH_LOG_LINES: usize = 200;

#[derive(Default)]
struct ExitState(AtomicBool);

struct DaemonUrlState(Mutex<String>);

struct DaemonProcessState(Mutex<DaemonProcessInner>);

#[derive(Default)]
struct DaemonProcessInner {
    child: Option<Child>,
    current_pid: Option<u32>,
    crash_tracker: CrashTracker,
    crash_report: Option<String>,
    expected_shutdown: bool,
    monitor_running: bool,
}

/// Stores dynamic HTML content served by the `notesmith-internal://` protocol.
/// The splash page is static, but fallback pages change per startup attempt.
struct InternalHtmlState(Mutex<InternalPages>);

struct InternalPages {
    fallback: Option<String>,
}

const INTERNAL_PROTOCOL: &str = "notesmith-internal";

#[derive(Debug, Clone, Copy)]
enum FallbackActionKind {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy)]
struct FallbackAction {
    label: &'static str,
    command: &'static str,
    kind: FallbackActionKind,
}

struct StartupFallbackView {
    title: String,
    message: String,
    actions: Vec<FallbackAction>,
    report_title: Option<&'static str>,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrashAction {
    Restart,
    ShowCrashLoop,
}

#[derive(Debug, Default)]
struct CrashTracker {
    recent_crashes: VecDeque<Instant>,
}

impl Default for DaemonProcessState {
    fn default() -> Self {
        Self(Mutex::new(DaemonProcessInner::default()))
    }
}

impl CrashTracker {
    fn record_crash(&mut self, now: Instant, window: Duration, threshold: usize) -> CrashAction {
        while self
            .recent_crashes
            .front()
            .is_some_and(|crash| now.duration_since(*crash) > window)
        {
            self.recent_crashes.pop_front();
        }

        self.recent_crashes.push_back(now);
        if self.recent_crashes.len() >= threshold {
            CrashAction::ShowCrashLoop
        } else {
            CrashAction::Restart
        }
    }

    fn reset(&mut self) {
        self.recent_crashes.clear();
    }
}

impl FallbackAction {
    const fn primary(label: &'static str, command: &'static str) -> Self {
        Self {
            label,
            command,
            kind: FallbackActionKind::Primary,
        }
    }

    const fn secondary(label: &'static str, command: &'static str) -> Self {
        Self {
            label,
            command,
            kind: FallbackActionKind::Secondary,
        }
    }
}

impl StartupFallbackView {
    fn startup(
        title: impl Into<String>,
        message: impl Into<String>,
        primary_label: &'static str,
        primary_command: &'static str,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            actions: vec![
                FallbackAction::primary(primary_label, primary_command),
                FallbackAction::secondary("Open Diagnostics", "open_diagnostics"),
                FallbackAction::secondary("Quit", "quit_app"),
            ],
            report_title: None,
            width: 480.0,
            height: 320.0,
        }
    }

    fn crash_loop(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            actions: vec![
                FallbackAction::secondary("View Error Report", "view_crash_report"),
                FallbackAction::primary("Restart Anyway", "restart_daemon_anyway"),
                FallbackAction::secondary("Quit", "quit_app"),
            ],
            report_title: Some("Crash report"),
            width: 720.0,
            height: 560.0,
        }
    }
}

impl Default for DaemonUrlState {
    fn default() -> Self {
        Self(Mutex::new(DaemonSettings::default().daemon_url))
    }
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(ExitState::default())
        .manage(DaemonUrlState::default())
        .manage(DaemonProcessState::default())
        .manage(InternalHtmlState(Mutex::new(InternalPages {
            fallback: None,
        })))
        .register_uri_scheme_protocol(INTERNAL_PROTOCOL, handle_internal_protocol)
        .enable_macos_default_menu(false)
        .invoke_handler(tauri::generate_handler![
            retry_daemon_connect,
            open_diagnostics,
            quit_app,
            restart_app,
            view_crash_report,
            restart_daemon_anyway
        ])
        .menu(build_app_menu)
        .on_menu_event(|app, event| {
            if let Err(error) = handle_menu_event(app, event.id().as_ref()) {
                tracing::error!("menu action failed: {error}");
            }
        })
        .on_tray_icon_event(|app, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
                && let Err(error) = show_main_window(app)
            {
                tracing::error!("tray click failed: {error}");
            }
        })
        .on_window_event(|window, event| {
            if window.label() == MAIN_WINDOW_LABEL
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            initialize_app(app).map_err(|error| -> Box<dyn std::error::Error> { error })?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. }
            if !app_handle.state::<ExitState>().0.load(Ordering::SeqCst) =>
        {
            api.prevent_exit();
        }
        RunEvent::Resumed => emit_wake_event(app_handle),
        _ => {}
    });
}

fn initialize_app(app: &tauri::App) -> Result<(), DynError> {
    show_splash_window(app.handle())?;
    setup_tray(app.handle())?;
    setup_deep_links(app.handle())?;
    tauri::async_runtime::block_on(run_startup_flow(app.handle()))?;
    Ok(())
}

/// Resolve the bundled notesmith sidecar binary path.
///
/// In a packaged app, the sidecar lives next to the main executable with a
/// target-triple suffix (e.g. `notesmith-aarch64-apple-darwin`). In dev mode
/// the sidecar won't exist, so we return `None` and fall back to `PATH`.
fn resolve_sidecar_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    let target_triple = option_env!("TAURI_ENV_TARGET_TRIPLE")
        .or(option_env!("TARGET"))
        .unwrap_or(env!("TARGET_TRIPLE"));

    let extension = if cfg!(windows) { ".exe" } else { "" };
    let sidecar = exe_dir.join(format!("notesmith-{target_triple}{extension}"));

    if sidecar.exists() {
        tracing::info!("resolved sidecar: {}", sidecar.display());
        Some(sidecar)
    } else {
        tracing::info!(
            "sidecar not found at {}; falling back to PATH",
            sidecar.display()
        );
        None
    }
}

fn setup_deep_links<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        let urls = event.urls();
        for url in urls {
            let url_str = url.as_str();
            tracing::info!("deep link received: {url_str}");
            match notesmith_core::url_scheme::parse_notesmith_url(url_str) {
                Ok(parsed) => handle_deep_link(&handle, parsed),
                Err(error) => tracing::error!("failed to parse deep link: {error}"),
            }
        }
    });
    Ok(())
}

fn handle_deep_link<R: Runtime>(app: &AppHandle<R>, parsed: notesmith_core::NotesmithUrl) {
    use notesmith_core::NotesmithUrl;

    // Ensure the main window is visible before navigating
    if let Err(error) = show_main_window(app) {
        tracing::error!("failed to show main window for deep link: {error}");
        return;
    }

    let daemon_base = current_daemon_url(app);

    match parsed {
        NotesmithUrl::Open { vault, path } => {
            navigate_webview(app, &format!("/vault/{vault}/note/{path}"));
        }
        NotesmithUrl::Daily { vault } => {
            navigate_webview(app, &format!("/vault/{vault}/daily"));
        }
        NotesmithUrl::Search { vault, query } => {
            navigate_webview(app, &format!("/vault/{vault}/search?q={query}"));
        }
        NotesmithUrl::New {
            vault,
            template,
            folder,
        } => {
            let mut route = format!("/vault/{vault}/new");
            let mut params = Vec::new();
            if let Some(t) = template {
                params.push(format!("template={t}"));
            }
            if let Some(f) = folder {
                params.push(format!("folder={f}"));
            }
            if !params.is_empty() {
                route.push('?');
                route.push_str(&params.join("&"));
            }
            navigate_webview(app, &route);
        }
        NotesmithUrl::Capture { vault, text } => {
            let url = format!("{daemon_base}/api/v/{vault}/capture");
            let body = serde_json::json!({ "text": text });
            tauri::async_runtime::spawn(async move {
                match reqwest::Client::new().post(&url).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!("capture successful");
                    }
                    Ok(resp) => {
                        tracing::error!("capture failed: {}", resp.status());
                    }
                    Err(error) => tracing::error!("capture request failed: {error}"),
                }
            });
        }
        NotesmithUrl::Task {
            vault,
            path,
            line_hash,
            status,
        } => {
            let url = format!("{daemon_base}/api/v/{vault}/tasks/toggle");
            let body = serde_json::json!({
                "path": path,
                "line_hash": line_hash,
                "status": status,
            });
            tauri::async_runtime::spawn(async move {
                match reqwest::Client::new().post(&url).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!("task toggle successful");
                    }
                    Ok(resp) => {
                        tracing::error!("task toggle failed: {}", resp.status());
                    }
                    Err(error) => tracing::error!("task toggle request failed: {error}"),
                }
            });
        }
        NotesmithUrl::Command { command_name, .. } => {
            navigate_webview(app, &format!("/command/{command_name}"));
        }
        NotesmithUrl::UserAction {
            action_name,
            params,
        } => {
            tracing::info!("user action: {action_name} (params: {params:?})");
            // User actions are best handled via the CLI; log and navigate to a status page
            navigate_webview(app, &format!("/action/{action_name}"));
        }
    }
}

fn navigate_webview<R: Runtime>(app: &AppHandle<R>, route: &str) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let script = format!("window.location.hash = '{}';", route.replace('\'', "\\'"));
        if let Err(error) = window.eval(&script) {
            tracing::error!("failed to navigate webview: {error}");
        }
    }
}

fn emit_wake_event<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL)
        && let Err(error) = window.eval(WAKE_EVENT_SCRIPT)
    {
        tracing::error!("failed to emit wake event: {error}");
    }
}

fn build_app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, MENU_OPEN, "Open Notesmith", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, MENU_CAPTURE, "Quick Capture", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, MENU_HIDE, "Hide Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, Some("CmdOrCtrl+Q"))?;
    let separator = PredefinedMenuItem::separator(app)?;
    let copy = PredefinedMenuItem::copy(app, None::<&str>)?;
    let paste = PredefinedMenuItem::paste(app, None::<&str>)?;
    let select_all = PredefinedMenuItem::select_all(app, None::<&str>)?;

    let app_submenu = Submenu::with_items(
        app,
        "Notesmith",
        true,
        &[&open, &capture, &separator, &hide, &quit],
    )?;
    let edit_submenu = Submenu::with_items(app, "Edit", true, &[&copy, &paste, &select_all])?;

    Menu::with_items(app, &[&app_submenu, &edit_submenu])
}

fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    let open = MenuItem::with_id(app, MENU_OPEN, "Open Notesmith", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, MENU_CAPTURE, "Quick Capture", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)?;
    let tray_menu = Menu::with_items(app, &[&open, &capture, &quit])?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&tray_menu)
        .tooltip("Notesmith")
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), DynError> {
    match id {
        MENU_OPEN | MENU_CAPTURE => show_main_window(app),
        MENU_HIDE => hide_main_window(app),
        MENU_QUIT => {
            request_exit(app);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    if let Some(window) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    if let Some(window) = app.get_webview_window(FALLBACK_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    ensure_main_window(app)?;

    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    Ok(())
}

fn hide_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.hide()?;
    }

    Ok(())
}

fn ensure_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    let app_url = current_app_url(app)?;

    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        if window.url()?.as_str() != app_url.as_str() {
            window.navigate(app_url)?;
        }
        return Ok(());
    }

    let mut window_config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .cloned()
        .ok_or_else(|| std::io::Error::other("missing main window config"))?;

    window_config.url = WebviewUrl::External(app_url);
    WebviewWindowBuilder::from_config(app, &window_config)?.build()?;
    Ok(())
}

fn request_exit<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(mut state) = app.state::<DaemonProcessState>().0.try_lock() {
        state.expected_shutdown = true;
    }
    app.state::<ExitState>().0.store(true, Ordering::SeqCst);
    app.exit(0);
}

fn startup_settings() -> DaemonSettings {
    DaemonSettings {
        sidecar_path: resolve_sidecar_path(),
        ..Default::default()
    }
}

async fn run_startup_flow(app: &tauri::AppHandle) -> Result<String, DynError> {
    show_splash_window(app)?;
    close_window(app, FALLBACK_WINDOW_LABEL)?;

    let settings = startup_settings();
    let outcome = daemon::orchestrate_startup_supervised(&settings).await;
    handle_startup_state(app, &settings, outcome).await
}

async fn handle_startup_state(
    app: &tauri::AppHandle,
    settings: &DaemonSettings,
    outcome: daemon::SupervisedStartup,
) -> Result<String, DynError> {
    close_window(app, SPLASH_WINDOW_LABEL)?;

    if let Some(child) = outcome.child {
        register_supervised_child(app.clone(), child);
    }

    match outcome.state {
        DaemonState::Ready => {
            close_window(app, FALLBACK_WINDOW_LABEL)?;
            set_current_daemon_url(app, daemon::resolve_daemon_url(settings));
            // Call ensure_main_window directly rather than show_main_window.
            // show_main_window has splash/fallback guards that check the window
            // manager, but destroy() dispatches asynchronously to the platform,
            // so the splash may still be registered when checked.
            ensure_main_window(app)?;
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            Ok("Notesmith is ready".to_string())
        }
        DaemonState::VersionMismatch { running, bundled } => {
            hide_main_window(app)?;
            show_fallback_window(
                app,
                StartupFallbackView::startup(
                    "Restart to finish updating?",
                    format!(
                        "Notesmith found daemon version {running}, but this app bundles {bundled}. Restart the desktop app to finish the update."
                    ),
                    "Restart App",
                    "restart_app",
                ),
            )?;
            Ok("Notesmith needs a restart".to_string())
        }
        DaemonState::Unreachable => {
            hide_main_window(app)?;
            show_fallback_window(
                app,
                StartupFallbackView::startup(
                    "Could not connect to Notesmith daemon",
                    "Notesmith couldn't start its background service. Retry or open diagnostics for more details.",
                    "Retry",
                    "retry_daemon_connect",
                ),
            )?;
            Ok("Notesmith daemon is unreachable".to_string())
        }
        DaemonState::PortConflict { pid } => {
            hide_main_window(app)?;
            show_fallback_window(
                app,
                StartupFallbackView::startup(
                    "Another Notesmith daemon is blocking startup",
                    format!(
                        "Notesmith found a running daemon process (PID {pid}) that never became ready. Quit it or open diagnostics before retrying."
                    ),
                    "Retry",
                    "retry_daemon_connect",
                ),
            )?;
            Ok(format!("Notesmith daemon PID {pid} is blocking startup"))
        }
    }
}

fn show_splash_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    if app.get_webview_window(SPLASH_WINDOW_LABEL).is_none() {
        WebviewWindowBuilder::new(app, SPLASH_WINDOW_LABEL, internal_url("/splash"))
            .title("Notesmith")
            .inner_size(320.0, 220.0)
            .resizable(false)
            .decorations(false)
            .center()
            .visible(true)
            .skip_taskbar(true)
            .build()?;
    }

    if let Some(window) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }

    Ok(())
}

fn show_fallback_window<R: Runtime>(
    app: &AppHandle<R>,
    view: StartupFallbackView,
) -> Result<(), DynError> {
    close_window(app, FALLBACK_WINDOW_LABEL)?;
    let width = view.width;
    let height = view.height;
    let resizable = view.report_title.is_some();

    // Store fallback HTML in managed state so the protocol handler can serve it
    app.state::<InternalHtmlState>()
        .0
        .lock()
        .expect("internal html state poisoned")
        .fallback = Some(fallback_html(&view));

    WebviewWindowBuilder::new(app, FALLBACK_WINDOW_LABEL, internal_url("/fallback"))
        .title("Notesmith")
        .inner_size(width, height)
        .resizable(resizable)
        .center()
        .skip_taskbar(true)
        .build()?;

    Ok(())
}

fn close_window<R: Runtime>(app: &AppHandle<R>, label: &str) -> Result<(), DynError> {
    if let Some(window) = app.get_webview_window(label) {
        // Use destroy() instead of close() to force-remove the window from the
        // manager synchronously. close() dispatches an event that requires the
        // event loop, which isn't running during block_on in setup().
        window.destroy()?;
    }

    Ok(())
}

fn current_daemon_url<R: Runtime>(app: &AppHandle<R>) -> String {
    app.state::<DaemonUrlState>()
        .0
        .lock()
        .expect("daemon url state poisoned")
        .clone()
}

fn set_current_daemon_url<R: Runtime>(app: &AppHandle<R>, daemon_url: String) {
    *app.state::<DaemonUrlState>()
        .0
        .lock()
        .expect("daemon url state poisoned") = daemon_url;
}

fn current_app_url<R: Runtime>(app: &AppHandle<R>) -> Result<Url, DynError> {
    Url::parse(&format!(
        "{}/app/",
        current_daemon_url(app).trim_end_matches('/')
    ))
    .map_err(Into::into)
}

fn register_supervised_child(app: tauri::AppHandle, child: Child) {
    let pid = child.id();
    let should_spawn_monitor = {
        let daemon_process = app.state::<DaemonProcessState>();
        let mut state = daemon_process
            .0
            .lock()
            .expect("daemon process state poisoned");
        state.child = Some(child);
        state.current_pid = pid;
        state.crash_report = None;
        state.expected_shutdown = false;
        if state.monitor_running {
            false
        } else {
            state.monitor_running = true;
            true
        }
    };

    if should_spawn_monitor {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            monitor_daemon_process(handle).await;
        });
    }
}

async fn start_and_track_supervised_daemon(app: tauri::AppHandle) -> Result<(), DynError> {
    let settings = startup_settings();
    let mut child = daemon::launch_daemon_supervised(settings.clone()).await?;

    if !daemon::wait_for_daemon_status(&settings).await {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Err(io::Error::other(format!(
            "notesmith daemon failed to become ready within {:?}",
            settings.startup_wait
        ))
        .into());
    }

    set_current_daemon_url(&app, daemon::resolve_daemon_url(&settings));
    register_supervised_child(app, child);
    Ok(())
}

async fn monitor_daemon_process(app: tauri::AppHandle) {
    loop {
        let mut child = {
            let daemon_process = app.state::<DaemonProcessState>();
            let mut state = daemon_process
                .0
                .lock()
                .expect("daemon process state poisoned");
            match state.child.take() {
                Some(child) => child,
                None => {
                    state.monitor_running = false;
                    return;
                }
            }
        };

        let exit_status = match child.wait().await {
            Ok(status) => status,
            Err(error) => {
                tracing::error!("failed waiting for daemon process: {error}");
                let crash_report =
                    build_crash_report(&format!("failed while waiting for process exit: {error}"));
                if let Err(show_error) =
                    record_crash_and_handle(app.clone(), crash_report, None).await
                {
                    tracing::error!("failed to handle daemon monitor error: {show_error}");
                }
                return;
            }
        };

        let expected_shutdown = {
            let daemon_process = app.state::<DaemonProcessState>();
            let mut state = daemon_process
                .0
                .lock()
                .expect("daemon process state poisoned");
            state.current_pid = None;
            let expected_shutdown = state.expected_shutdown;
            state.expected_shutdown = false;
            expected_shutdown
        };

        if app.state::<ExitState>().0.load(Ordering::SeqCst) {
            let daemon_process = app.state::<DaemonProcessState>();
            let mut state = daemon_process
                .0
                .lock()
                .expect("daemon process state poisoned");
            state.monitor_running = false;
            return;
        }

        if expected_shutdown || exit_status.success() {
            tracing::info!("notesmith daemon exited cleanly; supervisor will not restart it");
            let daemon_process = app.state::<DaemonProcessState>();
            let mut state = daemon_process
                .0
                .lock()
                .expect("daemon process state poisoned");
            state.monitor_running = false;
            return;
        }

        let crash_report = build_crash_report(&format_exit_summary(&exit_status));
        if let Err(error) =
            record_crash_and_handle(app.clone(), crash_report, Some(&exit_status)).await
        {
            tracing::error!("failed to handle daemon crash: {error}");
            let daemon_process = app.state::<DaemonProcessState>();
            let mut state = daemon_process
                .0
                .lock()
                .expect("daemon process state poisoned");
            state.monitor_running = false;
            return;
        }
    }
}

async fn record_crash_and_handle(
    app: tauri::AppHandle,
    crash_report: String,
    exit_status: Option<&std::process::ExitStatus>,
) -> Result<(), DynError> {
    let action = {
        let daemon_process = app.state::<DaemonProcessState>();
        let mut state = daemon_process
            .0
            .lock()
            .expect("daemon process state poisoned");
        state.crash_report = Some(crash_report.clone());
        state.crash_tracker.record_crash(
            Instant::now(),
            DAEMON_CRASH_WINDOW,
            DAEMON_CRASH_THRESHOLD,
        )
    };

    match action {
        CrashAction::Restart => {
            show_daemon_restart_notification(&app);

            if let Err(error) = start_and_track_supervised_daemon(app.clone()).await {
                let crash_report = build_crash_report_with_restart_failure(&error.to_string());
                let daemon_process = app.state::<DaemonProcessState>();
                let mut state = daemon_process
                    .0
                    .lock()
                    .expect("daemon process state poisoned");
                state.crash_report = Some(crash_report);
                state.crash_tracker.record_crash(
                    Instant::now(),
                    DAEMON_CRASH_WINDOW,
                    DAEMON_CRASH_THRESHOLD,
                );
                state.monitor_running = false;
                drop(state);

                tracing::error!("automatic daemon restart failed: {error}");
                hide_main_window(&app)?;
                show_fallback_window(
                    &app,
                    StartupFallbackView::crash_loop(
                        "Notesmith service keeps stopping",
                        "Notesmith tried to restart its background service, but it crashed twice within a minute. Review the crash report, restart anyway, or quit.",
                    ),
                )?;
                return Ok(());
            }

            if let Some(status) = exit_status {
                tracing::warn!(
                    "restarted daemon after unexpected exit: {}",
                    format_exit_summary(status)
                );
            } else {
                tracing::warn!("restarted daemon after monitor error");
            }

            Ok(())
        }
        CrashAction::ShowCrashLoop => {
            let daemon_process = app.state::<DaemonProcessState>();
            let mut state = daemon_process
                .0
                .lock()
                .expect("daemon process state poisoned");
            state.monitor_running = false;
            drop(state);

            hide_main_window(&app)?;
            show_fallback_window(
                &app,
                StartupFallbackView::crash_loop(
                    "Notesmith service keeps stopping",
                    "Notesmith detected a crash loop in its background service. Review the crash report, restart anyway, or quit.",
                ),
            )?;
            Ok(())
        }
    }
}

fn show_daemon_restart_notification<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app
        .notification()
        .builder()
        .title("Notesmith")
        .body("Notesmith service stopped unexpectedly. Restarting...")
        .show()
    {
        tracing::warn!("failed to show daemon restart notification: {error}");
    }
}

fn build_crash_report(exit_summary: &str) -> String {
    let log_path = daemon_log_file_path();
    let log_location = log_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unavailable".to_string());
    let log_tail = log_path
        .as_ref()
        .map(|path| read_last_log_lines(path, DAEMON_CRASH_LOG_LINES))
        .transpose()
        .map(|contents| contents.unwrap_or_else(|| "No recent daemon logs captured.\n".to_string()))
        .unwrap_or_else(|error| format!("Failed to read daemon log: {error}\n"));

    format!(
        "Notesmith daemon crash report\n\nExit: {exit_summary}\nLog file: {log_location}\n\nLast {DAEMON_CRASH_LOG_LINES} log lines:\n{log_tail}"
    )
}

fn build_crash_report_with_restart_failure(error: &str) -> String {
    let mut report = build_crash_report("automatic restart failed");
    report.push_str("\nAutomatic restart error:\n");
    report.push_str(error);
    report.push('\n');
    report
}

fn read_last_log_lines(path: &Path, max_lines: usize) -> Result<String, io::Error> {
    let reader = BufReader::new(File::open(path)?);
    let mut lines = VecDeque::with_capacity(max_lines);

    for line in reader.lines() {
        let line = line?;
        if max_lines == 0 {
            continue;
        }
        if lines.len() == max_lines {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    let mut body = lines.into_iter().collect::<Vec<_>>().join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    Ok(body)
}

fn format_exit_summary(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("process exited with code {code}");
    }

    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        return format!("process terminated by signal {signal}");
    }

    "process exited for an unknown reason".to_string()
}

fn splash_html() -> String {
    r#"<!doctype html>
<html>
  <body style="margin:0;background:#1a1a2e;color:white;display:flex;align-items:center;justify-content:center;height:100vh;font-family:system-ui">
    <div style="text-align:center">
      <h2 style="margin:0 0 8px">Notesmith</h2>
      <p style="margin:0;color:#cbd5e1">Starting...</p>
      <div style="width:40px;height:40px;border:3px solid #333;border-top:3px solid #fff;border-radius:50%;animation:spin 1s linear infinite;margin:20px auto"></div>
    </div>
    <style>@keyframes spin{to{transform:rotate(360deg)}}</style>
  </body>
</html>"#
        .to_string()
}

fn fallback_html(view: &StartupFallbackView) -> String {
    let actions = view
        .actions
        .iter()
        .map(|action| {
            let style = match action.kind {
                FallbackActionKind::Primary => "#2563eb",
                FallbackActionKind::Secondary => "#334155",
            };
            format!(
                r#"<button onclick="runCommand('{}')" style="border:0;border-radius:10px;padding:10px 16px;background:{};color:white;font:inherit;cursor:pointer">{}</button>"#,
                action.command,
                style,
                escape_html(action.label)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let report_panel = view.report_title.map(|title| {
        format!(
            r#"<section id="report-panel" hidden style="margin-top:18px;border-radius:12px;background:#0f172a;border:1px solid #334155;padding:16px">
        <h3 style="margin:0 0 12px;font-size:16px">{}</h3>
        <pre id="report" style="margin:0;max-height:240px;overflow:auto;white-space:pre-wrap;word-break:break-word;color:#cbd5e1;font-family:ui-monospace,SFMono-Regular,Menlo,monospace"></pre>
      </section>"#,
            escape_html(title)
        )
    }).unwrap_or_default();

    format!(
        r#"<!doctype html>
<html>
  <body style="margin:0;background:#0f172a;color:#e2e8f0;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh">
    <main style="width:min(680px,calc(100vw - 48px));padding:28px;border-radius:18px;background:#111827;box-shadow:0 16px 40px rgba(15,23,42,.45)">
      <h2 style="margin:0 0 12px;font-size:24px">{}</h2>
      <p id="message" style="margin:0 0 24px;line-height:1.5;color:#cbd5e1">{}</p>
      <div style="display:flex;gap:12px;flex-wrap:wrap">{}</div>
      <p id="status" style="margin:18px 0 0;color:#93c5fd;min-height:1.5em"></p>
      {}
    </main>
    <script>
      async function runCommand(command) {{
        const status = document.getElementById('status');
        try {{
          if (command !== 'view_crash_report') {{
            status.textContent = 'Working...';
          }}
          const result = await window.__TAURI_INTERNALS__.invoke(command);
          if (command === 'view_crash_report') {{
            const panel = document.getElementById('report-panel');
            const report = document.getElementById('report');
            if (panel && report) {{
              report.textContent = typeof result === 'string' ? result : '';
              panel.hidden = false;
              status.textContent = '';
            }}
            return;
          }}
          status.textContent = typeof result === 'string' ? result : '';
        }} catch (error) {{
          status.textContent = String(error);
        }}
      }}
    </script>
  </body>
</html>"#,
        escape_html(&view.title),
        escape_html(&view.message),
        actions,
        report_panel
    )
}

fn internal_url(path: &str) -> WebviewUrl {
    WebviewUrl::CustomProtocol(
        Url::parse(&format!("{INTERNAL_PROTOCOL}://localhost{path}"))
            .expect("internal protocol URL must parse"),
    )
}

fn handle_internal_protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Cow<'static, [u8]>> {
    let path = request.uri().path();
    let content_type = "text/html; charset=utf-8";

    match path {
        "/splash" => tauri::http::Response::builder()
            .header("Content-Type", content_type)
            .body(Cow::Owned(splash_html().into_bytes()))
            .expect("splash response must build"),

        "/fallback" => {
            let html = ctx
                .app_handle()
                .state::<InternalHtmlState>()
                .0
                .lock()
                .expect("internal html state poisoned")
                .fallback
                .clone()
                .unwrap_or_else(|| "No fallback content".to_string());

            tauri::http::Response::builder()
                .header("Content-Type", content_type)
                .body(Cow::Owned(html.into_bytes()))
                .expect("fallback response must build")
        }

        _ => tauri::http::Response::builder()
            .status(404)
            .body(Cow::Borrowed(b"Not found" as &[u8]))
            .expect("404 response must build"),
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn diagnostics_target() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(log_file) = daemon_log_file_path() {
        candidates.push(log_file.clone());
        if let Some(parent) = log_file.parent() {
            candidates.push(parent.to_path_buf());
        }
    }

    if let Some(lockfile_path) = notesmith_config::DaemonLockfile::path() {
        candidates.push(lockfile_path.clone());
        if let Some(parent) = lockfile_path.parent() {
            candidates.push(parent.to_path_buf());
        }
    }

    candidates.into_iter().find(|path| path.exists())
}

fn daemon_log_file_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Logs/Notesmith/daemon.log"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .map(|dir| dir.join("notesmith").join("daemon.log"))
    }
}

fn open_with_system_app(path: &Path) -> Result<(), DynError> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::other("opening diagnostics is not supported on this platform").into())
}

#[tauri::command]
async fn retry_daemon_connect(app: tauri::AppHandle) -> Result<String, String> {
    run_startup_flow(&app)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn open_diagnostics() -> Result<(), String> {
    let path = diagnostics_target()
        .ok_or_else(|| "Could not find Notesmith diagnostics on disk".to_string())?;
    open_with_system_app(&path).map_err(|error| error.to_string())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    request_exit(&app);
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    if let Ok(mut state) = app.state::<DaemonProcessState>().0.try_lock() {
        state.expected_shutdown = true;
    }
    app.state::<ExitState>().0.store(true, Ordering::SeqCst);
    app.restart();
}

#[tauri::command]
async fn view_crash_report(app: tauri::AppHandle) -> Result<String, String> {
    app.state::<DaemonProcessState>()
        .0
        .lock()
        .expect("daemon process state poisoned")
        .crash_report
        .clone()
        .ok_or_else(|| "No crash report is available".to_string())
}

#[tauri::command]
async fn restart_daemon_anyway(app: tauri::AppHandle) -> Result<String, String> {
    {
        let daemon_process = app.state::<DaemonProcessState>();
        let mut state = daemon_process
            .0
            .lock()
            .expect("daemon process state poisoned");
        if state.current_pid.is_some() {
            return Ok("Notesmith service is already running".to_string());
        }
        state.crash_tracker.reset();
        state.crash_report = None;
        state.expected_shutdown = false;
        state.monitor_running = false;
    }

    start_and_track_supervised_daemon(app.clone())
        .await
        .map_err(|error| error.to_string())?;
    close_window(&app, FALLBACK_WINDOW_LABEL).map_err(|error| error.to_string())?;
    ensure_main_window(&app).map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    Ok("Notesmith service restarted".to_string())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CrashAction, CrashTracker};

    #[test]
    fn first_crash_restarts_daemon() {
        let now = Instant::now();
        let mut tracker = CrashTracker::default();

        assert_eq!(
            tracker.record_crash(now, Duration::from_secs(60), 2),
            CrashAction::Restart
        );
    }

    #[test]
    fn second_crash_within_window_stops_restart_loop() {
        let now = Instant::now();
        let mut tracker = CrashTracker::default();

        tracker.record_crash(now, Duration::from_secs(60), 2);

        assert_eq!(
            tracker.record_crash(now + Duration::from_secs(30), Duration::from_secs(60), 2),
            CrashAction::ShowCrashLoop
        );
    }

    #[test]
    fn old_crashes_expire_outside_window() {
        let now = Instant::now();
        let mut tracker = CrashTracker::default();

        tracker.record_crash(now, Duration::from_secs(60), 2);

        assert_eq!(
            tracker.record_crash(now + Duration::from_secs(61), Duration::from_secs(60), 2),
            CrashAction::Restart
        );
    }
}
