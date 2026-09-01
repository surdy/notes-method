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
use std::time::{Duration, Instant, SystemTime};

use notesmith_tauri::app_url::{
    APP_PROTOCOL, app_asset_path, connection_window_url, should_fallback_to_index,
};
use notesmith_tauri::daemon::{self, DaemonSettings, DaemonState, DynError};
use notesmith_tauri::new_window_menu::{
    GroupStatus, MenuRow, ServerGroup, VaultSource, build_new_window_rows,
};
use notesmith_tauri::servers::{
    self, ConnectionList, ConnectionTestResult, ServerInput, ServerView, ServersFile,
};
use notesmith_tauri::vault_cache::{RefreshOutcome, VaultCache};
use notesmith_tauri::vault_menu::{
    OPEN_FOLDER_AS_VAULT_ID, decode_open_vault_target, validate_vault_display_name,
};
use notesmith_tauri::vault_window::{VaultKey, is_vault_window_label, vault_window_label_for_key};
use notesmith_tauri::window_registry::{WindowContext, WindowRegistry};
use notesmith_tauri::windows_persist::{
    self, Rect, WindowEntry, WindowsFile, dedupe_latest_per_vault,
};
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, Runtime, UriSchemeContext, Url, WebviewUrl,
    WebviewWindowBuilder,
    menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_notification::NotificationExt;
use tokio::process::Child;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

mod agent_bridge;
mod agent_diag;
mod agent_path;

const MAIN_WINDOW_LABEL: &str = "main";
const SETTINGS_WINDOW_LABEL: &str = "settings";
const SPLASH_WINDOW_LABEL: &str = "startup-splash";
const FALLBACK_WINDOW_LABEL: &str = "startup-fallback";
const TRAY_ID: &str = "notesmith-tray";
const MENU_OPEN: &str = "open";
const MENU_HIDE: &str = "hide";
const MENU_QUIT: &str = "quit";
const MENU_SETTINGS: &str = "settings";
const MENU_RESTART_SERVICE: &str = "restart-service";
const MENU_STOP_SERVICE: &str = "stop-service";
const MENU_VIEW_LOGS: &str = "view-logs";
const MENU_QUIT_APP: &str = "quit-app";
const WAKE_EVENT_SCRIPT: &str = "window.dispatchEvent(new Event('notesmith://wake'));";
const DAEMON_CRASH_WINDOW: Duration = Duration::from_secs(60);
const DAEMON_CRASH_THRESHOLD: usize = 2;
const DAEMON_CRASH_LOG_LINES: usize = 200;
const QUIT_CONFIRM_WINDOW: Duration = Duration::from_secs(5);

/// Debounce window-geometry writes so a drag doesn't trigger hundreds of
/// `windows.json` rewrites.
const WINDOWS_PERSIST_DEBOUNCE: Duration = Duration::from_millis(500);

/// CLI flag that suppresses `windows.json` replay for one launch.
const NO_RESTORE_FLAG: &str = "--no-restore";

#[derive(Default)]
struct ExitState(AtomicBool);

#[derive(Default)]
struct LastQuitAttempt(Mutex<Option<Instant>>);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitRequestAction {
    HideWindows,
    ArmExit,
    StopDaemonAndExit,
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

fn evaluate_quit_request(
    has_visible_window: bool,
    last_attempt: Option<Instant>,
    now: Instant,
) -> (QuitRequestAction, Option<Instant>) {
    if has_visible_window {
        return (QuitRequestAction::HideWindows, Some(now));
    }

    if last_attempt.is_some_and(|attempt| now.duration_since(attempt) < QUIT_CONFIRM_WINDOW) {
        (QuitRequestAction::StopDaemonAndExit, None)
    } else {
        (QuitRequestAction::ArmExit, Some(now))
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

/// Loaded saved-server list plus its on-disk path. The desktop's source of
/// truth for connections, shared by the Settings → Connection UI and the
/// status-bar switcher. Mutations persist immediately to `servers.json`.
#[derive(Default)]
struct ServersState(Mutex<ServersStateInner>);

#[derive(Default)]
struct ServersStateInner {
    file: ServersFile,
    path: Option<PathBuf>,
}

impl ServersState {
    /// Point the state at `servers.json` and load it (tolerant of a missing or
    /// corrupt file — see [`servers::load`]).
    fn set_path_and_load(&self, path: PathBuf) {
        let file = servers::load(&path);
        let mut guard = self.0.lock().expect("servers state poisoned");
        guard.file = file;
        guard.path = Some(path);
    }

    /// A cloned snapshot of the current server set.
    fn snapshot(&self) -> ServersFile {
        self.0.lock().expect("servers state poisoned").file.clone()
    }

    /// Apply `f` to the server set, persist the result to disk, and return
    /// whatever `f` produced. A failed write is logged but not fatal.
    fn mutate<T>(&self, f: impl FnOnce(&mut ServersFile) -> T) -> T {
        let mut guard = self.0.lock().expect("servers state poisoned");
        let out = f(&mut guard.file);
        if let Some(path) = guard.path.clone()
            && let Err(error) = servers::save(&path, &guard.file)
        {
            tracing::warn!(%error, "failed to persist servers.json");
        }
        out
    }
}

/// Managed wrapper around the pure per-server [`VaultCache`] (ADR 0017 B.3).
///
/// The async refresher folds per-server [`RefreshOutcome`]s into it; the
/// multi-server "New Window" menu builder reads snapshots so menu rendering
/// never blocks on the network.
#[derive(Default)]
struct VaultCacheState(Mutex<VaultCache>);

impl VaultCacheState {
    /// The cached entry for `server_id`, if any (a clone — the lock is not held
    /// while the menu builds). Read by the B.4 grouped-menu builder.
    #[allow(dead_code)] // consumed by the multi-server menu builder (B.4 / #234)
    fn get(&self, server_id: &str) -> Option<notesmith_tauri::vault_cache::ServerVaults> {
        self.0.lock().expect("vault cache poisoned").get(server_id)
    }

    /// Fold a refresh outcome for `server_id` into the cache at `now`.
    fn merge(&self, server_id: &str, outcome: RefreshOutcome, now: SystemTime) {
        self.0
            .lock()
            .expect("vault cache poisoned")
            .merge_refresh(server_id, outcome, now);
    }

    /// Drop cache entries for servers no longer present.
    fn retain(&self, live_ids: &[String]) {
        self.0
            .lock()
            .expect("vault cache poisoned")
            .retain_servers(live_ids);
    }
}

/// Debounce gate for focus-triggered vault-cache refreshes.
///
/// A `WindowEvent::Focused(true)` fires whenever the app regains focus — the
/// exact moment before a user reaches for the "New Window" menu — so we refresh
/// the remote vault list then. But focus can toggle rapidly (e.g. during a
/// window drag), so this gate ensures we re-fetch at most once per
/// [`VAULT_FOCUS_REFRESH_DEBOUNCE`].
#[derive(Default)]
struct VaultRefreshGate(Mutex<Option<Instant>>);

impl VaultRefreshGate {
    /// Returns `true` (and stamps `now`) when at least `min_interval` has
    /// elapsed since the last allowed refresh; `false` to skip this one.
    fn should_refresh(&self, now: Instant, min_interval: Duration) -> bool {
        let mut last = self.0.lock().expect("vault refresh gate poisoned");
        match *last {
            Some(prev) if now.duration_since(prev) < min_interval => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    }
}

/// Cached vault names for one saved server, exposed to the Settings UI so the
/// companion-memory picker can reuse the per-server vault cache from ADR 0017.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionCachedVaults {
    server_id: String,
    status: &'static str,
    vaults: Vec<String>,
}

/// The authoritative window → connection registry (ADR 0017).
///
/// Wraps the pure [`WindowRegistry`] in a mutex for use as Tauri managed state.
/// The single source of truth for "which server/vault does this window belong
/// to" — both for focus-existing (reuse by `(server_id, vault)`) and for
/// persistence/IPC, which read each window's own connection from here.
#[derive(Default)]
struct WindowConnections(Mutex<WindowRegistry>);

impl WindowConnections {
    fn insert(&self, label: String, context: WindowContext) {
        self.0
            .lock()
            .expect("window registry poisoned")
            .insert(label, context);
    }

    fn remove_label(&self, label: &str) -> Option<WindowContext> {
        self.0
            .lock()
            .expect("window registry poisoned")
            .remove_label(label)
    }

    /// Snapshot of every open vault window as `(label, server_id, vault)`.
    fn vault_windows(&self) -> Vec<(String, String, String)> {
        self.0
            .lock()
            .expect("window registry poisoned")
            .vault_windows()
    }

    fn context_for_label(&self, label: &str) -> Option<WindowContext> {
        self.0
            .lock()
            .expect("window registry poisoned")
            .context_for_label(label)
            .cloned()
    }

    /// Labels of every open window bound to `server_id`, for the open-window
    /// guard on destructive server edits (ADR 0017 Phase D).
    fn windows_for_server(&self, server_id: &str) -> Vec<String> {
        self.0
            .lock()
            .expect("window registry poisoned")
            .windows_for_server(server_id)
    }
}

/// Tracks the path to `windows.json` plus a debounced timestamp for the next
/// flush. The timestamp gates noisy geometry-change writes during a drag.
#[derive(Default)]
struct WindowsPersistState {
    inner: Mutex<WindowsPersistInner>,
}

#[derive(Default)]
struct WindowsPersistInner {
    path: Option<PathBuf>,
    last_write: Option<Instant>,
    /// Whether this launch should ignore an existing `windows.json` (the
    /// file itself stays on disk; we just skip the replay).
    no_restore: bool,
}

impl WindowsPersistState {
    fn set_path(&self, path: PathBuf) {
        self.inner.lock().expect("windows persist poisoned").path = Some(path);
    }

    fn path(&self) -> Option<PathBuf> {
        self.inner
            .lock()
            .expect("windows persist poisoned")
            .path
            .clone()
    }

    fn set_no_restore(&self, value: bool) {
        self.inner
            .lock()
            .expect("windows persist poisoned")
            .no_restore = value;
    }

    fn no_restore(&self) -> bool {
        self.inner
            .lock()
            .expect("windows persist poisoned")
            .no_restore
    }

    /// Returns true if at least `WINDOWS_PERSIST_DEBOUNCE` has elapsed since
    /// the last write (or no write has happened yet). Resets the debounce
    /// clock when it returns `true`.
    fn try_take_debounce_slot(&self) -> bool {
        let mut guard = self.inner.lock().expect("windows persist poisoned");
        let now = Instant::now();
        let ready = match guard.last_write {
            Some(prev) => now.duration_since(prev) >= WINDOWS_PERSIST_DEBOUNCE,
            None => true,
        };
        if ready {
            guard.last_write = Some(now);
        }
        ready
    }
}

fn main() {
    // Resolve the real PATH before anything spawns an agent CLI: macOS GUI
    // launches inherit a minimal launchd PATH without Homebrew/nvm/etc. (ADR 0013).
    agent_path::apply_resolved_path();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(ExitState::default())
        .manage(LastQuitAttempt::default())
        .manage(DaemonProcessState::default())
        .manage(ServersState::default())
        .manage(VaultCacheState::default())
        .manage(VaultRefreshGate::default())
        .manage(WindowConnections::default())
        .manage(WindowsPersistState::default())
        .manage(agent_bridge::AgentBridge::default())
        .manage(InternalHtmlState(Mutex::new(InternalPages {
            fallback: None,
        })))
        .register_uri_scheme_protocol(INTERNAL_PROTOCOL, handle_internal_protocol)
        .register_uri_scheme_protocol(APP_PROTOCOL, handle_app_protocol)
        .enable_macos_default_menu(false)
        .invoke_handler(tauri::generate_handler![
            retry_daemon_connect,
            open_diagnostics,
            quit_app,
            restart_app,
            view_crash_report,
            restart_daemon_anyway,
            open_vault_window,
            open_vault_on_server,
            set_window_title,
            confirm_window_close,
            open_folder_as_vault,
            list_open_vaults,
            close_vault_window,
            pick_vault_folder,
            agent_bridge::agent_list,
            agent_bridge::agent_config_get,
            agent_bridge::agent_config_set,
            agent_bridge::mcp_servers_get,
            agent_bridge::mcp_servers_set,
            agent_bridge::agent_start,
            agent_bridge::agent_prompt,
            agent_bridge::agent_select_model,
            agent_bridge::agent_set_read_only,
            agent_bridge::agent_answer_permission,
            agent_bridge::agent_stop,
            agent_bridge::agent_diagnostics_log,
            agent_bridge::agent_diagnostics_set_verbose,
            agent_bridge::agent_diagnostics_clear,
            agent_diag::agent_diagnostics,
            connection_list,
            connection_cached_vaults,
            connection_add,
            connection_update,
            connection_remove,
            connection_set_active,
            connection_test,
            refresh_remote_vaults,
            window_connection_info
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
            let label = window.label().to_string();
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if label == MAIN_WINDOW_LABEL {
                        // Onboarding window: keep legacy hide behaviour.
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    // Vault windows: let native close proceed. The
                    // `Destroyed` handler below cleans up the window registry
                    // and windows.json. Auto-save ensures no more than ~1 s of
                    // edits can be lost.
                }
                tauri::WindowEvent::Destroyed => {
                    if is_vault_window_label(&label) {
                        handle_vault_window_destroyed(window.app_handle(), &label);
                    }
                }
                tauri::WindowEvent::Focused(true) => {
                    // Returning to the app is the moment before a user opens the
                    // "New Window" menu — refresh the remote vault list so
                    // vaults added out-of-band (API, another client/window)
                    // appear without a restart. Debounced against focus churn.
                    let app = window.app_handle().clone();
                    if app
                        .state::<VaultRefreshGate>()
                        .should_refresh(Instant::now(), VAULT_FOCUS_REFRESH_DEBOUNCE)
                    {
                        tauri::async_runtime::spawn(refresh_vault_cache(app));
                    }
                }
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
                    if is_vault_window_label(&label) =>
                {
                    let app = window.app_handle().clone();
                    let label = label.clone();
                    // Debounce in-process: only one writer wins per slot.
                    if app
                        .state::<WindowsPersistState>()
                        .try_take_debounce_slot()
                    {
                        tauri::async_runtime::spawn(async move {
                            tauri::async_runtime::spawn_blocking(move || {
                                if let Err(error) = persist_open_windows(&app) {
                                    tracing::warn!(
                                        "failed to persist windows after geometry change ({label}): {error}"
                                    );
                                }
                            })
                            .await
                            .ok();
                        });
                    }
                }
                _ => {}
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
        RunEvent::ExitRequested { .. } => {
            // Last chance to flush persistence before the process exits.
            if let Err(error) = persist_open_windows(app_handle) {
                tracing::warn!("failed to flush windows.json on exit: {error}");
            }
        }
        RunEvent::Resumed => emit_wake_event(app_handle),
        _ => {}
    });
}

fn initialize_app(app: &tauri::App) -> Result<(), DynError> {
    let handle = app.handle();

    // Configure persistence path and the --no-restore CLI flag before any
    // window-event handler fires.
    if let Ok(config_dir) = handle.path().app_config_dir() {
        handle
            .state::<WindowsPersistState>()
            .set_path(windows_persist::windows_file_path(&config_dir));
        handle
            .state::<ServersState>()
            .set_path_and_load(servers::servers_file_path(&config_dir));
    } else {
        tracing::warn!("app config dir unavailable; windows.json will not be persisted");
    }
    let no_restore = std::env::args().any(|arg| arg == NO_RESTORE_FLAG);
    handle
        .state::<WindowsPersistState>()
        .set_no_restore(no_restore);
    if no_restore {
        tracing::info!("{NO_RESTORE_FLAG} detected; skipping windows.json replay this launch");
    }

    show_splash_window(handle)?;
    setup_tray(handle)?;
    setup_deep_links(handle)?;
    tauri::async_runtime::block_on(run_startup_flow(handle))?;

    // Kick off an initial remote vault-list refresh so the multi-server "New
    // Window" menu has data to show. Best-effort and off the startup path.
    tauri::async_runtime::spawn(refresh_vault_cache(handle.clone()));

    // Standing-in for a native "menu about to open" hook (which Tauri v2 does
    // not expose): a low-frequency background refresh, aligned to the menu's
    // cache TTL, so the "New Window" list is never more than one interval stale
    // even while the app stays frontmost. Focus events (above) make it instant
    // on refocus; this covers the already-focused, out-of-band case.
    tauri::async_runtime::spawn(periodic_vault_cache_refresh(handle.clone()));
    Ok(())
}

/// Resolve the bundled notesmith sidecar binary path.
///
/// Tauri **strips the target-triple suffix** when bundling `externalBin`, so the
/// packaged sidecar lives next to the main executable as plain `notesmith`
/// (e.g. `Notesmith.app/Contents/MacOS/notesmith`). Some contexts (e.g. the
/// source `binaries/` dir) keep the triple-suffixed name, so we accept both. In
/// dev mode no sidecar exists, so we return `None` and fall back to `PATH`.
fn resolve_sidecar_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    let target_triple = option_env!("TAURI_ENV_TARGET_TRIPLE")
        .or(option_env!("TARGET"))
        .unwrap_or(env!("TARGET_TRIPLE"));

    match find_sidecar_in(exe_dir, target_triple) {
        Some(sidecar) => {
            tracing::info!("resolved sidecar: {}", sidecar.display());
            Some(sidecar)
        }
        None => {
            tracing::info!(
                "no notesmith sidecar next to {}; falling back to PATH",
                exe_dir.display()
            );
            None
        }
    }
}

/// Find the notesmith sidecar in `dir`, preferring the triple-stripped name that
/// Tauri produces when bundling and falling back to the triple-suffixed source
/// name. Returns the first candidate that exists on disk.
fn find_sidecar_in(dir: &std::path::Path, target_triple: &str) -> Option<PathBuf> {
    let extension = if cfg!(windows) { ".exe" } else { "" };
    [
        dir.join(format!("notesmith{extension}")),
        dir.join(format!("notesmith-{target_triple}{extension}")),
    ]
    .into_iter()
    .find(|candidate| candidate.exists())
}

/// The ONNX weight filename that marks a directory as a usable bundled model,
/// matching `notesmith_embed::BUNDLED_ONNX_FILE`.
const BUNDLED_MODEL_ONNX: &str = "model.onnx";

/// Resolve the app-bundled embedding-model directory, if present.
///
/// The model is shipped as a Tauri bundle resource (`resources/embed-model`),
/// which lands next to the executable at runtime: on macOS at
/// `Notesmith.app/Contents/Resources/embed-model` (i.e. `../Resources` from the
/// `MacOS` executable dir), and alongside the binary in flat/dev layouts. When
/// found, its path is passed to the spawned daemon via `NOTESMITH_EMBED_MODEL_DIR`
/// so enabling embeddings is offline (ADR 0018 §9.2, #256 Part B). Returns `None`
/// in unbundled/dev builds, where the daemon downloads the model on first run.
fn resolve_bundled_model_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("../Resources/embed-model"),
        exe_dir.join("embed-model"),
    ];
    find_bundled_model_dir_in(candidates.iter().map(PathBuf::as_path))
}

