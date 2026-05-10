use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{
    Router,
    routing::{get, post},
};
use notesmith_config::{GlobalConfig, VaultConfig};
use notesmith_core::VaultEngine;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{events, routes, watcher::watch_all_vaults};

pub struct VaultState {
    pub cache: VaultCache,
    pub search_index: SearchIndex,
    pub engine: NativeVaultEngine,
    pub root: PathBuf,
    pub vault_config: VaultConfig,
    pub template_engine: notesmith_templates::TemplateEngine,
}

pub struct AppState {
    pub vaults: HashMap<String, VaultState>,
    pub event_tx: events::EventSender,
}

impl Default for AppState {
    fn default() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(events::EVENT_CHANNEL_CAPACITY);
        Self {
            vaults: HashMap::new(),
            event_tx,
        }
    }
}

pub type SharedAppState = Arc<RwLock<AppState>>;

pub fn build_router(state: AppState) -> Router {
    build_router_with_shared_state(Arc::new(RwLock::new(state)))
}

fn build_router_with_shared_state(state: SharedAppState) -> Router {
    Router::new()
        .route("/ping", get(routes::ping))
        .route(
            "/api/v/{vault}/notes",
            get(routes::list_notes).post(routes::create_note),
        )
        .route(
            "/api/v/{vault}/notes/{*path}",
            get(routes::get_note)
                .put(routes::put_note)
                .patch(routes::patch_note)
                .delete(routes::delete_note),
        )
        .route(
            "/api/v/{vault}/notes-append/{*path}",
            post(routes::append_note),
        )
        .route("/api/v/{vault}/notes-move/{*path}", post(routes::move_note))
        .route(
            "/api/v/{vault}/inbox",
            get(routes::list_inbox).post(routes::inbox_capture),
        )
        .route("/api/v/{vault}/search", get(routes::search_notes))
        .route("/api/v/{vault}/query/sql", post(routes::execute_sql_query))
        .route(
            "/api/v/{vault}/tasks",
            get(routes::list_tasks).post(routes::create_task),
        )
        .route(
            "/api/v/{vault}/tasks/toggle",
            post(routes::toggle_task_status),
        )
        .route("/api/v/{vault}/templates", get(routes::list_templates))
        .route(
            "/api/v/{vault}/templates/{name}/render",
            post(routes::render_template),
        )
        .route(
            "/api/v/{vault}/templates/{name}/instantiate",
            post(routes::instantiate_template),
        )
        .route("/api/v/{vault}/route/preview", post(routes::route_preview))
        .route("/api/v/{vault}/route/apply", post(routes::route_apply))
        .route(
            "/api/v/{vault}/daily/{date}",
            get(routes::get_daily_note).post(routes::create_daily_note),
        )
        .route(
            "/api/v/{vault}/daily/agent-create",
            post(routes::agent_create_daily),
        )
        .route("/api/v/{vault}/events", get(routes::vault_events))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve_with_listener(listener: TcpListener, state: AppState) -> anyhow::Result<()> {
    axum::serve(listener, build_router(state))
        .await
        .context("failed to serve notesmith-http")
}

pub async fn serve(bind: &str, state: AppState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind notesmith-http to {bind}"))?;
    serve_with_listener(listener, state).await
}

pub async fn serve_configured_vaults(
    config: &GlobalConfig,
    bind_override: Option<&str>,
) -> anyhow::Result<()> {
    let bind = bind_override.unwrap_or(&config.daemon.bind);
    let state = Arc::new(RwLock::new(build_app_state(config)?));
    let _watchers = watch_all_vaults(state.clone()).await?;
    let _schedulers = crate::scheduler::start_daily_schedulers(state.clone()).await;
    let hook_vaults: Vec<crate::hooks::HookVaultContext> = {
        let state = state.read().await;
        state
            .vaults
            .iter()
            .map(|(name, vault)| crate::hooks::HookVaultContext {
                vault_name: name.clone(),
                vault_root: vault.root.clone(),
                hooks_config: vault.vault_config.hooks.clone(),
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
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind notesmith-http to {bind}"))?;
    axum::serve(listener, build_router_with_shared_state(state))
        .await
        .context("failed to serve notesmith-http")
}

pub fn build_app_state(config: &GlobalConfig) -> anyhow::Result<AppState> {
    let (event_tx, _) = crate::events::create_event_channel();
    let mut vaults = HashMap::new();

    for (vault_name, registration) in &config.vaults {
        let root = registration.path.clone();
        let engine = NativeVaultEngine;
        let notes = engine
            .scan(&root)
            .with_context(|| format!("failed to scan vault {vault_name}"))?;
        let cache = VaultCache::open(&cache_path_for_vault(vault_name)?)?;
        cache.reindex(vault_name, &notes)?;
        let search_index = SearchIndex::open(&search_index_path_for_vault(vault_name)?)?;
        search_index.reindex(vault_name, &notes)?;
        let vault_config = VaultConfig::load_from_vault(&root).unwrap_or_else(|_| VaultConfig {
            name: vault_name.clone(),
            inbox: Default::default(),
            daily: Default::default(),
            editor: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
            homepage: None,
        });
        let cache_path = cache_path_for_vault(vault_name)?;
        let template_engine =
            notesmith_templates::TemplateEngine::new(root.clone(), Some(cache_path));
        vaults.insert(
            vault_name.clone(),
            VaultState {
                cache,
                search_index,
                engine,
                root,
                vault_config,
                template_engine,
            },
        );
    }

    Ok(AppState { vaults, event_tx })
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
