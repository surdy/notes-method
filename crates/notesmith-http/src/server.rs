use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize},
    },
};

use anyhow::{Context, anyhow};
use arc_swap::ArcSwap;
use axum::extract::{Path as AxumPath, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::{
    Router,
    routing::{any, delete, get, post},
};
use chrono::{DateTime, Utc};
use notesmith_config::{DaemonLockfile, GlobalConfig, VaultConfig, migration};
use notesmith_core::VaultEngine;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_permission::PermissionGrantStore;
use notesmith_transcript::TranscriptStore;
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
    watcher::WatcherState,
};

pub struct VaultState {
    pub cache: Arc<VaultCache>,
    pub search_index: Arc<SearchIndex>,
    pub engine: NativeVaultEngine,
    pub root: PathBuf,
    pub vault_config: ArcSwap<VaultConfig>,
    pub watcher_state: WatcherState,
    pub rebuilding: AtomicBool,
    pub template_engine: Arc<notesmith_templates::TemplateEngine>,
    pub preview_signing_key: Arc<[u8; blake3::KEY_LEN]>,
    /// Bounded per-vault accumulator of parse warnings (malformed frontmatter,
    /// etc.) surfaced through `GET /api/status` (issue #92, ADR 0009).
    pub parse_warnings: Arc<crate::parse_warnings::ParseWarnings>,
}

pub struct AppState {
    pub vaults: HashMap<String, VaultState>,
    pub event_tx: events::EventSender,
    pub event_buffer: Arc<events::EventBuffer>,
    pub global_config_path: PathBuf,
    pub started_at: DateTime<Utc>,
    pub sse_connection_count: Arc<AtomicUsize>,
    pub shutdown_tx: watch::Sender<bool>,
    pub shutdown_rx: watch::Receiver<bool>,
    pub mcp_services: McpServiceCache,
    /// Daemon-owned, durable per-vault chat transcript store (ADR 0012
    /// Decision 13). Lives outside vaults and outside the rebuildable index
    /// cache so chat history survives restarts and reindexes.
    pub transcripts: Arc<TranscriptStore>,
    /// Daemon-owned, durable per-vault store of persisted "Always Allow" agent
    /// write grants (issue #189). Like transcripts it lives in the data dir so
    /// grants survive daemon/app restarts; consulted by the desktop frontend to
    /// pre-seed a session's permission state.
    pub permissions: Arc<PermissionGrantStore>,
    /// Live per-vault filesystem watchers, keyed by vault name. This is the same
    /// `Arc` shared with the global config watcher, so vaults registered via the
    /// HTTP API can be loaded into the engine map *and* file-watched immediately
    /// (see `routes::vaults::add_vault`) instead of waiting for the config
    /// watcher's debounce window.
    pub vault_watchers: SharedVaultWatchers,
}

impl Default for AppState {
    fn default() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(events::EVENT_CHANNEL_CAPACITY);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            vaults: HashMap::new(),
            event_tx,
            event_buffer: Arc::new(events::EventBuffer::new(events::EVENT_BUFFER_CAPACITY)),
            global_config_path: default_global_config_path(),
            started_at: Utc::now(),
            sse_connection_count: Arc::new(AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
            mcp_services: McpServiceCache::default(),
            transcripts: Arc::new(
                TranscriptStore::open_in_memory()
                    .expect("in-memory transcript store should always open"),
            ),
            permissions: Arc::new(
                PermissionGrantStore::open_in_memory()
                    .expect("in-memory permission store should always open"),
            ),
            vault_watchers: Default::default(),
        }
    }
}

pub type SharedAppState = Arc<RwLock<AppState>>;