/// Pure selection over candidate directories, separated for unit testing: the
/// first candidate that contains [`BUNDLED_MODEL_ONNX`] wins.
fn find_bundled_model_dir_in<'a>(
    candidates: impl IntoIterator<Item = &'a std::path::Path>,
) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|dir| dir.join(BUNDLED_MODEL_ONNX).is_file())
        .map(std::path::Path::to_path_buf)
}

/// Resolve the app-bundled whisper-model directory, if present.
///
/// Mirrors [`resolve_bundled_model_dir`] (embeddings) for transcription: the
/// model ships as a Tauri bundle resource (`resources/whisper-model`) that lands
/// at `Notesmith.app/Contents/Resources/whisper-model` (or alongside the binary
/// in dev/flat layouts). When found, the path is passed to the spawned daemon
/// via `NOTESMITH_WHISPER_MODEL_DIR` and inherited by the colocated transcription
/// worker, so first-enable is offline (ADR 0023 §3). Returns `None` in
/// unbundled/dev builds, where the worker downloads the model on first run.
fn resolve_bundled_whisper_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("../Resources/whisper-model"),
        exe_dir.join("whisper-model"),
    ];
    find_bundled_whisper_dir_in(candidates.iter().map(PathBuf::as_path))
}

/// Pure selection over candidate directories, separated for unit testing: the
/// first candidate that holds a whisper.cpp GGML model (`ggml-*.bin`) wins. The
/// filename is not hard-coded so any bundled tier/quantization resolves (matches
/// `notesmith_transcribe::whisper_model_file`).
fn find_bundled_whisper_dir_in<'a>(
    candidates: impl IntoIterator<Item = &'a std::path::Path>,
) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|dir| dir_has_ggml_model(dir))
        .map(std::path::Path::to_path_buf)
}

/// Whether `dir` contains at least one whisper.cpp GGML model file
/// (`ggml-*.bin`). Mirrors the detection in `notesmith_transcribe::model`
/// without taking a crate dependency (as the embed resolver duplicates
/// [`BUNDLED_MODEL_ONNX`]).
fn dir_has_ggml_model(dir: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ggml-") && n.ends_with(".bin"))
    })
}

/// The filename that marks a directory as the built SvelteKit frontend.
const BUNDLED_APP_INDEX: &str = "index.html";

/// Resolve the app-bundled SvelteKit frontend directory, if present.
///
/// The frontend (`ui/app/build`) ships as a Tauri bundle resource mapped to
/// `app/`, which lands next to the executable at runtime: on macOS at
/// `Notesmith.app/Contents/Resources/app` (i.e. `../Resources/app` from the
/// `MacOS` executable dir), and alongside the binary in flat/dev layouts. When
/// found, its path is passed to the spawned daemon via `NOTESMITH_APP_DIR` so
/// the daemon serves `/app/` to the desktop webview. Returns `None` in
/// unbundled/dev builds, where the daemon falls back to the source-tree
/// `<exe>/../../ui/app/build`.
fn resolve_bundled_app_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [exe_dir.join("../Resources/app"), exe_dir.join("app")];
    find_bundled_app_dir_in(candidates.iter().map(PathBuf::as_path))
}

/// Pure selection over candidate directories, separated for unit testing: the
/// first candidate that contains [`BUNDLED_APP_INDEX`] wins.
fn find_bundled_app_dir_in<'a>(
    candidates: impl IntoIterator<Item = &'a std::path::Path>,
) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|dir| dir.join(BUNDLED_APP_INDEX).is_file())
        .map(std::path::Path::to_path_buf)
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

    let daemon_base = active_daemon_url(app);

    match parsed {
        NotesmithUrl::Open { vault, path } => {
            focus_vault_and_navigate(app, &vault, &format!("/vault/{vault}/note/{path}"));
        }
        NotesmithUrl::Daily { vault } => {
            focus_vault_and_navigate(app, &vault, &format!("/vault/{vault}/daily"));
        }
        NotesmithUrl::Search { vault, query } => {
            focus_vault_and_navigate(app, &vault, &format!("/vault/{vault}/search?q={query}"));
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
            focus_vault_and_navigate(app, &vault, &route);
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
            if let Err(error) = show_main_window(app) {
                tracing::error!("failed to show main window for command deep link: {error}");
                return;
            }
            navigate_default_webview(app, &format!("/command/{command_name}"));
        }
        NotesmithUrl::UserAction {
            action_name,
            params,
        } => {
            tracing::info!("user action: {action_name} (params: {params:?})");
            if let Err(error) = show_main_window(app) {
                tracing::error!("failed to show main window for action deep link: {error}");
                return;
            }
            // User actions are best handled via the CLI; log and navigate to a status page
            navigate_default_webview(app, &format!("/action/{action_name}"));
        }
    }
}

