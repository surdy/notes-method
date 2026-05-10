#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};

use notesmith_tauri::daemon::{self, DynError};
use tauri::{
    AppHandle, Manager, RunEvent, Runtime, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

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
    tauri::async_runtime::block_on(daemon::ensure_daemon_running())?;
    setup_tray(app.handle())?;
    show_main_window(app.handle())?;
    Ok(())
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
