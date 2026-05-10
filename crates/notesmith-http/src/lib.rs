//! notesmith-http: Axum daemon, REST endpoints, SSE, and static app serving

pub mod events;
pub mod hooks;
pub mod routes;
pub mod scheduler;
pub mod server;
pub mod watcher;

pub use events::{EventSender, EventType, VaultEvent, create_event_channel};
pub use hooks::{HookVaultContext, start_hook_listener};
pub use scheduler::{
    DailyScheduler, catch_up_daily_notes, ensure_daily_note, start_daily_schedulers,
};
pub use server::{
    AppState, SharedAppState, VaultState, build_app_state, build_router, cache_dir_for_vault,
    cache_path_for_vault, search_index_path_for_vault, serve, serve_configured_vaults,
    serve_with_listener,
};
pub use watcher::{VaultWatcher, watch_all_vaults, watch_vault};