/// Open (or focus) the window bound to `vault`, then evaluate the navigation
/// script in it. Used by vault-scoped deep links.
fn focus_vault_and_navigate<R: Runtime>(app: &AppHandle<R>, vault: &str, route: &str) {
    let label = match ensure_vault_window(app, vault) {
        Ok(label) => label,
        Err(error) => {
            tracing::error!("failed to ensure window for vault {vault}: {error}");
            return;
        }
    };

    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let script = format!("window.location.hash = '{}';", route.replace('\'', "\\'"));
        if let Err(error) = window.eval(&script) {
            tracing::error!("failed to navigate webview for vault {vault}: {error}");
        }
    }
}

/// Navigate the most appropriate non-vault-scoped window (e.g. the onboarding
/// window if no vaults are open, otherwise the first vault window).
fn navigate_default_webview<R: Runtime>(app: &AppHandle<R>, route: &str) {
    let script = format!("window.location.hash = '{}';", route.replace('\'', "\\'"));
    let label = first_known_vault_window_label(app).or_else(|| {
        app.get_webview_window(MAIN_WINDOW_LABEL)
            .map(|_| MAIN_WINDOW_LABEL.to_string())
    });
    if let Some(label) = label
        && let Some(window) = app.get_webview_window(&label)
        && let Err(error) = window.eval(&script)
    {
        tracing::error!("failed to navigate webview: {error}");
    }
}

fn emit_wake_event<R: Runtime>(app: &AppHandle<R>) {
    for label in all_app_window_labels(app) {
        if let Some(window) = app.get_webview_window(&label)
            && let Err(error) = window.eval(WAKE_EVENT_SCRIPT)
        {
            tracing::error!("failed to emit wake event for {label}: {error}");
        }
    }
}

/// Build the system menubar.
///
/// The "File" submenu now hosts a dynamic "New Window" submenu listing each
/// registered vault plus an "Open Folder…" entry, so the user can relaunch
/// any closed vault window without leaving the menubar.
fn build_app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let new_window_rows = new_window_menu_rows(app);

    let open = MenuItem::with_id(app, MENU_OPEN, "Open Notesmith", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?;
    let hide = MenuItem::with_id(app, MENU_HIDE, "Close Window", true, Some("CmdOrCtrl+W"))?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, Some("CmdOrCtrl+Q"))?;
    let separator = PredefinedMenuItem::separator(app)?;
    let restart_service = MenuItem::with_id(
        app,
        MENU_RESTART_SERVICE,
        "Restart Service",
        true,
        None::<&str>,
    )?;
    let stop_service = MenuItem::with_id(
        app,
        MENU_STOP_SERVICE,
        "Stop Background Service",
        true,
        None::<&str>,
    )?;
    let view_logs = MenuItem::with_id(app, MENU_VIEW_LOGS, "View Logs", true, None::<&str>)?;
    let copy = PredefinedMenuItem::copy(app, None::<&str>)?;
    let paste = PredefinedMenuItem::paste(app, None::<&str>)?;
    let select_all = PredefinedMenuItem::select_all(app, None::<&str>)?;

    let app_menu_items: Vec<&dyn tauri::menu::IsMenuItem<R>> = if cfg!(target_os = "macos") {
        vec![&open, &settings, &separator, &hide, &quit]
    } else {
        vec![&open, &separator, &hide, &quit]
    };
    let app_submenu = Submenu::with_items(app, "Notesmith", true, &app_menu_items)?;

    let new_window_items = build_new_window_submenu_items(app, &new_window_rows)?;
    let new_window_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        new_window_items.iter().map(|item| item.as_ref()).collect();
    let new_window_submenu = Submenu::with_items(app, "New Window", true, &new_window_refs)?;
    let file_separator = PredefinedMenuItem::separator(app)?;
    let file_settings =
        MenuItem::with_id(app, MENU_SETTINGS, "Settings", true, Some("CmdOrCtrl+,"))?;
    let file_menu_items: Vec<&dyn tauri::menu::IsMenuItem<R>> = if cfg!(target_os = "macos") {
        vec![&new_window_submenu]
    } else {
        vec![&new_window_submenu, &file_separator, &file_settings]
    };
    let file_submenu = Submenu::with_items(app, "File", true, &file_menu_items)?;

    let edit_submenu = Submenu::with_items(app, "Edit", true, &[&copy, &paste, &select_all])?;
    let diagnostics_submenu = Submenu::with_items(
        app,
        "Diagnostics",
        true,
        &[&restart_service, &stop_service, &view_logs],
    )?;

    Menu::with_items(
        app,
        &[
            &app_submenu,
            &file_submenu,
            &edit_submenu,
            &diagnostics_submenu,
        ],
    )
}

fn build_tray_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let new_window_rows = new_window_menu_rows(app);

    let open_items = build_new_window_submenu_items(app, &new_window_rows)?;
    let open_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        open_items.iter().map(|item| item.as_ref()).collect();
    let open_submenu = Submenu::with_items(app, "Open", true, &open_refs)?;

    let restart_service = MenuItem::with_id(
        app,
        MENU_RESTART_SERVICE,
        "Restart Service",
        true,
        None::<&str>,
    )?;
    let stop_service =
        MenuItem::with_id(app, MENU_STOP_SERVICE, "Stop Service", true, None::<&str>)?;
    let view_logs = MenuItem::with_id(app, MENU_VIEW_LOGS, "View Logs", true, None::<&str>)?;
    let quit_app = MenuItem::with_id(app, MENU_QUIT_APP, "Quit Notesmith", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &open_submenu,
            &separator1,
            &restart_service,
            &stop_service,
            &view_logs,
            &separator2,
            &quit_app,
        ],
    )
}

/// Freshness window before a successfully-fetched server's vault list is shown
/// as "stale" (still openable, just visually de-emphasised) in the menu.
const VAULT_CACHE_TTL: Duration = Duration::from_secs(60);

/// Gather the New Window submenu rows from current state: local vault names from
/// `GlobalConfig` plus a cached [`ServerGroup`] per configured remote server.
/// The layout itself is computed by the pure [`build_new_window_rows`].
fn new_window_menu_rows<R: Runtime>(app: &AppHandle<R>) -> Vec<MenuRow> {
    let local_vaults = registered_vault_names();
    let servers = app.state::<ServersState>().snapshot().servers;
    let cache = app.state::<VaultCacheState>();
    let now = SystemTime::now();

    let remotes: Vec<ServerGroup> = servers
        .into_iter()
        .map(|server| {
            let entry = cache.get(&server.id);
            let status = GroupStatus::from_display(
                entry
                    .as_ref()
                    .map(|cached| cached.display_status(now, VAULT_CACHE_TTL)),
            );
            let vaults = entry.map(|cached| cached.vaults).unwrap_or_default();
            ServerGroup {
                server_id: server.id,
                name: server.name,
                vaults,
                status,
            }
        })
        .collect();

    build_new_window_rows(&local_vaults, &remotes)
}

/// Map the pure [`MenuRow`]s into concrete Tauri menu items for the "Open" /
/// "New Window" submenu. Headers and info rows render as disabled items with
/// auto-generated ids (they never dispatch); vault rows carry their
/// server-qualified id so [`handle_menu_event`] can route them.
fn build_new_window_submenu_items<R: Runtime>(
    app: &AppHandle<R>,
    rows: &[MenuRow],
) -> tauri::Result<Vec<Box<dyn tauri::menu::IsMenuItem<R>>>> {
    let mut entries: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = Vec::new();
    for row in rows {
        match row {
            MenuRow::Header(text) | MenuRow::Info(text) => {
                // Disabled, auto-id item used purely as a non-interactive label.
                entries.push(Box::new(MenuItem::new(app, text, false, None::<&str>)?));
            }
            MenuRow::Vault {
                id,
                label,
                enabled,
                source,
            } => match source {
                Some(src) => {
                    // Subtle outline source glyphs (muted-gray monitor / cloud),
                    // matching the chosen "quiet native menu" mockup. Decoded from
                    // the small bundled PNGs in icons/menu/ (24x24 RGBA).
                    let png: &[u8] = match src {
                        VaultSource::Local => {
                            include_bytes!("../icons/menu/source-local.png").as_slice()
                        }
                        VaultSource::Remote => {
                            include_bytes!("../icons/menu/source-remote.png").as_slice()
                        }
                    };
                    let icon = tauri::image::Image::from_bytes(png)?;
                    entries.push(Box::new(IconMenuItem::with_id(
                        app,
                        id,
                        label,
                        *enabled,
                        Some(icon),
                        None::<&str>,
                    )?));
                }
                None => {
                    entries.push(Box::new(MenuItem::with_id(
                        app,
                        id,
                        label,
                        *enabled,
                        None::<&str>,
                    )?));
                }
            },
            MenuRow::Separator => {
                entries.push(Box::new(PredefinedMenuItem::separator(app)?));
            }
            MenuRow::OpenFolder => {
                entries.push(Box::new(MenuItem::with_id(
                    app,
                    OPEN_FOLDER_AS_VAULT_ID,
                    "Open Folder\u{2026}",
                    true,
                    None::<&str>,
                )?));
            }
        }
    }
    Ok(entries)
}

fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    let tray_menu = build_tray_menu(app)?;

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

/// Rebuild the app + tray menus from the current vault list. Called after
/// a successful vault registration (and other config changes).
fn rebuild_dynamic_menus<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    let app_menu = build_app_menu(app)?;
    app.set_menu(app_menu)?;
    let tray_menu = build_tray_menu(app)?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(tray_menu))?;
    }
    Ok(())
}

/// Sorted list of vault names from `GlobalConfig`. Empty when no vaults
/// are registered or when the config fails to load.
fn registered_vault_names() -> Vec<String> {
    match notesmith_config::GlobalConfig::load() {
        Ok(config) => config.vaults.keys().cloned().collect(),
        Err(error) => {
            tracing::warn!("failed to load global config for menu: {error}");
            Vec::new()
        }
    }
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), DynError> {
    match id {
        MENU_OPEN => show_main_window(app),
        MENU_SETTINGS => show_settings_window(app),
        MENU_HIDE => hide_main_window(app),
        MENU_QUIT => handle_quit_request(app),
        MENU_RESTART_SERVICE => {
            restart_service(app.clone());
            Ok(())
        }
        MENU_STOP_SERVICE => {
            stop_service(app.clone());
            Ok(())
        }
        MENU_VIEW_LOGS => open_diagnostics_target(),
        MENU_QUIT_APP => {
            stop_daemon_and_exit(app.clone());
            Ok(())
        }
        OPEN_FOLDER_AS_VAULT_ID => {
            // Ask the active webview (or any webview) to show its
            // folder-picker modal. The modal itself ships in #103.
            for label in all_app_window_labels(app) {
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.emit("notesmith://open-folder-as-vault", ());
                }
            }
            Ok(())
        }
        other if decode_open_vault_target(other).is_some() => {
            let (server_id, vault) = decode_open_vault_target(other).expect("just checked");
            let label = ensure_vault_window_for(app, &server_id, &vault)?;
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    clear_last_quit_attempt(app);

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

    // Prefer focusing an existing vault window. If none exist, open the
    // default vault (or fall back to the onboarding main window if no
    // vault is registered).
    if let Some(label) = first_known_vault_window_label(app)
        && let Some(window) = app.get_webview_window(&label)
    {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    if !should_use_local_vault_state(&effective_settings(app)) {
        ensure_main_window(app)?;
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        return Ok(());
    }

    match resolve_default_vault() {
        Some(vault) => {
            let label = ensure_vault_window(app, &vault)?;
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        None => {
            ensure_main_window(app)?;
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
    }

    Ok(())
}

fn show_settings_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    clear_last_quit_attempt(app);
    ensure_settings_window(app)?;
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(())
}

fn hide_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    for label in all_app_window_labels(app) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.hide();
        }
    }

    Ok(())
}

