use std::path::PathBuf;

use notesmith_config::HooksConfig;
use notesmith_hooks::{HookEvent, HookPayload, HookRunner, fire_hook};
use tokio::sync::broadcast;

use crate::events::{EventReceiver, EventType, VaultEvent};

#[derive(Clone)]
pub struct HookVaultContext {
    pub vault_name: String,
    pub vault_root: PathBuf,
    pub hooks_config: HooksConfig,
}

pub fn start_hook_listener(
    mut event_rx: EventReceiver,
    vaults: Vec<HookVaultContext>,
    runner: HookRunner,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    handle_event(&event, &vaults, &runner).await;
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "hook listener lagged behind event stream");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("event channel closed, stopping hook listener");
                    break;
                }
            }
        }
    })
}

async fn handle_event(event: &VaultEvent, vaults: &[HookVaultContext], runner: &HookRunner) {
    let Some(ctx) = vaults.iter().find(|vault| vault.vault_name == event.vault) else {
        return;
    };

    let (hook_event, script) = match event.event_type {
        EventType::NoteCreated => (
            HookEvent::OnNoteCreate,
            ctx.hooks_config.on_note_create.as_deref(),
        ),
        EventType::DailyCreated => (
            HookEvent::OnDailyCreate,
            ctx.hooks_config.on_daily_create.as_deref(),
        ),
        _ => return,
    };

    let Some(script_path) = script else {
        return;
    };

    let payload = HookPayload {
        event: hook_event.as_str().to_string(),
        vault: event.vault.clone(),
        path: event.path.clone(),
        frontmatter: None,
        source: None,
    };

    fire_hook(runner, &ctx.vault_root, script_path, payload).await;
}
