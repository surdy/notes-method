use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{
    Router,
    routing::{get, post},
};
use notesmith_config::GlobalConfig;
use notesmith_core::VaultEngine;
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{routes, watcher::watch_all_vaults};

pub struct VaultState {
    pub cache: VaultCache,
    pub search_index: SearchIndex,
    pub engine: NativeVaultEngine,
    pub root: PathBuf,
}

#[derive(Default)]
pub struct AppState {
    pub vaults: HashMap<String, VaultState>,
}

pub type SharedAppState = Arc<RwLock<AppState>>;

pub fn build_router(state: AppState) -> Router {
    build_router_with_shared_state(Arc::new(RwLock::new(state)))
}

fn build_router_with_shared_state(state: SharedAppState) -> Router {
    Router::new()
        .route("/ping", get(routes::ping))
        .route("/api/v/{vault}/notes", get(routes::list_notes))
        .route("/api/v/{vault}/notes/{*path}", get(routes::get_note))
        .route("/api/v/{vault}/search", get(routes::search_notes))
        .route("/api/v/{vault}/query/sql", post(routes::execute_sql_query))
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
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind notesmith-http to {bind}"))?;
    axum::serve(listener, build_router_with_shared_state(state))
        .await
        .context("failed to serve notesmith-http")
}

pub fn build_app_state(config: &GlobalConfig) -> anyhow::Result<AppState> {
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
        vaults.insert(
            vault_name.clone(),
            VaultState {
                cache,
                search_index,
                engine,
                root,
            },
        );
    }

    Ok(AppState { vaults })
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