/// Cache of per-vault MCP-over-HTTP services, keyed by `(vault_name, read_only)`.
///
/// rmcp's [`notesmith_mcp::NotesmithHttpService`] holds per-session state, so a
/// single instance must be reused across requests for a given vault/mode rather
/// than rebuilt per request. Services are created lazily on first request (so
/// vaults added after the daemon starts are reachable without a restart) and
/// evicted when a vault is removed or its path changes.
pub type McpServiceCache = Arc<Mutex<HashMap<(String, bool), notesmith_mcp::NotesmithHttpService>>>;

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
        .route("/api/v/{vault}/notes-rename/{*path}", post(rename_note))
        .route("/api/v/{vault}/capture", post(capture_note))
        .route("/api/v/{vault}/clip", post(clip_note))
        .route("/api/v/{vault}/search", get(search_notes))
        .route("/api/v/{vault}/related/{*path}", get(related_notes))
        .route(
            "/api/v/{vault}/sidebar-config",
            get(get_sidebar_config).put(put_sidebar_config),
        )
        .route(
            "/api/v/{vault}/config",
            get(get_vault_config).put(put_vault_config),
        )
        .route("/api/v/{vault}/fields", get(get_fields))
        .route(
            "/api/v/{vault}/fields/{key}/suggest",
            get(suggest_field_values),
        )
        .route("/api/v/{vault}/folders", get(get_folders))
        .route("/api/v/{vault}/folder-notes", get(get_folder_notes))
        .route("/api/v/{vault}/folders-rename/{*path}", post(rename_folder))
        .route("/api/v/{vault}/query/sql", post(execute_sql_query))
        .route(
            "/api/v/{vault}/embeddings/stats",
            get(crate::routes::embeddings::get_embedding_stats),
        )
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
        .route(
            "/api/v/{vault}/prompts",
            get(crate::routes::prompts::list_prompts),
        )
        .route(
            "/api/v/{vault}/customizations",
            get(crate::routes::customizations::list_customizations),
        )
        .route("/api/v/{vault}/route/preview", post(route_preview))
        .route("/api/v/{vault}/route/apply", post(route_apply))
        .route("/api/v/{vault}/git/status", get(git_status))
        .route("/api/v/{vault}/git/init", post(git_init))
        .route("/api/v/{vault}/git/log", get(git_log))
        .route("/api/v/{vault}/git/diff/{sha}", get(git_diff))
        .route("/api/v/{vault}/git/commit", post(git_commit))
        .route("/api/v/{vault}/git/sync", post(git_sync))
        .route(
            "/api/v/{vault}/daily/{date}",
            get(get_daily_note).post(create_daily_note),
        )
        .route(
            "/api/v/{vault}/daily/agent-create",
            post(agent_create_daily),
        )
        .route(
            "/api/v/{vault}/periodic/{kind}/current",
            get(get_current_periodic_note),
        )
        .route(
            "/api/v/{vault}/periodic/{kind}/list",
            get(list_periodic_notes),
        )
        .route("/api/v/{vault}/events", get(vault_events))
        .route(
            "/api/v/{vault}/agent/threads",
            get(crate::routes::transcripts::list_threads)
                .post(crate::routes::transcripts::create_thread),
        )
        .route(
            "/api/v/{vault}/agent/threads/{thread_id}",
            get(crate::routes::transcripts::get_thread)
                .delete(crate::routes::transcripts::delete_thread),
        )
        .route(
            "/api/v/{vault}/agent/threads/{thread_id}/rename",
            post(crate::routes::transcripts::rename_thread),
        )
        .route(
            "/api/v/{vault}/agent/threads/{thread_id}/session",
            post(crate::routes::transcripts::set_thread_session),
        )
        .route(
            "/api/v/{vault}/agent/threads/{thread_id}/messages",
            get(crate::routes::transcripts::list_messages)
                .post(crate::routes::transcripts::append_message),
        )
        .route(
            "/api/v/{vault}/agent/permissions",
            get(crate::routes::permissions::list_grants)
                .post(crate::routes::permissions::grant_permission),
        )
        .route(
            "/api/v/{vault}/agent/permissions/{tool}",
            delete(crate::routes::permissions::revoke_permission),
        )
        .route("/mcp/{vault}", any(mcp_service_handler))
        .route("/mcp-ro/{vault}", any(mcp_ro_service_handler))
        .nest_service("/app", app_service)
        .layer(middleware::map_response(set_version_headers))
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

