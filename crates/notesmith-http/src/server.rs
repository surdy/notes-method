use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicUsize},
};

use anyhow::{Context, anyhow};
use arc_swap::ArcSwap;
use axum::http::header;
use axum::middleware;
use axum::{
    Router,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use notesmith_config::{DaemonLockfile, GlobalConfig, VaultConfig};
use notesmith_core::VaultEngine;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use tokio::{
    net::TcpListener,
    sync::{RwLock, watch},
};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::routes::*;
use crate::{
    config_watcher::{SharedVaultWatchers, watch_global_config},
    events,
};

pub struct VaultState {
    pub cache: VaultCache,
    pub search_index: SearchIndex,
    pub engine: NativeVaultEngine,
    pub root: PathBuf,
    pub vault_config: ArcSwap<VaultConfig>,
    pub template_engine: notesmith_templates::TemplateEngine,
}

pub struct AppState {
    pub vaults: HashMap<String, VaultState>,
    pub event_tx: events::EventSender,
    pub global_config_path: PathBuf,
    pub started_at: DateTime<Utc>,
    pub sse_connection_count: Arc<AtomicUsize>,
    pub shutdown_tx: watch::Sender<bool>,
    pub shutdown_rx: watch::Receiver<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(events::EVENT_CHANNEL_CAPACITY);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            vaults: HashMap::new(),
            event_tx,
            global_config_path: default_global_config_path(),
            started_at: Utc::now(),
            sse_connection_count: Arc::new(AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
        }
    }
}

pub type SharedAppState = Arc<RwLock<AppState>>;

pub fn build_router(state: AppState) -> Router {
    build_router_with_app_dir(state, app_build_dir())
}

fn build_router_with_shared_state(state: SharedAppState) -> Router {
    build_router_with_shared_state_and_app_dir(state, app_build_dir())
}

fn build_router_with_app_dir(state: AppState, app_dir: PathBuf) -> Router {
    build_router_with_shared_state_and_app_dir(Arc::new(RwLock::new(state)), app_dir)
}

fn build_router_with_shared_state_and_app_dir(state: SharedAppState, app_dir: PathBuf) -> Router {
    let index_path = app_dir.join("index.html");
    let app_service = ServeDir::new(app_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(index_path));

    Router::new()
        .route("/ping", get(ping))
        .route("/api/status", get(crate::routes::status::get_status))
        .route("/admin/logs", get(crate::routes::admin::get_logs))
        .route("/admin/shutdown", post(crate::routes::admin::shutdown))
        .route("/admin/restart", post(crate::routes::admin::restart))
        .route("/api/capabilities", get(get_capabilities))
        .route("/api/app/vaults", get(list_vaults).post(add_vault))
        .route(
            "/api/app/vaults/{name}",
            axum::routing::put(update_vault).delete(remove_vault),
        )
        .route(
            "/api/app/default-vault",
            axum::routing::put(set_default_vault),
        )
        .route("/api/app/vaults/{name}/reindex", post(reindex_vault))
        .route("/api/v/{vault}/notes", get(list_notes).post(create_note))
        .route(
            "/api/v/{vault}/notes/{*path}",
            get(get_note)
                .put(put_note)
                .patch(patch_note)
                .delete(delete_note),
        )
        .route("/api/v/{vault}/html/{*path}", get(render_note_html))
        .route("/api/v/{vault}/notes-append/{*path}", post(append_note))
        .route("/api/v/{vault}/notes-move/{*path}", post(move_note))
        .route("/api/v/{vault}/capture", post(capture_note))
        .route("/api/v/{vault}/search", get(search_notes))
        .route(
            "/api/v/{vault}/sidebar-config",
            get(get_sidebar_config).put(put_sidebar_config),
        )
        .route(
            "/api/v/{vault}/config",
            get(get_vault_config).put(put_vault_config),
        )
        .route("/api/v/{vault}/folders", get(get_folders))
        .route("/api/v/{vault}/folder-notes", get(get_folder_notes))
        .route("/api/v/{vault}/query/sql", post(execute_sql_query))
        .route("/api/v/{vault}/tasks", get(list_tasks).post(create_task))
        .route("/api/v/{vault}/tasks/toggle", post(toggle_task_status))
        .route("/api/v/{vault}/templates", get(list_templates))
        .route(
            "/api/v/{vault}/templates/{name}/render",
            post(render_template),
        )
        .route(
            "/api/v/{vault}/templates/{name}/instantiate",
            post(instantiate_template),
        )
        .route("/api/v/{vault}/route/preview", post(route_preview))
        .route("/api/v/{vault}/route/apply", post(route_apply))
        .route("/api/v/{vault}/git/status", get(git_status))
        .route("/api/v/{vault}/git/sync", post(git_sync))
        .route(
            "/api/v/{vault}/daily/{date}",
            get(get_daily_note).post(create_daily_note),
        )
        .route(
            "/api/v/{vault}/daily/agent-create",
            post(agent_create_daily),
        )
        .route("/api/v/{vault}/events", get(vault_events))
        .nest_service("/app", app_service)
        .layer(middleware::map_response(set_cache_headers))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn set_cache_headers(
    uri: axum::http::Uri,
    mut response: axum::response::Response,
) -> axum::response::Response {
    let path = uri.path();
    if path.contains("/_app/immutable/") {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    } else if path.starts_with("/app") {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        );
    }
    response
}

