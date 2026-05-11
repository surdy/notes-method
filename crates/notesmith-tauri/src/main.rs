#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use notesmith_tauri::daemon::{self, DaemonSettings, DynError};
use tauri::{
    AppHandle, Manager, RunEvent, Runtime, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_deep_link::DeepLinkExt;

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "notesmith-tray";
const MENU_OPEN: &str = "open";
const MENU_CAPTURE: &str = "capture";
const MENU_HIDE: &str = "hide";
const MENU_QUIT: &str = "quit";

#[derive(Default)]
struct ExitState(AtomicBool);

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .manage(ExitState::default())
        .enable_macos_default_menu(false)
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
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
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

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event
            && !app_handle.state::<ExitState>().0.load(Ordering::SeqCst)
        {
            api.prevent_exit();
        }
    });
}

fn initialize_app<R: Runtime>(app: &tauri::App<R>) -> Result<(), DynError> {
    let settings = DaemonSettings {
        sidecar_path: resolve_sidecar_path(),
        ..Default::default()
    };
    tauri::async_runtime::block_on(daemon::ensure_daemon_running_with(settings))?;
    setup_tray(app.handle())?;
    setup_deep_links(app.handle())?;
    show_main_window(app.handle())?;
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

    let config = notesmith_config::GlobalConfig::load().unwrap_or_default();
    let daemon_base = format!("http://{}", config.daemon.bind);

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
        NotesmithUrl::Inbox { vault, text } => {
            let url = format!("{daemon_base}/api/v/{vault}/inbox");
            let body = serde_json::json!({ "text": text });
            tauri::async_runtime::spawn(async move {
                match reqwest::Client::new().post(&url).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!("inbox capture successful");
                    }
                    Ok(resp) => {
                        tracing::error!("inbox capture failed: {}", resp.status());
                    }
                    Err(error) => tracing::error!("inbox request failed: {error}"),
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
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let window_config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .ok_or_else(|| std::io::Error::other("missing main window config"))?;

    WebviewWindowBuilder::from_config(app, window_config)?.build()?;
    Ok(())
}

fn request_exit<R: Runtime>(app: &AppHandle<R>) {
    app.state::<ExitState>().0.store(true, Ordering::SeqCst);
    app.exit(0);
}