fn clear_last_quit_attempt<R: Runtime>(app: &AppHandle<R>) {
    *app.state::<LastQuitAttempt>()
        .0
        .lock()
        .expect("last quit attempt poisoned") = None;
}

fn admin_route_url(base_url: &str, route: &str) -> String {
    format!(
        "{}/admin/{}",
        base_url.trim_end_matches('/'),
        route.trim_start_matches('/')
    )
}

fn has_visible_window<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.webview_windows()
        .values()
        .any(|window| window.is_visible().unwrap_or(false))
}

fn hide_all_windows<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    for window in app.webview_windows().values() {
        window.hide()?;
    }

    Ok(())
}

fn notify_user<R: Runtime>(app: &AppHandle<R>, body: &str) {
    if let Err(error) = app
        .notification()
        .builder()
        .title("Notesmith")
        .body(body)
        .show()
    {
        tracing::warn!("failed to show notification: {error}");
    }
}

fn set_expected_daemon_shutdown<R: Runtime>(app: &AppHandle<R>, expected: bool) {
    if let Ok(mut state) = app.state::<DaemonProcessState>().0.try_lock() {
        state.expected_shutdown = expected;
    }
}

fn handle_quit_request<R: Runtime + 'static>(app: &AppHandle<R>) -> Result<(), DynError> {
    let now = Instant::now();
    let last_quit_state = app.state::<LastQuitAttempt>();
    let mut last_quit_attempt = last_quit_state
        .0
        .lock()
        .expect("last quit attempt poisoned");
    let (action, next_attempt) =
        evaluate_quit_request(has_visible_window(app), *last_quit_attempt, now);
    *last_quit_attempt = next_attempt;
    drop(last_quit_attempt);

    match action {
        QuitRequestAction::HideWindows => {
            hide_all_windows(app)?;
            notify_user(
                app,
                "Notesmith is still running in the menu bar. Quit again within 5 seconds to stop the background service and exit.",
            );
            Ok(())
        }
        QuitRequestAction::ArmExit => {
            notify_user(
                app,
                "Quit again within 5 seconds to stop the background service and exit.",
            );
            Ok(())
        }
        QuitRequestAction::StopDaemonAndExit => {
            stop_daemon_and_exit(app.clone());
            Ok(())
        }
    }
}

async fn post_admin_command(url: String) -> Result<(), DynError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .post(url)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn daemon_is_reachable<R: Runtime>(app: &AppHandle<R>) -> bool {
    let url = format!("{}/ping", active_daemon_url(app).trim_end_matches('/'));
    match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client
            .get(url)
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false),
        Err(error) => {
            tracing::warn!("failed to build client for daemon reachability probe: {error}");
            false
        }
    }
}

async fn request_service_stop<R: Runtime>(app: &AppHandle<R>) -> Result<String, DynError> {
    set_expected_daemon_shutdown(app, true);
    let url = admin_route_url(&active_daemon_url(app), "shutdown");

    match post_admin_command(url).await {
        Ok(()) => Ok("Notesmith service is stopping.".to_string()),
        Err(error) => {
            if daemon_is_reachable(app).await {
                set_expected_daemon_shutdown(app, false);
                Err(error)
            } else {
                set_expected_daemon_shutdown(app, false);
                Ok("Notesmith service is already stopped.".to_string())
            }
        }
    }
}

async fn request_service_restart<R: Runtime>(app: &AppHandle<R>) -> Result<String, DynError> {
    let url = admin_route_url(&active_daemon_url(app), "restart");

    match post_admin_command(url).await {
        Ok(()) => Ok("Notesmith service is restarting.".to_string()),
        Err(error) => {
            if daemon_is_reachable(app).await {
                Err(error)
            } else {
                start_and_track_supervised_daemon(app.clone()).await?;
                Ok("Notesmith service started.".to_string())
            }
        }
    }
}

fn stop_service<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        match request_service_stop(&app).await {
            Ok(message) => notify_user(&app, &message),
            Err(error) => {
                tracing::error!("failed to stop daemon service: {error}");
                notify_user(&app, &format!("Failed to stop service: {error}"));
            }
        }
    });
}

fn restart_service<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        match request_service_restart(&app).await {
            Ok(message) => notify_user(&app, &message),
            Err(error) => {
                tracing::error!("failed to restart daemon service: {error}");
                notify_user(&app, &format!("Failed to restart service: {error}"));
            }
        }
    });
}

fn stop_daemon_and_exit<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = request_service_stop(&app).await {
            tracing::warn!("failed to stop daemon during quit: {error}");
        }
        request_exit(&app);
    });
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

    window_config.url = webview_url_for_app(app_url);
    WebviewWindowBuilder::from_config(app, &window_config)?.build()?;
    Ok(())
}

fn ensure_settings_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), DynError> {
    let settings_url = current_settings_app_url(app)?;

    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        if window.url()?.as_str() != settings_url.as_str() {
            window.navigate(settings_url)?;
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

    window_config.label = SETTINGS_WINDOW_LABEL.to_string();
    window_config.url = webview_url_for_app(settings_url);
    window_config.title = "Notesmith Settings".to_string();
    WebviewWindowBuilder::from_config(app, &window_config)?.build()?;
    Ok(())
}

/// Ensure a window exists for the given vault, returning its label.
///
/// If a window for this vault already exists in the registry and the underlying
/// webview is still present, this is a no-op. Otherwise a new
/// window is created by cloning the `main` window config from `tauri.conf.json`,
/// rewriting the label and URL (with `?vault=<vault>` appended) so the
/// frontend can read the binding from `window.location.search`.
///
/// The caller is responsible for `.show()/.set_focus()` on the returned window.
fn ensure_vault_window<R: Runtime>(app: &AppHandle<R>, vault: &str) -> Result<String, DynError> {
    ensure_vault_window_for(app, &active_server_id(app), vault)
}

/// Open (or focus) the window for `vault` bound to a *specific* connection.
///
/// Unlike [`ensure_vault_window`], which uses the active connection, this stamps
/// the given `server_id` into the window context and builds the URL from it —
/// so the restore path can reopen a remote window against its own server even
/// when a different connection is active (ADR 0017 A.5).
fn ensure_vault_window_for<R: Runtime>(
    app: &AppHandle<R>,
    server_id: &str,
    vault: &str,
) -> Result<String, DynError> {
    // Server-qualified identity: the same vault name on two daemons (e.g. a
    // local and a remote `personal`) yields two distinct windows. For the local
    // server the label is byte-identical to the legacy name-only scheme, so
    // existing windows and persisted geometry keep matching (ADR 0017 A.6).
    let key = VaultKey::new(server_id.to_string(), vault.to_string());
    let label = vault_window_label_for_key(&key);
    // Build the window's URL from the *same* connection it is stamped with, so
    // a window bound to a remote server loads that server's frontend (ADR 0017).
    let target_url = app_url_for_server(app, server_id, "/", Some(vault))?;
    let window_context = WindowContext::vault(server_id.to_string(), vault.to_string());

    if let Some(window) = app.get_webview_window(&label) {
        if window.url()?.as_str() != target_url.as_str() {
            window.navigate(target_url)?;
        }
        app.state::<WindowConnections>()
            .insert(label.clone(), window_context);
        return Ok(label);
    }

    let mut window_config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .cloned()
        .ok_or_else(|| std::io::Error::other("missing main window config"))?;

    window_config.label = label.clone();
    window_config.url = webview_url_for_app(target_url);
    window_config.title = "Notesmith".to_string();
    WebviewWindowBuilder::from_config(app, &window_config)?.build()?;

    app.state::<WindowConnections>()
        .insert(label.clone(), window_context);

    // New window — schedule a persistence flush so windows.json reflects it.
    schedule_persist(app);

    Ok(label)
}

/// Open a vault window and apply the saved geometry from `windows.json`.
///
/// Used by the restore-from-disk path. Differs from [`ensure_vault_window_for`]
/// in that it positions/sizes the new window after creation. Existing windows
/// are left untouched (we don't overwrite the user's current geometry).
fn ensure_vault_window_with_geometry<R: Runtime>(
    app: &AppHandle<R>,
    server_id: &str,
    vault: &str,
    entry: &WindowEntry,
) -> Result<String, DynError> {
    let label = ensure_vault_window_for(app, server_id, vault)?;
    if let Some(window) = app.get_webview_window(&label) {
        // Clamp to a visible monitor so windows from a now-disconnected
        // display don't end up off-screen.
        let rect = clamp_to_visible_monitors(&window, entry);
        let _ = window.set_position(tauri::PhysicalPosition {
            x: rect.x,
            y: rect.y,
        });
        let _ = window.set_size(tauri::PhysicalSize {
            width: rect.w,
            height: rect.h,
        });
    }
    Ok(label)
}

fn clamp_to_visible_monitors<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    entry: &WindowEntry,
) -> Rect {
    let monitors = window
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let pos = m.position();
            let size = m.size();
            Rect {
                x: pos.x,
                y: pos.y,
                w: size.width,
                h: size.height,
            }
        })
        .collect::<Vec<_>>();
    windows_persist::clamp_to_monitor(
        Rect {
            x: entry.x,
            y: entry.y,
            w: entry.w,
            h: entry.h,
        },
        &monitors,
    )
}

/// Snapshot the open vault windows and write `windows.json` atomically.
///
/// Skipped silently when the persistence path hasn't been configured (e.g.
/// the app_config_dir lookup failed at startup).
fn persist_open_windows<R: Runtime>(app: &AppHandle<R>) -> io::Result<()> {
    let Some(path) = app.state::<WindowsPersistState>().path() else {
        return Ok(());
    };

    let entries = snapshot_window_entries(app);
    let file = WindowsFile {
        version: windows_persist::SCHEMA_VERSION,
        windows: dedupe_latest_per_vault(entries),
    };
    windows_persist::save(&path, &file)
}

fn snapshot_window_entries<R: Runtime>(app: &AppHandle<R>) -> Vec<WindowEntry> {
    let vault_windows = app.state::<WindowConnections>().vault_windows();

    let mut out = Vec::with_capacity(vault_windows.len());
    for (label, server_id, vault) in vault_windows {
        let Some(window) = app.get_webview_window(&label) else {
            continue;
        };
        let pos = match window.outer_position() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let size = match window.outer_size() {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Persist the connection this window is bound to so it restores against
        // the right daemon (ADR 0017).
        out.push(WindowEntry {
            vault,
            server_id: Some(server_id),
            x: pos.x,
            y: pos.y,
            w: size.width,
            h: size.height,
        });
    }
    out
}

/// Spawn a non-blocking persistence flush.
fn schedule_persist<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = persist_open_windows(&app) {
                tracing::warn!("failed to persist windows.json: {error}");
            }
        })
        .await
        .ok();
    });
}

/// Called after a vault window has actually closed.
///
/// Removes the window from the registry and rewrites `windows.json` so a
/// subsequent launch doesn't reopen the closed window.
fn handle_vault_window_destroyed<R: Runtime>(app: &AppHandle<R>, label: &str) {
    app.state::<WindowConnections>().remove_label(label);
    schedule_persist(app);
}