fn app_build_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NOTESMITH_APP_DIR") {
        return PathBuf::from(dir);
    }

    // Resolve relative to the binary location so the daemon works regardless of CWD.
    // Binary is at <workspace>/target/{debug,release}/notesmith — walk up to workspace root.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe
            .canonicalize()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        {
            // Try <exe_dir>/../../ui/app/build (for target/release/notesmith)
            let candidate = exe_dir.join("../../ui/app/build");
            if let Ok(resolved) = candidate.canonicalize() {
                return resolved;
            }
        }
    }

    // Fallback to relative path (works when CWD is workspace root)
    PathBuf::from("ui/app/build")
}

async fn wait_for_watch_shutdown(mut shutdown_rx: watch::Receiver<bool>) {
    while !*shutdown_rx.borrow() {
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

async fn wait_for_shutdown_trigger<CtrlC, Sigterm>(
    shutdown_rx: watch::Receiver<bool>,
    ctrl_c: CtrlC,
    sigterm: Sigterm,
) where
    CtrlC: Future<Output = ()>,
    Sigterm: Future<Output = ()>,
{
    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm => {},
        _ = wait_for_watch_shutdown(shutdown_rx) => {},
    }
}

async fn wait_for_shutdown_signal(shutdown_rx: watch::Receiver<bool>) -> anyhow::Result<()> {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|error| anyhow!("failed to listen for SIGTERM: {error}"))?;
        wait_for_shutdown_trigger(shutdown_rx, ctrl_c, async move {
            let _ = sigterm.recv().await;
        })
        .await;
    }

    #[cfg(not(unix))]
    {
        wait_for_shutdown_trigger(shutdown_rx, ctrl_c, std::future::pending::<()>()).await;
    }

    Ok(())
}

async fn serve_shared_with_listener(
    listener: TcpListener,
    state: SharedAppState,
    remove_lockfile_on_shutdown: bool,
) -> anyhow::Result<()> {
    let shutdown_rx = {
        let state = state.read().await;
        state.shutdown_rx.clone()
    };

    axum::serve(listener, build_router_with_shared_state(state))
        .with_graceful_shutdown(async move {
            if let Err(error) = wait_for_shutdown_signal(shutdown_rx).await {
                tracing::warn!("graceful shutdown signal listener failed: {error}");
            }

            if remove_lockfile_on_shutdown {
                if let Err(error) = DaemonLockfile::remove() {
                    tracing::warn!("failed to remove daemon lockfile during shutdown: {error}");
                }
            }
        })
        .await
        .context("failed to serve notesmith-http")
}

pub async fn serve_with_listener(listener: TcpListener, state: AppState) -> anyhow::Result<()> {
    serve_shared_with_listener(listener, Arc::new(RwLock::new(state)), false).await
}

pub async fn serve(bind: &str, state: AppState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind notesmith-http to {bind}"))?;
    serve_with_listener(listener, state).await
}

struct DaemonLockfileGuard;

impl Drop for DaemonLockfileGuard {
    fn drop(&mut self) {
        if let Err(error) = DaemonLockfile::remove() {
            tracing::warn!("failed to remove daemon lockfile: {error}");
        }
    }
}

fn ensure_no_active_daemon() -> anyhow::Result<()> {
    if let Some(lockfile) = DaemonLockfile::read_active()? {
        anyhow::bail!(
            "Another Notesmith daemon is running (PID {} on port {})",
            lockfile.pid,
            lockfile.port
        );
    }

    Ok(())
}

