#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use notesmith_tauri::daemon::{self, DaemonSettings, DaemonState, DynError};
use tauri::{
    AppHandle, Manager, RunEvent, Runtime, UriSchemeContext, Url, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_deep_link::DeepLinkExt;

const MAIN_WINDOW_LABEL: &str = "main";
const SPLASH_WINDOW_LABEL: &str = "startup-splash";
const FALLBACK_WINDOW_LABEL: &str = "startup-fallback";
const TRAY_ID: &str = "notesmith-tray";
const MENU_OPEN: &str = "open";
const MENU_CAPTURE: &str = "capture";
const MENU_HIDE: &str = "hide";
const MENU_QUIT: &str = "quit";
const WAKE_EVENT_SCRIPT: &str = "window.dispatchEvent(new Event('notesmith://wake'));";

#[derive(Default)]
struct ExitState(AtomicBool);

struct DaemonUrlState(Mutex<String>);

/// Stores dynamic HTML content served by the `notesmith-internal://` protocol.
/// The splash page is static, but fallback pages change per startup attempt.
struct InternalHtmlState(Mutex<InternalPages>);

struct InternalPages {
    fallback: Option<String>,
}

const INTERNAL_PROTOCOL: &str = "notesmith-internal";

enum PrimaryAction {
    Retry,
    RestartApp,
}

struct StartupFallbackView {
    title: String,
    message: String,
    primary_action: PrimaryAction,
}

impl Default for DaemonUrlState {
    fn default() -> Self {
        Self(Mutex::new(DaemonSettings::default().daemon_url))
    }
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .manage(ExitState::default())
        .manage(DaemonUrlState::default())
        .manage(InternalHtmlState(Mutex::new(InternalPages {
            fallback: None,
        })))
        .register_uri_scheme_protocol(INTERNAL_PROTOCOL, handle_internal_protocol)
        .enable_macos_default_menu(false)
        .invoke_handler(tauri::generate_handler![
            retry_daemon_connect,
            open_diagnostics,
            quit_app,
            restart_app
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

fn initialize_app<R: Runtime>(app: &tauri::App<R>) -> Result<(), DynError> {
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
    app.state::<ExitState>().0.store(true, Ordering::SeqCst);
    app.exit(0);
}

fn startup_settings() -> DaemonSettings {
    DaemonSettings {
        sidecar_path: resolve_sidecar_path(),
        ..Default::default()
    }
}

async fn run_startup_flow<R: Runtime>(app: &AppHandle<R>) -> Result<String, DynError> {
    show_splash_window(app)?;
    close_window(app, FALLBACK_WINDOW_LABEL)?;

    let settings = startup_settings();
    let state = daemon::orchestrate_startup(&settings).await;
    handle_startup_state(app, &settings, state)
}

fn handle_startup_state<R: Runtime>(
    app: &AppHandle<R>,
    settings: &DaemonSettings,
    state: DaemonState,
) -> Result<String, DynError> {
    close_window(app, SPLASH_WINDOW_LABEL)?;

    match state {
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
                StartupFallbackView {
                    title: "Restart to finish updating?".to_string(),
                    message: format!(
                        "Notesmith found daemon version {running}, but this app bundles {bundled}. Restart the desktop app to finish the update."
                    ),
                    primary_action: PrimaryAction::RestartApp,
                },
            )?;
            Ok("Notesmith needs a restart".to_string())
        }
        DaemonState::Unreachable => {
            hide_main_window(app)?;
            show_fallback_window(
                app,
                StartupFallbackView {
                    title: "Could not connect to Notesmith daemon".to_string(),
                    message:
                        "Notesmith couldn't start its background service. Retry or open diagnostics for more details."
                            .to_string(),
                    primary_action: PrimaryAction::Retry,
                },
            )?;
            Ok("Notesmith daemon is unreachable".to_string())
        }
        DaemonState::PortConflict { pid } => {
            hide_main_window(app)?;
            show_fallback_window(
                app,
                StartupFallbackView {
                    title: "Another Notesmith daemon is blocking startup".to_string(),
                    message: format!(
                        "Notesmith found a running daemon process (PID {pid}) that never became ready. Quit it or open diagnostics before retrying."
                    ),
                    primary_action: PrimaryAction::Retry,
                },
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

    // Store fallback HTML in managed state so the protocol handler can serve it
    app.state::<InternalHtmlState>()
        .0
        .lock()
        .expect("internal html state poisoned")
        .fallback = Some(fallback_html(&view));

    WebviewWindowBuilder::new(app, FALLBACK_WINDOW_LABEL, internal_url("/fallback"))
        .title("Notesmith")
        .inner_size(480.0, 320.0)
        .resizable(false)
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
    let (primary_label, primary_command) = match view.primary_action {
        PrimaryAction::Retry => ("Retry", "retry_daemon_connect"),
        PrimaryAction::RestartApp => ("Restart App", "restart_app"),
    };

    format!(
        r#"<!doctype html>
<html>
  <body style="margin:0;background:#0f172a;color:#e2e8f0;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh">
    <main style="width:min(420px,calc(100vw - 48px));padding:28px;border-radius:18px;background:#111827;box-shadow:0 16px 40px rgba(15,23,42,.45)">
      <h2 style="margin:0 0 12px;font-size:24px">{}</h2>
      <p id="message" style="margin:0 0 24px;line-height:1.5;color:#cbd5e1">{}</p>
      <div style="display:flex;gap:12px;flex-wrap:wrap">
        <button id="primary" onclick="runPrimary()" style="border:0;border-radius:10px;padding:10px 16px;background:#2563eb;color:white;font:inherit;cursor:pointer">{}</button>
        <button onclick="runCommand('open_diagnostics')" style="border:0;border-radius:10px;padding:10px 16px;background:#334155;color:white;font:inherit;cursor:pointer">Open Diagnostics</button>
        <button onclick="runCommand('quit_app')" style="border:0;border-radius:10px;padding:10px 16px;background:#475569;color:white;font:inherit;cursor:pointer">Quit</button>
      </div>
      <p id="status" style="margin:18px 0 0;color:#93c5fd;min-height:1.5em"></p>
    </main>
    <script>
      async function runCommand(command) {{
        const status = document.getElementById('status');
        status.textContent = 'Working...';
        try {{
          const result = await window.__TAURI_INTERNALS__.invoke(command);
          status.textContent = typeof result === 'string' ? result : '';
        }} catch (error) {{
          status.textContent = String(error);
        }}
      }}
      function runPrimary() {{
        return runCommand('{}');
      }}
    </script>
  </body>
</html>"#,
        escape_html(&view.title),
        escape_html(&view.message),
        primary_label,
        primary_command
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
    app.state::<ExitState>().0.store(true, Ordering::SeqCst);
    app.restart();
}