/// Replay `windows.json` and open one window per saved entry.
///
/// Returns the number of windows opened (0 when the file is missing, empty,
/// corrupt, or when `--no-restore` is in effect).
fn restore_windows_from_disk<R: Runtime>(app: &AppHandle<R>) -> usize {
    let state = app.state::<WindowsPersistState>();
    if state.no_restore() {
        return 0;
    }
    let Some(path) = state.path() else {
        return 0;
    };
    let file = match windows_persist::load(&path) {
        Ok(Some(file)) => file,
        Ok(None) => return 0,
        Err(error) => {
            tracing::warn!("failed to load windows.json: {error}");
            return 0;
        }
    };

    let mut opened = 0;
    let servers = app.state::<ServersState>().snapshot();
    let default_id = active_server_id(app);
    for entry in &file.windows {
        // Resolve which connection this window belongs to. Legacy entries (no
        // server_id) migrate to the default connection; a window whose server
        // was deleted is left unresolved rather than silently opened on local.
        let server_id = match servers.resolve_window_server(entry.server_id.as_deref(), &default_id)
        {
            servers::WindowServerResolution::Resolved(id)
            | servers::WindowServerResolution::Migrated(id) => id,
            servers::WindowServerResolution::Unresolved(missing) => {
                tracing::warn!(
                    vault = %entry.vault,
                    server = %missing,
                    "skipping window restore: its server is no longer configured"
                );
                continue;
            }
        };
        match ensure_vault_window_with_geometry(app, &server_id, &entry.vault, entry) {
            Ok(label) => {
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.show();
                }
                opened += 1;
            }
            Err(error) => {
                tracing::warn!(
                    "failed to restore vault window for '{}': {error}",
                    entry.vault
                );
            }
        }
    }
    opened
}

/// Returns the configured default vault, or the first registered vault when
/// no explicit default is set, or `None` when no vaults are registered.
fn resolve_default_vault() -> Option<String> {
    notesmith_config::GlobalConfig::load()
        .ok()
        .and_then(|config| config.effective_default().map(str::to_string))
}

/// Return any known vault window label (preferring the default vault's) for
/// "focus the active app window" actions like a tray click.
fn first_known_vault_window_label<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let vault_windows = app.state::<WindowConnections>().vault_windows();

    if let Some(default) = resolve_default_vault()
        && let Some((label, ..)) = vault_windows
            .iter()
            .find(|(label, _, vault)| *vault == default && app.get_webview_window(label).is_some())
    {
        return Some(label.clone());
    }

    vault_windows
        .into_iter()
        .find(|(label, ..)| app.get_webview_window(label).is_some())
        .map(|(label, ..)| label)
}

/// Labels of every app-facing window (vault windows + onboarding main +
/// settings). Sourced from the window registry for vault windows.
fn all_app_window_labels<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    let mut labels: Vec<String> = app
        .state::<WindowConnections>()
        .vault_windows()
        .into_iter()
        .map(|(label, _, _)| label)
        .collect();
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        labels.push(MAIN_WINDOW_LABEL.to_string());
    }
    if app.get_webview_window(SETTINGS_WINDOW_LABEL).is_some() {
        labels.push(SETTINGS_WINDOW_LABEL.to_string());
    }
    labels
}

fn show_main_app_window<R: Runtime>(
    app: &AppHandle<R>,
    settings: &DaemonSettings,
) -> Result<(), DynError> {
    close_window(app, FALLBACK_WINDOW_LABEL)?;

    if !should_use_local_vault_state(settings) {
        ensure_main_window(app)?;
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        return Ok(());
    }

    // First: replay persisted windows so a user who had two vaults open
    // gets both back. If anything was restored, we're done.
    if restore_windows_from_disk(app) > 0 {
        return Ok(());
    }

    // Otherwise, fall back to opening the default vault (existing behaviour)
    // or onboarding when no vaults exist.
    match resolve_default_vault() {
        Some(vault) => {
            let label = ensure_vault_window(app, &vault)?;
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        None => {
            ensure_main_window(app)?;
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
    }
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
        model_dir: resolve_bundled_model_dir(),
        whisper_model_dir: resolve_bundled_whisper_dir(),
        app_dir: resolve_bundled_app_dir(),
        ..Default::default()
    }
}

async fn run_startup_flow<R: Runtime>(app: &AppHandle<R>) -> Result<String, DynError> {
    show_splash_window(app)?;
    close_window(app, FALLBACK_WINDOW_LABEL)?;

    let settings = effective_settings(app);
    let outcome = daemon::orchestrate_startup_supervised(&settings).await;
    handle_startup_state(app, &settings, outcome).await
}

async fn handle_startup_state<R: Runtime>(
    app: &AppHandle<R>,
    settings: &DaemonSettings,
    outcome: daemon::SupervisedStartup,
) -> Result<String, DynError> {
    close_window(app, SPLASH_WINDOW_LABEL)?;

    let daemon::SupervisedStartup {
        state,
        child,
        upgraded_daemon,
    } = outcome;

    if let Some(child) = child {
        register_supervised_child(app.clone(), child);
    }

    match state {
        DaemonState::Ready => {
            show_main_app_window(app, settings)?;
            if upgraded_daemon {
                notify_user(app, "Updated background service to latest version.");
            }
            Ok("Notesmith is ready".to_string())
        }
        DaemonState::VersionMismatch { .. } => {
            // The version mismatch is already handled by the auto-restart in
            // orchestrate_startup_supervised. If we still get VersionMismatch
            // here, it means the daemon is user-owned, so show the main window
            // and let the VersionBanner handle the prompt.
            show_main_app_window(app, settings)?;
            Ok("Notesmith daemon needs an update".to_string())
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

/// The active connection's daemon URL, resolved live (local → lockfile port,
/// remote → the stored server URL). Replaces the former app-global
/// `DaemonUrlState`: there is no single mutable "current" URL anymore, each
/// window targets its own connection (ADR 0017). This is the default used by
/// connection-agnostic paths (admin lifecycle, deep-link HTTP posts).
fn active_daemon_url<R: Runtime>(app: &AppHandle<R>) -> String {
    daemon::resolve_daemon_url(&effective_settings(app))
}

fn current_app_url<R: Runtime>(app: &AppHandle<R>) -> Result<Url, DynError> {
    current_app_url_for_vault(app, None)
}

fn current_settings_app_url<R: Runtime>(app: &AppHandle<R>) -> Result<Url, DynError> {
    app_url_for_server(app, &active_server_id(app), "/settings", None)
}

fn current_app_url_for_vault<R: Runtime>(
    app: &AppHandle<R>,
    vault: Option<&str>,
) -> Result<Url, DynError> {
    app_url_for_server(app, &active_server_id(app), "/", vault)
}

/// Build the frontend URL a window bound to `server_id` should load for the
/// given `route`/`vault`. Resolves the connection's daemon URL + load mode from
/// the persisted server list (ADR 0017): each window targets *its* connection,
/// not the single global active one.
fn app_url_for_server<R: Runtime>(
    app: &AppHandle<R>,
    server_id: &str,
    route: &str,
    vault: Option<&str>,
) -> Result<Url, DynError> {
    let settings = settings_for_server(app, server_id);
    let (_mode, url) =
        connection_window_url(&settings.daemon_url, settings.external_url, route, vault);
    Url::parse(&url).map_err(Into::into)
}

/// Daemon settings for the **active connection** — the default used by Global
/// windows (settings/onboarding) and any code path that isn't bound to a
/// specific window's connection.
fn effective_settings<R: Runtime>(app: &AppHandle<R>) -> DaemonSettings {
    settings_for_server(app, &active_server_id(app))
}

/// Daemon settings for a specific connection (`server_id`). The persisted server
/// list is authoritative: a stored remote id yields `external_url = true` + its
/// URL; [`servers::LOCAL_ID`] (or an unknown id) yields the local daemon URL.
/// `daemon_bin`/sidecar and timeouts come from the base defaults.
fn settings_for_server<R: Runtime>(app: &AppHandle<R>, server_id: &str) -> DaemonSettings {
    let base = startup_settings();
    let (url, remote) = app
        .state::<ServersState>()
        .snapshot()
        .target_for(server_id, daemon::DEFAULT_DAEMON_URL);
    DaemonSettings {
        daemon_url: url,
        external_url: remote,
        ..base
    }
}

fn should_use_local_vault_state(settings: &DaemonSettings) -> bool {
    !settings.external_url
}

/// The stable id of the currently-active connection (`servers::LOCAL_ID` for
/// the local daemon). Used to stamp new windows with their owning server in the
/// [`WindowConnections`] registry.
fn active_server_id<R: Runtime>(app: &AppHandle<R>) -> String {
    app.state::<ServersState>()
        .snapshot()
        .connection_list()
        .active_id
}

/// Resolve the daemon connection a window's IPC should target — its per-window
/// server URL plus bearer token, from the registry (ADR 0017 A.4) — rather than
/// the single global active connection.
///
/// A `Global` or not-yet-registered window (settings / onboarding) falls back to
/// the active (default) connection, so connection-scoped actions like "add a
/// vault" still work from the Settings window.
fn window_connection_target<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
) -> servers::ConnectionTarget {
    app.state::<ServersState>()
        .snapshot()
        .resolve_target(&window_server_id(app, label), daemon::DEFAULT_DAEMON_URL)
}

/// The server id a window is bound to, from the registry (ADR 0017 A.4). A
/// `Global` or not-yet-registered window (settings / onboarding) falls back to
/// the active (default) connection.
fn window_server_id<R: Runtime>(app: &AppHandle<R>, label: &str) -> String {
    app.state::<WindowConnections>()
        .context_for_label(label)
        .and_then(|context| context.server_id().map(str::to_string))
        .unwrap_or_else(|| active_server_id(app))
}

/// Resolve the daemon URL and local-state flag for the connection a specific
/// window is bound to (ADR 0017). Agent MCP bindings must target the window's
/// *own* daemon: a remote vault (e.g. `embed-test` on the Clusterfault daemon)
/// is absent from the local sidecar, so binding the vault's MCP server to the
/// global active connection points it at the wrong daemon and it fails to
/// connect — leaving the agent with only unrelated global MCP servers to answer
/// from (issue #259 follow-up).
pub(crate) fn window_daemon_target<R: Runtime>(app: &AppHandle<R>, label: &str) -> (String, bool) {
    let settings = settings_for_server(app, &window_server_id(app, label));
    (
        daemon::resolve_daemon_url(&settings),
        should_use_local_vault_state(&settings),
    )
}

/// Report the **calling window's** connection identity (id, display name,
/// local/remote) for its status-bar badge (ADR 0017 C.1). Resolved from the
/// window's own registry context, not the global active connection — so a local
/// and a remote window each report their own server.
#[tauri::command]
fn window_connection_info(
    app: tauri::AppHandle,
    window: tauri::Window,
) -> servers::ConnectionIdentity {
    let server_id = window_server_id(&app, window.label());
    app.state::<ServersState>()
        .snapshot()
        .connection_identity(&server_id)
}

/// Attach a bearer token to a request when one is configured for the target
/// server. Tokens travel only as an `Authorization` header, never a query
/// param.
fn with_bearer(request: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    match token {
        Some(token) => request.bearer_auth(token),
        None => request,
    }
}

fn webview_url_for_app(url: Url) -> WebviewUrl {
    if url.scheme() == APP_PROTOCOL {
        WebviewUrl::CustomProtocol(url)
    } else {
        WebviewUrl::External(url)
    }
}

fn register_supervised_child<R: Runtime + 'static>(app: AppHandle<R>, child: Child) {
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

async fn start_and_track_supervised_daemon<R: Runtime + 'static>(
    app: AppHandle<R>,
) -> Result<(), DynError> {
    let settings = effective_settings(&app);
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

    register_supervised_child(app, child);
    Ok(())
}

async fn monitor_daemon_process<R: Runtime + 'static>(app: AppHandle<R>) {
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

async fn record_crash_and_handle<R: Runtime + 'static>(
    app: AppHandle<R>,
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

fn handle_app_protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Cow<'static, [u8]>> {
    let path = request.uri().path();
    let Some(asset_path) = app_asset_path(path) else {
        return not_found_response();
    };

    if let Some(asset) = ctx.app_handle().asset_resolver().get(asset_path) {
        return asset_response(asset);
    }

    if should_fallback_to_index(path)
        && let Some(asset) = ctx
            .app_handle()
            .asset_resolver()
            .get("index.html".to_string())
    {
        return asset_response(asset);
    }

    not_found_response()
}

fn asset_response(asset: tauri::Asset) -> tauri::http::Response<Cow<'static, [u8]>> {
    let mut builder = tauri::http::Response::builder().header("Content-Type", asset.mime_type);
    if let Some(csp) = asset.csp_header {
        builder = builder.header("Content-Security-Policy", csp);
    }
    builder
        .body(Cow::Owned(asset.bytes))
        .expect("asset response must build")
}