fn write_daemon_lockfile(listener: &TcpListener, started_at: DateTime<Utc>) -> anyhow::Result<()> {
    let lockfile = DaemonLockfile {
        pid: std::process::id(),
        port: listener
            .local_addr()
            .context("failed to read bound daemon address")?
            .port(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at,
        binary_path: std::env::current_exe().context("failed to resolve daemon binary path")?,
    };

    match lockfile.write() {
        Ok(()) => Ok(()),
        Err(notesmith_config::ConfigError::WriteError { source, .. })
            if source.kind() == std::io::ErrorKind::AlreadyExists =>
        {
            if let Some(existing) = DaemonLockfile::read_active()? {
                anyhow::bail!(
                    "Another Notesmith daemon is running (PID {} on port {})",
                    existing.pid,
                    existing.port
                );
            }

            lockfile
                .write()
                .context("failed to write daemon lockfile after clearing stale entry")
        }
        Err(error) => Err(error).context("failed to write daemon lockfile"),
    }
}

pub async fn serve_configured_vaults(
    config: &GlobalConfig,
    bind_override: Option<&str>,
) -> anyhow::Result<()> {
    ensure_no_active_daemon()?;
    let _log_guard = crate::logging::init_logging();

    let bind = bind_override.unwrap_or(&config.daemon.bind);
    let state = Arc::new(RwLock::new(build_app_state(config)?));
    let vault_watchers: SharedVaultWatchers = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let vault_names = {
        let state = state.read().await;
        state.vaults.keys().cloned().collect::<Vec<_>>()
    };
    for vault_name in vault_names {
        let watcher = crate::watcher::watch_vault(state.clone(), vault_name.clone()).await?;
        vault_watchers.lock().await.insert(vault_name, watcher);
    }
    let _config_watcher = watch_global_config(state.clone(), vault_watchers).await?;
    let _schedulers = crate::scheduler::start_daily_schedulers(state.clone()).await;
    let hook_vaults: Vec<crate::hooks::HookVaultContext> = {
        let state = state.read().await;
        state
            .vaults
            .iter()
            .map(|(name, vault)| crate::hooks::HookVaultContext {
                vault_name: name.clone(),
                vault_root: vault.root.clone(),
                hooks_config: vault.vault_config.load().hooks.clone(),
            })
            .collect()
    };
    let hook_rx = {
        let state = state.read().await;
        state.event_tx.subscribe()
    };
    let _hook_listener = crate::hooks::start_hook_listener(
        hook_rx,
        hook_vaults,
        notesmith_hooks::HookRunner::default(),
    );

    // Start git timers for vaults with git enabled
    let git_configs: Vec<notesmith_git::timers::GitTimerConfig> = {
        let state = state.read().await;
        state
            .vaults
            .iter()
            .filter(|(_, v)| v.vault_config.load().git.enabled)
            .map(|(name, v)| notesmith_git::timers::GitTimerConfig {
                vault_name: name.clone(),
                vault_root: v.root.clone(),
                config: v.vault_config.load().git.clone(),
            })
            .collect()
    };
    let _git_timers = notesmith_git::timers::start_git_timers(git_configs).await;

    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind notesmith-http to {bind}"))?;

    let started_at = {
        let state = state.read().await;
        state.started_at
    };
    write_daemon_lockfile(&listener, started_at)?;
    let _lockfile_guard = DaemonLockfileGuard;

    serve_shared_with_listener(listener, state, true).await
}

pub fn create_vault_state(vault_name: &str, vault_path: &Path) -> anyhow::Result<VaultState> {
    let engine = NativeVaultEngine;
    let notes = engine
        .scan(vault_path)
        .with_context(|| format!("failed to scan vault {vault_name}"))?;
    let cache = VaultCache::open(&cache_path_for_vault(vault_name)?)?;
    cache.reindex(vault_name, &notes)?;
    let search_index = SearchIndex::open(&search_index_path_for_vault(vault_name)?)?;
    search_index.reindex(vault_name, &notes)?;
    let vault_config = VaultConfig::load_from_vault(vault_path).unwrap_or_else(|_| VaultConfig {
        name: vault_name.to_string(),
        capture: Default::default(),
        daily: Default::default(),
        editor: Default::default(),
        git: Default::default(),
        hooks: Default::default(),
        homepage: None,
    });
    let cache_path = cache_path_for_vault(vault_name)?;
    let template_engine =
        notesmith_templates::TemplateEngine::new(vault_path.to_path_buf(), Some(cache_path));

    Ok(VaultState {
        cache,
        search_index,
        engine,
        root: vault_path.to_path_buf(),
        vault_config: ArcSwap::from_pointee(vault_config),
        template_engine,
    })
}

pub fn build_app_state(config: &GlobalConfig) -> anyhow::Result<AppState> {
    let (event_tx, _) = crate::events::create_event_channel();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut vaults = HashMap::new();

    for (vault_name, registration) in &config.vaults {
        vaults.insert(
            vault_name.clone(),
            create_vault_state(vault_name, &registration.path)?,
        );
    }

    Ok(AppState {
        vaults,
        event_tx,
        global_config_path: default_global_config_path(),
        started_at: Utc::now(),
        sse_connection_count: Arc::new(AtomicUsize::new(0)),
        shutdown_tx,
        shutdown_rx,
    })
}

pub fn cache_dir_for_vault(vault_name: &str) -> anyhow::Result<PathBuf> {
    let cache_root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(dirs::cache_dir)
        .context("could not determine cache directory")?;
    Ok(cache_root
        .join("notesmith")
        .join(sanitize_vault_name(vault_name)))
}

pub fn cache_path_for_vault(vault_name: &str) -> anyhow::Result<PathBuf> {
    Ok(cache_dir_for_vault(vault_name)?.join("cache.sqlite"))
}

pub fn search_index_path_for_vault(vault_name: &str) -> anyhow::Result<PathBuf> {
    Ok(cache_dir_for_vault(vault_name)?.join("tantivy"))
}

fn sanitize_vault_name(vault_name: &str) -> String {
    vault_name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

fn default_global_config_path() -> PathBuf {
    GlobalConfig::default_path().unwrap_or_else(|| PathBuf::from(".config/notesmith/config.toml"))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::PathBuf, time::Duration};

    use axum::{body::Body, http::Request};
    use notesmith_config::{GlobalConfig, VaultRegistration};
    use tower::ServiceExt;

    use crate::events::EventType;

    use super::{
        AppState, build_app_state, build_router_with_app_dir, create_vault_state,
        wait_for_shutdown_trigger,
    };

    #[tokio::test]
    async fn serves_app_index_for_nested_app_routes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app_dir = temp_dir.path().join("app");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            app_dir.join("index.html"),
            "<html><body>app shell</body></html>",
        )
        .unwrap();

        let response = build_router_with_app_dir(AppState::default(), app_dir)
            .oneshot(
                Request::builder()
                    .uri("/app/customers/acme")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("app shell"), "body was: {text}");
    }

    #[tokio::test]
    async fn admin_shutdown_returns_ok() {
        let response =
            build_router_with_app_dir(AppState::default(), PathBuf::from("ui/app/build"))
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/admin/shutdown")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn admin_shutdown_triggers_signal() {
        let state = AppState::default();
        let mut shutdown_rx = state.shutdown_rx.clone();

        let response = build_router_with_app_dir(state, PathBuf::from("ui/app/build"))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/shutdown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        tokio::time::timeout(Duration::from_secs(1), async {
            while !*shutdown_rx.borrow() {
                shutdown_rx.changed().await.unwrap();
            }
        })
        .await
        .unwrap();

        assert!(*shutdown_rx.borrow());
    }

    #[tokio::test]
    async fn admin_shutdown_emits_shutting_down_event() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(&vault_root).unwrap();

        let config = GlobalConfig {
            daemon: Default::default(),
            default_vault: Some("work".to_string()),
            vaults: BTreeMap::from([(
                "work".to_string(),
                VaultRegistration {
                    path: vault_root.clone(),
                },
            )]),
        };
        let state = build_app_state(&config).unwrap();
        let mut event_rx = state.event_tx.subscribe();

        let response = build_router_with_app_dir(state, PathBuf::from("ui/app/build"))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/shutdown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(event.vault, "work");
        assert_eq!(event.event_type, EventType::ShuttingDown);
    }

    #[tokio::test]
    async fn shutdown_waiter_completes_on_sigterm() {
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let (sigterm_tx, sigterm_rx) = tokio::sync::oneshot::channel::<()>();

        let waiter = tokio::spawn(wait_for_shutdown_trigger(
            shutdown_rx,
            std::future::pending::<()>(),
            async move {
                let _ = sigterm_rx.await;
            },
        ));

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiter.is_finished());

        sigterm_tx.send(()).unwrap();

        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn create_vault_state_indexes_existing_notes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(
            vault_root.join("Inbox/Test Note.md"),
            "# Test Note\n\ncreate_vault_state smoke test\n",
        )
        .unwrap();

        let vault_name = format!(
            "work-{}",
            temp_dir.path().file_name().unwrap().to_string_lossy()
        );
        let vault = create_vault_state(&vault_name, &vault_root).unwrap();

        assert_eq!(vault.root, vault_root);
        assert_eq!(vault.vault_config.load().name, vault_name);

        let note_count: i64 = vault
            .cache
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM notes WHERE path = 'Inbox/Test Note.md'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(note_count, 1);

        let search_results = vault.search_index.search("smoke", 10).unwrap();
        assert!(
            search_results
                .iter()
                .any(|result| result.path == "Inbox/Test Note.md")
        );
    }
}