async fn set_version_headers(mut response: axum::response::Response) -> axum::response::Response {
    let headers = response.headers_mut();
    headers.insert(
        "X-Notesmith-Server-Version",
        header::HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );
    headers.insert(
        "X-Notesmith-Schema-Version",
        header::HeaderValue::from_static("1"),
    );
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

pub async fn serve_shared_with_listener(
    listener: TcpListener,
    state: SharedAppState,
    remove_lockfile_on_shutdown: bool,
) -> anyhow::Result<()> {
    let shutdown_rx = {
        let state = state.read().await;
        state.shutdown_rx.clone()
    };

    let router = build_router_with_shared_state(state.clone());

    axum::serve(listener, router)
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

/// Resolve (lazily building and caching) the per-vault MCP-over-HTTP service.
///
/// Returns `None` when the vault is unknown. The service is created on first
/// request — so vaults added after the daemon starts are reachable without a
/// restart — and reused on subsequent requests so MCP session state persists.
async fn resolve_mcp_service(
    state: &SharedAppState,
    vault: &str,
    read_only: bool,
) -> Option<notesmith_mcp::NotesmithHttpService> {
    let key = (vault.to_string(), read_only);

    {
        let cache = state.read().await.mcp_services.clone();
        let guard = cache.lock().expect("mcp service cache poisoned");
        if let Some(service) = guard.get(&key) {
            return Some(service.clone());
        }
    }

    // Build outside the cache lock; constructing the service is synchronous and
    // must not hold the std mutex across the state read.
    let (service, cache) = {
        let app_state = state.read().await;
        let vault_state = app_state.vaults.get(vault)?;
        let ops: Arc<dyn notesmith_ops::Ops> = if read_only {
            Arc::new(notesmith_ops::ReadOnlyOps::new(local_ops_for(
                vault,
                vault_state,
            )))
        } else {
            Arc::new(local_ops_for(vault, vault_state))
        };
        (
            notesmith_mcp::streamable_http_service(ops),
            app_state.mcp_services.clone(),
        )
    };

    let mut guard = cache.lock().expect("mcp service cache poisoned");
    Some(guard.entry(key).or_insert(service).clone())
}

async fn dispatch_mcp(
    state: SharedAppState,
    vault: String,
    read_only: bool,
    request: Request,
) -> Response {
    match resolve_mcp_service(&state, &vault, read_only).await {
        Some(service) => service.handle(request).await.map(axum::body::Body::new),
        None => (StatusCode::NOT_FOUND, format!("unknown vault: {vault}")).into_response(),
    }
}

async fn mcp_service_handler(
    State(state): State<SharedAppState>,
    AxumPath(vault): AxumPath<String>,
    request: Request,
) -> Response {
    dispatch_mcp(state, vault, false, request).await
}

async fn mcp_ro_service_handler(
    State(state): State<SharedAppState>,
    AxumPath(vault): AxumPath<String>,
    request: Request,
) -> Response {
    dispatch_mcp(state, vault, true, request).await
}

pub(crate) fn local_ops_for(name: &str, vault: &VaultState) -> notesmith_ops::LocalOps {
    notesmith_ops::LocalOps::from_shared(
        name.to_string(),
        vault.root.clone(),
        vault.cache.clone(),
        vault.search_index.clone(),
        vault.template_engine.clone(),
        vault.vault_config.load().as_ref().clone(),
        Arc::clone(&vault.preview_signing_key),
    )
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

    // Seed built-in default prompts into the daemon config dir on first run
    // (issue #193). Best-effort: a write failure here must never block startup.
    if let Some(prompts_dir) = notesmith_prompts::default_prompts_dir() {
        match notesmith_prompts::seed_default_prompts(&prompts_dir) {
            Ok(written) if written > 0 => {
                tracing::info!(dir = %prompts_dir.display(), count = written, "seeded default prompts");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(dir = %prompts_dir.display(), reason = %error, "could not seed default prompts");
            }
        }
    }
    let vault_watchers = {
        let state = state.read().await;
        state.vault_watchers.clone()
    };
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
    let _embed_schedulers = crate::embed_scheduler::start_embed_workers(state.clone()).await;
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
    let vault_config = migration::load_and_migrate(vault_path).unwrap_or_else(|error| {
        tracing::warn!("Failed to load/migrate vault config for {vault_name}: {error}");
        default_vault_config(vault_name)
    });
    let cache_path = cache_path_for_vault(vault_name)?;
    let cache = open_or_repair_cache(vault_name, vault_path, &cache_path, &notes, &vault_config)?;
    let search_index_path = search_index_path_for_vault(vault_name)?;
    let search_index = open_or_repair_search_index(vault_name, &search_index_path, &notes)?;
    let cache_path = cache_path_for_vault(vault_name)?;
    let template_engine =
        notesmith_templates::TemplateEngine::new(vault_path.to_path_buf(), Some(cache_path));

    let parse_warnings = crate::parse_warnings::ParseWarnings::new();
    let now = chrono::Utc::now();
    parse_warnings.replace_all(
        notes
            .iter()
            .filter_map(|note| crate::parse_warnings::note_parse_warning(note, now)),
    );

    Ok(VaultState {
        cache: Arc::new(cache),
        search_index: Arc::new(search_index),
        engine,
        root: vault_path.to_path_buf(),
        vault_config: ArcSwap::from_pointee(vault_config),
        watcher_state: WatcherState::new(),
        rebuilding: AtomicBool::new(false),
        template_engine: Arc::new(template_engine),
        preview_signing_key: notesmith_ops::LocalOps::new_preview_signing_key(),
        parse_warnings: Arc::new(parse_warnings),
    })
}

fn default_vault_config(vault_name: &str) -> VaultConfig {
    VaultConfig {
        name: vault_name.to_string(),
        ..Default::default()
    }
}

fn open_or_repair_cache(
    vault_name: &str,
    vault_root: &Path,
    cache_path: &Path,
    notes: &[notesmith_core::Note],
    vault_config: &VaultConfig,
) -> anyhow::Result<VaultCache> {
    match VaultCache::open_for_vault(cache_path, vault_root) {
        Ok(cache) => {
            if cache.check_integrity().unwrap_or(false) {
                cache.reindex_with_periodic(vault_name, notes, &vault_config.periodic)?;
                return Ok(cache);
            }

            tracing::warn!(
                "SQLite cache integrity check failed for vault {vault_name}; rebuilding cache"
            );
        }
        Err(error) => {
            tracing::warn!("failed to open SQLite cache for vault {vault_name}: {error}");
        }
    }

    move_corrupt_sqlite_artifacts(cache_path)?;

    let cache = VaultCache::open_for_vault(cache_path, vault_root)?;
    cache.reindex_with_periodic(vault_name, notes, &vault_config.periodic)?;
    Ok(cache)
}

fn open_or_repair_search_index(
    vault_name: &str,
    search_index_path: &Path,
    notes: &[notesmith_core::Note],
) -> anyhow::Result<SearchIndex> {
    match SearchIndex::open(search_index_path) {
        Ok(search_index) => {
            if search_index.check_integrity().unwrap_or(false) {
                search_index.reindex(vault_name, notes)?;
                return Ok(search_index);
            }

            tracing::warn!(
                "search index integrity check failed for vault {vault_name}; rebuilding index"
            );
        }
        Err(error) => {
            tracing::warn!("failed to open search index for vault {vault_name}: {error}");
        }
    }

    move_corrupt_file(search_index_path)?;

    let search_index = SearchIndex::open(search_index_path)?;
    search_index.reindex(vault_name, notes)?;
    Ok(search_index)
}

fn move_corrupt_sqlite_artifacts(cache_path: &Path) -> anyhow::Result<()> {
    move_corrupt_file(cache_path)?;

    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", cache_path.display(), suffix));
        move_corrupt_file(&sidecar)?;
    }

    Ok(())
}

fn move_corrupt_file(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let file_name = path
        .file_name()
        .context("corrupt artifact path is missing a file name")?
        .to_string_lossy();
    let corrupt_path = path.with_file_name(format!("{file_name}.corrupt.{timestamp}"));
    tracing::info!(
        "moving corrupt artifact from {} to {}",
        path.display(),
        corrupt_path.display()
    );
    std::fs::rename(path, corrupt_path)?;
    Ok(())
}

pub fn build_app_state(config: &GlobalConfig) -> anyhow::Result<AppState> {
    let (event_tx, _) = crate::events::create_event_channel();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut vaults = HashMap::new();

    for (vault_name, registration) in &config.vaults {
        // Per ADR 0009, a single unloadable vault must not abort daemon startup
        // (which would take down every *other* vault too). Log and skip the
        // failing vault; the rest of the daemon comes up normally.
        match create_vault_state(vault_name, &registration.path) {
            Ok(vault_state) => {
                vaults.insert(vault_name.clone(), vault_state);
            }
            Err(error) => {
                tracing::error!(
                    vault = %vault_name,
                    path = %registration.path.display(),
                    reason = %error,
                    "skipping vault that failed to initialize during startup"
                );
            }
        }
    }

    Ok(AppState {
        vaults,
        event_tx,
        event_buffer: Arc::new(events::EventBuffer::new(events::EVENT_BUFFER_CAPACITY)),
        global_config_path: default_global_config_path(),
        started_at: Utc::now(),
        sse_connection_count: Arc::new(AtomicUsize::new(0)),
        shutdown_tx,
        shutdown_rx,
        mcp_services: McpServiceCache::default(),
        transcripts: Arc::new(open_transcript_store()?),
        permissions: Arc::new(open_permission_store()?),
        vault_watchers: Default::default(),
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

/// Durable, daemon-owned data directory (distinct from the rebuildable cache
/// dir). Honours `XDG_DATA_HOME`, falling back to the platform local-data dir.
pub fn data_dir() -> anyhow::Result<PathBuf> {
    let data_root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .context("could not determine data directory")?;
    Ok(data_root.join("notesmith"))
}

/// Path to the single daemon-owned transcript database. Per ADR 0012 Decision
/// 13 this lives in the durable data dir — not inside any vault and not in the
/// `cache.sqlite` index DB, which is dropped on schema bumps/reindex.
pub fn transcripts_path() -> anyhow::Result<PathBuf> {
    Ok(data_dir()?.join("transcripts.sqlite"))
}

fn open_transcript_store() -> anyhow::Result<TranscriptStore> {
    let path = transcripts_path()?;
    TranscriptStore::open(&path)
        .with_context(|| format!("opening transcript store at {}", path.display()))
}

/// Path to the single daemon-owned agent-permission grant database. Per issue
/// #189 this lives alongside the transcript store in the durable data dir so
/// "Always Allow" grants survive daemon/app restarts.
pub fn permissions_path() -> anyhow::Result<PathBuf> {
    Ok(data_dir()?.join("agent-permissions.sqlite"))
}

fn open_permission_store() -> anyhow::Result<PermissionGrantStore> {
    let path = permissions_path()?;
    PermissionGrantStore::open(&path)
        .with_context(|| format!("opening permission store at {}", path.display()))
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
    use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration};
    use notesmith_core::VaultEngine;
    use tower::ServiceExt;

    use crate::events::EventType;

    use super::{
        AppState, build_app_state, build_router_with_app_dir, create_vault_state,
        move_corrupt_file, open_or_repair_cache, wait_for_shutdown_trigger,
    };

    #[test]
    fn transcripts_live_outside_vault_and_cache() {
        let tp = super::transcripts_path().expect("transcripts path");
        // Durable data dir, not the rebuildable cache dir.
        assert!(tp.ends_with("notesmith/transcripts.sqlite"), "{tp:?}");
        // Distinct file from any vault's index cache.
        let cp = super::cache_path_for_vault("anyvault").expect("cache path");
        assert_ne!(tp, cp);
        // Not scoped under a vault directory.
        assert!(!tp.to_string_lossy().contains("anyvault"));
    }

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
    async fn api_responses_include_version_headers() {
        let response =
            build_router_with_app_dir(AppState::default(), PathBuf::from("ui/app/build"))
                .oneshot(
                    Request::builder()
                        .uri("/api/status")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("X-Notesmith-Server-Version")
                .unwrap()
                .to_str()
                .unwrap(),
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(
            response
                .headers()
                .get("X-Notesmith-Schema-Version")
                .unwrap()
                .to_str()
                .unwrap(),
            crate::API_SCHEMA_VERSION.to_string()
        );
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
        let vault_name = format!(
            "work-{}",
            temp_dir.path().file_name().unwrap().to_string_lossy()
        );

        let config = GlobalConfig {
            daemon: Default::default(),
            default_vault: Some(vault_name.clone()),
            vaults: BTreeMap::from([(
                vault_name.clone(),
                VaultRegistration {
                    path: vault_root.clone(),
                },
            )]),
            agents: Default::default(),
            mcp: Default::default(),
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

        assert_eq!(event.vault, vault_name);
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

    #[test]
    fn move_corrupt_file_renames_existing_artifact() {
        let temp_dir = tempfile::tempdir().unwrap();
        let corrupt_path = temp_dir.path().join("cache.sqlite");
        fs::write(&corrupt_path, "not a sqlite database").unwrap();

        move_corrupt_file(&corrupt_path).unwrap();

        assert!(!corrupt_path.exists());
        let renamed = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .find(|name| name.starts_with("cache.sqlite.corrupt."))
            .expect("expected renamed corrupt artifact");
        assert!(!renamed.is_empty());
    }

    #[test]
    fn open_or_repair_cache_rebuilds_from_corrupt_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(
            vault_root.join("Inbox/Repaired.md"),
            "# Repaired\n\ncache recovery test\n",
        )
        .unwrap();

        let notes = notesmith_vault::NativeVaultEngine
            .scan(&vault_root)
            .unwrap();
        let cache_path = temp_dir.path().join("cache.sqlite");
        fs::write(&cache_path, "not a sqlite database").unwrap();

        let cache = open_or_repair_cache(
            "work",
            &vault_root,
            &cache_path,
            &notes,
            &VaultConfig::default(),
        )
        .unwrap();

        let note_count: i64 = cache
            .connection()
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get::<_, i64>(0))
            .unwrap();
        assert_eq!(note_count, 1);
        let renamed = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .find(|name| name.starts_with("cache.sqlite.corrupt."))
            .expect("expected moved corrupt cache");
        assert!(!renamed.is_empty());
    }
}