fn not_found_response() -> tauri::http::Response<Cow<'static, [u8]>> {
    tauri::http::Response::builder()
        .status(404)
        .body(Cow::Borrowed(b"Not found" as &[u8]))
        .expect("404 response must build")
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

fn open_diagnostics_target() -> Result<(), DynError> {
    let path = diagnostics_target()
        .ok_or_else(|| std::io::Error::other("Could not find Notesmith diagnostics on disk"))?;
    open_with_system_app(&path)
}

#[tauri::command]
async fn retry_daemon_connect(app: tauri::AppHandle) -> Result<String, String> {
    run_startup_flow(&app)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn open_diagnostics() -> Result<(), String> {
    open_diagnostics_target().map_err(|error| error.to_string())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    stop_daemon_and_exit(app);
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

    match resolve_default_vault() {
        Some(vault) => {
            let label = ensure_vault_window(&app, &vault).map_err(|error| error.to_string())?;
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        None => {
            ensure_main_window(&app).map_err(|error| error.to_string())?;
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
    }

    Ok("Notesmith service restarted".to_string())
}

/// Open (or focus) the window bound to the given vault.
///
/// In local-daemon mode, returns an error if the vault is not registered in the
/// global config. In remote-daemon mode the configured daemon is authoritative,
/// so the frontend may request windows for vaults that do not exist locally.
/// Creates a new window with `?vault=<vault>` in the URL, or focuses the
/// existing window if one is already open for that vault.
#[tauri::command]
async fn open_vault_window(app: tauri::AppHandle, vault: String) -> Result<(), String> {
    // Validate the vault is registered so we don't create a window pointing
    // at a non-existent vault (the frontend would surface a confusing error).
    if should_use_local_vault_state(&effective_settings(&app)) {
        let config = notesmith_config::GlobalConfig::load().map_err(|error| error.to_string())?;
        if config.vault(&vault).is_none() {
            return Err(format!("Vault '{vault}' is not registered"));
        }
    }

    let label = ensure_vault_window(&app, &vault).map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(())
}

/// Open (or focus) a window for `vault` bound to a *specific* connection
/// (ADR 0017 C.2 "Open this vault on another server…"). Unlike
/// [`open_vault_window`], which uses the active connection, this targets the
/// given `server_id` so the same vault name can be opened on a different daemon
/// in its own window.
#[tauri::command]
async fn open_vault_on_server(
    app: tauri::AppHandle,
    server_id: String,
    vault: String,
) -> Result<(), String> {
    // For the local daemon, validate the vault is registered (parity with
    // open_vault_window). A remote vault is validated by its own daemon.
    if server_id == servers::LOCAL_ID {
        let config = notesmith_config::GlobalConfig::load().map_err(|error| error.to_string())?;
        if config.vault(&vault).is_none() {
            return Err(format!("Vault '{vault}' is not registered"));
        }
    }

    let label =
        ensure_vault_window_for(&app, &server_id, &vault).map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(())
}
///
/// The frontend pushes `<vault> — <note>` (or just `<vault>` when no note
/// is open) on tab changes so the OS window switcher shows distinct titles.
#[tauri::command]
fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|error| error.to_string())
}

fn daemon_error_detail(value: &serde_json::Value) -> Option<String> {
    value
        .get("message")
        .or_else(|| value.get("error"))
        .and_then(|message| message.as_str().map(str::to_string))
}

/// Legacy command: webview response to close-requested events.
///
/// Retained for API compatibility. Vault windows now close natively (the OS
/// close button is no longer intercepted) and cleanup happens in the
/// `WindowEvent::Destroyed` handler.
#[tauri::command]
async fn confirm_window_close(
    app: tauri::AppHandle,
    window: tauri::Window,
    allow: bool,
) -> Result<(), String> {
    let label = window.label().to_string();
    if !is_vault_window_label(&label) {
        return Err(format!("not a vault window: {label}"));
    }
    if !allow {
        return Ok(());
    }

    // The window registry entry + windows.json are cleaned up by the
    // `WindowEvent::Destroyed` handler once destroy() takes effect.
    if let Some(webview) = app.get_webview_window(&label) {
        webview.destroy().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Register a folder as a new vault and immediately open a window for it.
///
/// Steps:
/// 1. Validate the display name client-side (length, characters, uniqueness).
/// 2. POST to `<daemon>/api/app/vaults` with the path + display name.
/// 3. Poll `GET /api/app/vaults` until the new vault appears (≤1 s) so the
///    config-cache is warm before we ask `ensure_vault_window` to open it.
/// 4. Rebuild the dynamic menus so the new vault shows up immediately.
/// 5. Open the window.
#[tauri::command]
async fn open_folder_as_vault(
    app: tauri::AppHandle,
    window: tauri::Window,
    path: String,
    display_name: String,
    create: Option<bool>,
    // Optional kit id to scaffold into the new vault (e.g. `work-notes`),
    // forwarded verbatim — the daemon owns validation.
    kit: Option<String>,
) -> Result<(), String> {
    // Resolve the *calling window's* daemon + bearer token (not the global
    // active connection). A vault window targets its own server; the Settings
    // window falls back to the active connection.
    let target = window_connection_target(&app, window.label());

    let existing: Vec<String> = if !target.remote {
        notesmith_config::GlobalConfig::load()
            .map_err(|error| error.to_string())?
            .vaults
            .keys()
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let validated = validate_vault_display_name(&display_name, existing.iter())?;

    let url = format!("{}/api/app/vaults", target.url.trim_end_matches('/'));
    let mut body =
        serde_json::json!({ "name": validated, "path": path, "create": create.unwrap_or(false) });
    if let Some(kit) = kit.as_deref().filter(|kit| !kit.is_empty()) {
        body["kit"] = serde_json::Value::String(kit.to_string());
    }

    let client = reqwest::Client::new();
    let response = with_bearer(client.post(&url), target.token.as_deref())
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("Failed to contact Notesmith daemon: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|value| daemon_error_detail(&value))
            .unwrap_or_else(|| format!("daemon returned status {status}"));
        return Err(detail);
    }

    // Wait for the new vault to appear in GET /api/app/vaults so subsequent
    // window opens see a fully-loaded vault.
    let list_url = url.clone();
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        let listed = with_bearer(client.get(&list_url), target.token.as_deref())
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|error| format!("Failed to list vaults after register: {error}"))?
            .json::<serde_json::Value>()
            .await
            .map_err(|error| format!("Failed to parse vault list: {error}"))?;

        let found = listed
            .as_array()
            .map(|entries| {
                entries.iter().any(|entry| {
                    entry.get("name").and_then(|n| n.as_str()) == Some(validated.as_str())
                })
            })
            .unwrap_or(false);

        if found {
            break;
        }
        if attempts >= 10 {
            return Err(format!(
                "Vault '{validated}' was registered but didn't appear in the daemon vault list within 1s"
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if let Err(error) = rebuild_dynamic_menus(&app) {
        tracing::warn!("failed to rebuild menus after register: {error}");
    }

    let label = ensure_vault_window(&app, &validated).map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    Ok(())
}

/// Return the names of vaults with at least one open window.
///
/// Used by the Settings UI in #103 to disable the "Remove vault" button
/// when the vault is currently open.
#[tauri::command]
fn list_open_vaults(app: tauri::AppHandle) -> Vec<String> {
    let mut open: Vec<String> = app
        .state::<WindowConnections>()
        .vault_windows()
        .into_iter()
        .filter(|(label, _, _)| app.get_webview_window(label).is_some())
        .map(|(_, _, vault)| vault)
        .collect();
    open.sort();
    open.dedup();
    open
}

/// Close the window(s) for a given vault, if any are open.
///
/// Used by the Settings UI to gracefully close an open vault window before
/// the user removes the vault registration. `WindowEvent::Destroyed` will
/// then clean up the registry entry and persist `windows.json`.
///
/// Returns Ok(()) whether or not a window was found — callers shouldn't have
/// to distinguish "no window open" from "window closed" before removing the
/// vault registration.
#[tauri::command]
async fn close_vault_window(app: tauri::AppHandle, vault: String) -> Result<(), String> {
    let labels: Vec<String> = app
        .state::<WindowConnections>()
        .vault_windows()
        .into_iter()
        .filter(|(_, _, name)| *name == vault)
        .map(|(label, _, _)| label)
        .collect();
    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            window.destroy().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Open a native folder picker and return the absolute path the user selected.
///
/// Returns `Ok(None)` when the user cancels the dialog. The path is validated
/// to point at an existing directory; symlinks are accepted. The frontend uses
/// this for the "Open Folder as Vault" flow in #103.
///
/// On macOS the native NSOpenPanel must be created from the main AppKit
/// thread; calling rfd from a worker thread fails silently or panics. We
/// dispatch to the main thread via `AppHandle::run_on_main_thread` and run
/// the (synchronous) `pick_folder()` there, then return the result through a
/// oneshot channel.
#[tauri::command]
async fn pick_vault_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<std::path::PathBuf>>();
    app.run_on_main_thread(move || {
        let picked = rfd::FileDialog::new()
            .set_title("Choose a folder to open as a vault")
            .pick_folder();
        // Receiver may have been dropped if the command was cancelled; ignore.
        let _ = tx.send(picked);
    })
    .map_err(|error| format!("Failed to dispatch folder picker to main thread: {error}"))?;

    let picked = rx
        .await
        .map_err(|error| format!("Folder picker dropped before returning: {error}"))?;

    let Some(path) = picked else {
        return Ok(None);
    };
    if !path.is_dir() {
        return Err(format!("Selected path is not a folder: {}", path.display()));
    }
    let path_str = path
        .to_str()
        .ok_or_else(|| "Selected path contains invalid UTF-8".to_string())?
        .to_string();
    Ok(Some(path_str))
}

/// Return the saved-server list (token-less) and the active connection id.
#[tauri::command]
fn connection_list(app: tauri::AppHandle) -> ConnectionList {
    app.state::<ServersState>().snapshot().connection_list()
}

fn cached_vaults_for_settings(
    servers: &ServersFile,
    cache: &VaultCacheState,
    now: SystemTime,
    ttl: Duration,
) -> Vec<ConnectionCachedVaults> {
    use notesmith_tauri::vault_cache::VaultListStatus;

    servers
        .servers
        .iter()
        .map(|server| {
            let entry = cache.get(&server.id);
            let (status, vaults) = match entry {
                Some(entry) => {
                    let status = match entry.display_status(now, ttl) {
                        VaultListStatus::Fresh => "fresh",
                        VaultListStatus::Stale => "stale",
                        VaultListStatus::AuthError => "auth_error",
                        VaultListStatus::Unreachable => "unreachable",
                    };
                    (status, entry.vaults)
                }
                None => ("missing", Vec::new()),
            };
            ConnectionCachedVaults {
                server_id: server.id.clone(),
                status,
                vaults,
            }
        })
        .collect()
}

/// Return the cached vault list for each saved remote server. Used by Settings
/// to populate the companion-memory vault picker without inventing a second
/// store; values come directly from ADR 0017's per-server cache.
#[tauri::command]
fn connection_cached_vaults(app: tauri::AppHandle) -> Vec<ConnectionCachedVaults> {
    let servers = app.state::<ServersState>().snapshot();
    let cache = app.state::<VaultCacheState>();
    cached_vaults_for_settings(&servers, &cache, SystemTime::now(), VAULT_CACHE_TTL)
}

/// Add a new server. Validates the name and URL; returns the created entry.
#[tauri::command]
fn connection_add(
    app: tauri::AppHandle,
    name: String,
    url: String,
    token: Option<String>,
) -> Result<ServerView, String> {
    let view = app.state::<ServersState>().mutate(|file| {
        let id = file
            .add(ServerInput { name, url, token })
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(file.get(&id).expect("entry was just added").view())
    })?;
    tauri::async_runtime::spawn(refresh_vault_cache(app.clone()));
    Ok(view)
}

/// User-facing message for the open-window guard on destructive server edits
/// (URL change, removal). Names the server and how many windows must close.
fn open_windows_guard_message(name: &str, count: usize, action: &str) -> String {
    let windows = if count == 1 { "window" } else { "windows" };
    format!("Close the {count} open {windows} connected to \u{201c}{name}\u{201d} before {action}.")
}

/// Update an existing server. Omitted fields are left unchanged; a blank
/// `token` clears the stored credential.
///
/// **URL changes are blocked while windows are open** against this server, so a
/// live window never silently swaps the daemon it talks to (ADR 0017 Phase D).
/// Rename and token edits are always allowed.
#[tauri::command]
fn connection_update(
    app: tauri::AppHandle,
    id: String,
    name: Option<String>,
    url: Option<String>,
    token: Option<String>,
) -> Result<ServerView, String> {
    if let Some(current) = app.state::<ServersState>().snapshot().get(&id) {
        let view = current.view();
        if servers::update_changes_url(&view.url, &url) {
            let open = app.state::<WindowConnections>().windows_for_server(&id);
            if !open.is_empty() {
                return Err(open_windows_guard_message(
                    &view.name,
                    open.len(),
                    "changing its URL",
                ));
            }
        }
    }
    let view = app.state::<ServersState>().mutate(|file| {
        file.update(&id, name, url, token)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(file.get(&id).expect("entry exists after update").view())
    })?;
    tauri::async_runtime::spawn(refresh_vault_cache(app.clone()));
    Ok(view)
}

/// Remove a server. Blocked while windows are open against it, so an open
/// window never ends up bound to a server that no longer exists (ADR 0017
/// Phase D); the user must close those windows first.
#[tauri::command]
fn connection_remove(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let open = app.state::<WindowConnections>().windows_for_server(&id);
    if !open.is_empty() {
        let name = app
            .state::<ServersState>()
            .snapshot()
            .get(&id)
            .map(|entry| entry.view().name)
            .unwrap_or_else(|| id.clone());
        return Err(open_windows_guard_message(&name, open.len(), "removing it"));
    }
    app.state::<ServersState>()
        .mutate(|file| file.remove(&id).map_err(|error| error.to_string()))?;
    tauri::async_runtime::spawn(refresh_vault_cache(app.clone()));
    Ok(())
}

/// Set the **default** connection used by new windows (and connection-agnostic
/// paths). Persists the selection and emits `connection-changed`, but is
/// **non-destructive**: it does not retarget any open window. Each window keeps
/// its own per-window connection (ADR 0017 A.6).
///
/// Pass `id = None` (or `"local"`) for the local daemon, or a stored server id
/// for a remote daemon. When switching the default to local, the local daemon
/// is started if it isn't already running so the default connection is live;
/// switching to remote never spawns one.
#[tauri::command]
async fn connection_set_active(
    app: tauri::AppHandle,
    id: Option<String>,
) -> Result<ConnectionList, String> {
    app.state::<ServersState>().mutate(|file| {
        file.set_active(id.as_deref())
            .map_err(|error| error.to_string())
    })?;

    let settings = effective_settings(&app);

    // Making local the default: make sure the local daemon is up so the next
    // window opened on it connects immediately.
    if !settings.external_url && !daemon::wait_for_daemon_status(&settings).await {
        start_and_track_supervised_daemon(app.clone())
            .await
            .map_err(|error| format!("Failed to start the local daemon: {error}"))?;
    }

    let list = app.state::<ServersState>().snapshot().connection_list();
    if let Err(error) = app.emit(CONNECTION_CHANGED_EVENT, &list) {
        tracing::warn!(%error, "failed to emit connection-changed event");
    }
    Ok(list)
}

/// Event emitted to the frontend when the active connection changes so the
/// status-bar switcher can update without a full reload.
const CONNECTION_CHANGED_EVENT: &str = "notesmith://connection-changed";

/// Probe a candidate daemon URL for reachability without saving it. Never
/// fails the command — unreachable hosts return `reachable: false` with a
/// reason so the UI can show inline feedback.
#[tauri::command]
async fn connection_test(url: String, token: Option<String>) -> ConnectionTestResult {
    probe_daemon(&url, token.as_deref()).await
}

/// Manually refresh every remote server's cached vault list (ADR 0017 B.3).
///
/// Used by the multi-server "New Window" menu surface to pull fresh data on
/// demand, and re-invoked automatically after connection changes. Best-effort:
/// it never errors — failures are recorded in the cache as `AuthError` /
/// `Unreachable` so the menu can still show the last-known vaults.
#[tauri::command]
async fn refresh_remote_vaults(app: tauri::AppHandle) {
    refresh_vault_cache(app).await;
}

/// Concurrency cap for per-server vault refreshes — keeps a large server list
/// from opening an unbounded number of sockets at once.
const VAULT_REFRESH_CONCURRENCY: usize = 6;

/// Per-server timeout for a single vault-list fetch. Short so one slow or dead
/// server never stalls the whole refresh (and, transitively, the menu).
const VAULT_REFRESH_TIMEOUT: Duration = Duration::from_secs(4);

/// Minimum spacing between focus-triggered vault-cache refreshes, so rapid
/// focus toggles (e.g. a window drag) don't hammer remote servers.
const VAULT_FOCUS_REFRESH_DEBOUNCE: Duration = Duration::from_secs(5);

fn build_vault_refresh_client() -> Option<reqwest::Client> {
    match reqwest::Client::builder()
        .timeout(VAULT_REFRESH_TIMEOUT)
        .build()
    {
        Ok(client) => Some(client),
        Err(error) => {
            tracing::warn!(%error, "failed to build vault-refresh HTTP client");
            None
        }
    }
}

pub(crate) async fn refresh_server_vault_cache(
    app: tauri::AppHandle,
    server_id: String,
) -> Option<notesmith_tauri::vault_cache::ServerVaults> {
    let snapshot = app.state::<ServersState>().snapshot();
    let server = snapshot.get(&server_id)?;
    let client = build_vault_refresh_client()?;
    let outcome = fetch_server_vaults(&client, &server.url, server.token.as_deref()).await;
    let cache = app.state::<VaultCacheState>();
    cache.merge(&server_id, outcome, SystemTime::now());
    if let Err(error) = rebuild_dynamic_menus(&app) {
        tracing::warn!(%error, "failed to rebuild menus after companion vault refresh");
    }
    cache.get(&server_id)
}

/// Background loop that refreshes the remote vault-list cache at the menu's
/// cache-TTL cadence, so the "New Window" list stays current even while the app
/// stays frontmost and no focus event fires. Runs for the life of the app;
/// each pass is best-effort (`refresh_vault_cache` never errors).
async fn periodic_vault_cache_refresh(app: tauri::AppHandle) {
    loop {
        tokio::time::sleep(VAULT_CACHE_TTL).await;
        refresh_vault_cache(app.clone()).await;
    }
}

/// Refresh the cached vault list for every **remote** server, concurrently and
/// with a short per-server timeout, then fold the outcomes into the
/// [`VaultCacheState`] (ADR 0017 B.3).
///
/// Local vaults are sourced directly from `GlobalConfig` by the menu builder and
/// are not cached here. This is best-effort and never blocks menu rendering: a
/// failed fetch is recorded as `AuthError`/`Unreachable` (preserving the
/// last-known list), and removed servers are evicted via `retain_servers`.
async fn refresh_vault_cache(app: tauri::AppHandle) {
    use futures::StreamExt;

    let servers = app.state::<ServersState>().snapshot().servers;
    let live_ids: Vec<String> = servers.iter().map(|server| server.id.clone()).collect();

    // Evict cache entries for servers that no longer exist before fetching.
    app.state::<VaultCacheState>().retain(&live_ids);

    if servers.is_empty() {
        return;
    }

    let Some(client) = build_vault_refresh_client() else {
        return;
    };

    let outcomes = futures::stream::iter(servers.into_iter().map(|server| {
        let client = client.clone();
        async move {
            let outcome = fetch_server_vaults(&client, &server.url, server.token.as_deref()).await;
            (server.id, outcome)
        }
    }))
    .buffer_unordered(VAULT_REFRESH_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    let now = SystemTime::now();
    let cache = app.state::<VaultCacheState>();
    for (id, outcome) in outcomes {
        cache.merge(&id, outcome, now);
    }

    // Reflect the refreshed cache in the dynamic menus. The grouped multi-server
    // menu lands in B.4; this is harmless before then.
    if let Err(error) = rebuild_dynamic_menus(&app) {
        tracing::warn!(%error, "failed to rebuild menus after vault refresh");
    }
}

/// Fetch one server's vault list and classify the result, mapping any transport
/// failure to [`RefreshOutcome::Unreachable`].
async fn fetch_server_vaults(
    client: &reqwest::Client,
    base: &str,
    token: Option<&str>,
) -> RefreshOutcome {
    let url = format!("{}/api/app/vaults", base.trim_end_matches('/'));
    match with_bearer(client.get(&url), token).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            match response.bytes().await {
                Ok(body) => classify_vault_response(status, &body),
                Err(_) => RefreshOutcome::Unreachable,
            }
        }
        Err(_) => RefreshOutcome::Unreachable,
    }
}

/// Pure status→outcome policy for a vault-list response, separated from the
/// network call so it can be unit-tested:
/// - `2xx` with a parseable body → [`RefreshOutcome::Loaded`];
/// - `401`/`403` → [`RefreshOutcome::AuthError`];
/// - any other status, or an unparseable `2xx` body → [`RefreshOutcome::Unreachable`].
fn classify_vault_response(status: u16, body: &[u8]) -> RefreshOutcome {
    if (200..300).contains(&status) {
        match servers::parse_vault_names(body) {
            Some(names) => RefreshOutcome::Loaded(names),
            None => RefreshOutcome::Unreachable,
        }
    } else if status == 401 || status == 403 {
        RefreshOutcome::AuthError
    } else {
        RefreshOutcome::Unreachable
    }
}

const CONNECTION_PROBE_TIMEOUT: Duration = Duration::from_secs(4);

async fn probe_daemon(url: &str, token: Option<&str>) -> ConnectionTestResult {
    let base = url.trim().trim_end_matches('/');
    if base.is_empty() {
        return ConnectionTestResult::unreachable("Enter a server URL");
    }
    let client = match reqwest::Client::builder()
        .timeout(CONNECTION_PROBE_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => return ConnectionTestResult::unreachable(error.to_string()),
    };

    let started = Instant::now();
    let mut request = client.get(format!("{base}/ping"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    match request.send().await {
        Ok(response) if response.status().is_success() => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let vault_count = probe_vault_count(&client, base, token).await;
            ConnectionTestResult {
                reachable: true,
                latency_ms: Some(latency_ms),
                vault_count,
                error: None,
            }
        }
        Ok(response) => {
            ConnectionTestResult::unreachable(format!("Server returned {}", response.status()))
        }
        Err(error) => ConnectionTestResult::unreachable(friendly_probe_error(&error)),
    }
}

/// Best-effort vault count from `/api/app/vaults`; `None` on any failure.
async fn probe_vault_count(
    client: &reqwest::Client,
    base: &str,
    token: Option<&str>,
) -> Option<u32> {
    let mut request = client.get(format!("{base}/api/app/vaults"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.bytes().await.ok()?;
    servers::parse_vault_count(&body)
}

fn friendly_probe_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "No response (timed out). Check the daemon is running and reachable.".to_string()
    } else if error.is_connect() {
        "Couldn't connect. Check the URL and that Tailscale is up on both machines.".to_string()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use super::{
        CrashAction, CrashTracker, DaemonSettings, QuitRequestAction, VaultCacheState,
        VaultRefreshGate, admin_route_url, cached_vaults_for_settings, classify_vault_response,
        daemon_error_detail, evaluate_quit_request, find_bundled_model_dir_in,
        find_bundled_whisper_dir_in, find_sidecar_in, open_windows_guard_message,
        should_use_local_vault_state,
    };
    use notesmith_tauri::servers::{ServerEntry, ServersFile};
    use notesmith_tauri::vault_cache::RefreshOutcome;

    #[test]
    fn open_windows_guard_message_names_server_and_counts_windows() {
        assert_eq!(
            open_windows_guard_message("Memory Server", 1, "removing it"),
            "Close the 1 open window connected to \u{201c}Memory Server\u{201d} before removing it."
        );
        assert_eq!(
            open_windows_guard_message("Memory Server", 3, "changing its URL"),
            "Close the 3 open windows connected to \u{201c}Memory Server\u{201d} before changing its URL."
        );
    }

    #[test]
    fn vault_refresh_gate_debounces_within_interval_and_reopens_after() {
        let gate = VaultRefreshGate::default();
        let start = Instant::now();
        let interval = Duration::from_secs(5);

        // First call always refreshes.
        assert!(gate.should_refresh(start, interval));
        // A second call within the interval is suppressed.
        assert!(!gate.should_refresh(start + Duration::from_secs(1), interval));
        assert!(!gate.should_refresh(start + Duration::from_secs(4), interval));
        // Once the interval has elapsed, refreshing is allowed again...
        assert!(gate.should_refresh(start + Duration::from_secs(6), interval));
        // ...and the window restarts from that last allowed refresh.
        assert!(!gate.should_refresh(start + Duration::from_secs(9), interval));
    }

    #[test]
    fn classify_vault_response_maps_status_and_body_to_outcomes() {
        // 2xx with a parseable body → Loaded(names).
        assert_eq!(
            classify_vault_response(200, br#"[{"name":"people"},{"name":"tech"}]"#),
            RefreshOutcome::Loaded(vec!["people".to_string(), "tech".to_string()])
        );
        // 2xx but an unparseable body → Unreachable (don't wipe last-known list).
        assert_eq!(
            classify_vault_response(200, b"not json"),
            RefreshOutcome::Unreachable
        );
        // Auth failures are distinguished so the menu can flag credentials.
        assert_eq!(classify_vault_response(401, b""), RefreshOutcome::AuthError);
        assert_eq!(classify_vault_response(403, b""), RefreshOutcome::AuthError);
        // Any other status → Unreachable.
        assert_eq!(
            classify_vault_response(500, b""),
            RefreshOutcome::Unreachable
        );
        assert_eq!(
            classify_vault_response(404, b""),
            RefreshOutcome::Unreachable
        );
    }

    #[test]
    fn find_sidecar_prefers_the_triple_stripped_bundle_name() {
        let dir = tempfile::tempdir().unwrap();
        let triple = "aarch64-apple-darwin";
        // Tauri ships the sidecar with the triple stripped.
        std::fs::write(dir.path().join("notesmith"), b"bin").unwrap();
        std::fs::write(dir.path().join(format!("notesmith-{triple}")), b"bin").unwrap();

        assert_eq!(
            find_sidecar_in(dir.path(), triple),
            Some(dir.path().join("notesmith"))
        );
    }

    #[test]
    fn find_sidecar_falls_back_to_the_triple_suffixed_name() {
        let dir = tempfile::tempdir().unwrap();
        let triple = "aarch64-apple-darwin";
        std::fs::write(dir.path().join(format!("notesmith-{triple}")), b"bin").unwrap();

        assert_eq!(
            find_sidecar_in(dir.path(), triple),
            Some(dir.path().join(format!("notesmith-{triple}")))
        );
    }

    #[test]
    fn find_sidecar_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_sidecar_in(dir.path(), "aarch64-apple-darwin"), None);
    }

    #[test]
    fn find_bundled_model_dir_picks_first_dir_with_onnx() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("resources-missing");
        let present = root.path().join("resources-present");
        std::fs::create_dir_all(&missing).unwrap();
        std::fs::create_dir_all(&present).unwrap();
        // Only the second candidate holds the ONNX weights.
        std::fs::write(present.join("model.onnx"), b"stub").unwrap();

        assert_eq!(
            find_bundled_model_dir_in([missing.as_path(), present.as_path()]),
            Some(present.clone())
        );
    }

    #[test]
    fn find_bundled_model_dir_is_none_without_onnx() {
        let dir = tempfile::tempdir().unwrap();
        // A directory that exists but lacks model.onnx must not qualify.
        assert_eq!(find_bundled_model_dir_in([dir.path()]), None);
    }

    #[test]
    fn find_bundled_whisper_dir_picks_first_dir_with_ggml_model() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("whisper-missing");
        let present = root.path().join("whisper-present");
        std::fs::create_dir_all(&missing).unwrap();
        std::fs::create_dir_all(&present).unwrap();
        // Only the second candidate holds a ggml model (quantized filename).
        std::fs::write(present.join("ggml-small.en-q5_1.bin"), b"stub").unwrap();

        assert_eq!(
            find_bundled_whisper_dir_in([missing.as_path(), present.as_path()]),
            Some(present.clone())
        );
    }

    #[test]
    fn find_bundled_whisper_dir_ignores_readme_only_dir() {
        let dir = tempfile::tempdir().unwrap();
        // A README-only resource dir (weights not fetched) must not qualify.
        std::fs::write(dir.path().join("README.md"), b"docs").unwrap();
        std::fs::write(dir.path().join("model.bin"), b"not ggml").unwrap();
        assert_eq!(find_bundled_whisper_dir_in([dir.path()]), None);
    }

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
    fn daemon_error_detail_prefers_message() {
        let value = serde_json::json!({
            "message": "Path '/vaults/new' does not exist",
            "error": "path_not_found"
        });

        assert_eq!(
            daemon_error_detail(&value).as_deref(),
            Some("Path '/vaults/new' does not exist")
        );
    }

    #[test]
    fn daemon_error_detail_uses_error_when_message_is_absent() {
        let value = serde_json::json!({
            "error": "Could not write config at /config/notesmith/config.toml: Read-only file system (os error 30)"
        });

        assert_eq!(
            daemon_error_detail(&value).as_deref(),
            Some(
                "Could not write config at /config/notesmith/config.toml: Read-only file system (os error 30)"
            )
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

    #[test]
    fn quit_request_hides_visible_windows_and_arms_second_quit() {
        let now = Instant::now();

        assert_eq!(
            evaluate_quit_request(true, None, now),
            (QuitRequestAction::HideWindows, Some(now))
        );
    }

    #[test]
    fn second_quit_with_hidden_windows_stops_daemon_and_exits() {
        let now = Instant::now();
        let first_attempt = now - Duration::from_secs(2);

        assert_eq!(
            evaluate_quit_request(false, Some(first_attempt), now),
            (QuitRequestAction::StopDaemonAndExit, None)
        );
    }

    #[test]
    fn stale_hidden_quit_attempt_requires_another_confirmation() {
        let now = Instant::now();
        let first_attempt = now - Duration::from_secs(6);

        assert_eq!(
            evaluate_quit_request(false, Some(first_attempt), now),
            (QuitRequestAction::ArmExit, Some(now))
        );
    }

    #[test]
    fn admin_route_url_reuses_current_daemon_origin() {
        assert_eq!(
            admin_route_url("http://127.0.0.1:27183/", "shutdown"),
            "http://127.0.0.1:27183/admin/shutdown"
        );
    }

    #[test]
    fn external_daemon_mode_does_not_use_local_vault_state() {
        assert!(!should_use_local_vault_state(&DaemonSettings {
            external_url: true,
            ..DaemonSettings::default()
        }));
    }

    #[test]
    fn local_daemon_mode_uses_local_vault_state() {
        assert!(should_use_local_vault_state(&DaemonSettings::default()));
    }

    #[test]
    fn settings_cached_vaults_keeps_fresh_entries_fresh_within_ttl() {
        let servers = ServersFile {
            version: 1,
            active_id: None,
            servers: vec![ServerEntry {
                id: "home".into(),
                name: "Home".into(),
                url: "http://example.com".into(),
                token: None,
            }],
        };
        let cache = VaultCacheState::default();
        cache.merge(
            "home",
            RefreshOutcome::Loaded(vec!["personal".into()]),
            SystemTime::UNIX_EPOCH + Duration::from_secs(100),
        );

        let cached = cached_vaults_for_settings(
            &servers,
            &cache,
            SystemTime::UNIX_EPOCH + Duration::from_secs(150),
            Duration::from_secs(60),
        );

        assert_eq!(cached[0].status, "fresh");
        assert_eq!(cached[0].vaults, vec!["personal".to_string()]);
    }

    #[test]
    fn settings_cached_vaults_marks_loaded_entries_stale_after_ttl() {
        let servers = ServersFile {
            version: 1,
            active_id: None,
            servers: vec![ServerEntry {
                id: "home".into(),
                name: "Home".into(),
                url: "http://example.com".into(),
                token: None,
            }],
        };
        let cache = VaultCacheState::default();
        cache.merge(
            "home",
            RefreshOutcome::Loaded(vec!["personal".into()]),
            SystemTime::UNIX_EPOCH + Duration::from_secs(100),
        );

        let cached = cached_vaults_for_settings(
            &servers,
            &cache,
            SystemTime::UNIX_EPOCH + Duration::from_secs(200),
            Duration::from_secs(60),
        );

        assert_eq!(cached[0].status, "stale");
        assert_eq!(cached[0].vaults, vec!["personal".to_string()]);
    }

    #[test]
    fn settings_cached_vaults_preserves_auth_and_unreachable_statuses() {
        let servers = ServersFile {
            version: 1,
            active_id: None,
            servers: vec![
                ServerEntry {
                    id: "auth".into(),
                    name: "Auth".into(),
                    url: "http://auth.example.com".into(),
                    token: None,
                },
                ServerEntry {
                    id: "offline".into(),
                    name: "Offline".into(),
                    url: "http://offline.example.com".into(),
                    token: None,
                },
            ],
        };
        let cache = VaultCacheState::default();
        let last_seen = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        cache.merge(
            "auth",
            RefreshOutcome::Loaded(vec!["personal".into()]),
            last_seen,
        );
        cache.merge("auth", RefreshOutcome::AuthError, last_seen);
        cache.merge(
            "offline",
            RefreshOutcome::Loaded(vec!["work".into()]),
            last_seen,
        );
        cache.merge("offline", RefreshOutcome::Unreachable, last_seen);

        let cached = cached_vaults_for_settings(
            &servers,
            &cache,
            SystemTime::UNIX_EPOCH + Duration::from_secs(10_000),
            Duration::from_secs(60),
        );

        assert_eq!(cached[0].status, "auth_error");
        assert_eq!(cached[0].vaults, vec!["personal".to_string()]);
        assert_eq!(cached[1].status, "unreachable");
        assert_eq!(cached[1].vaults, vec!["work".to_string()]);
    }
}
