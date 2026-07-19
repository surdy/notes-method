use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use notesmith_config::{HooksConfig, PeriodicNoteMatch};
use notesmith_core::PeriodKind;
use notesmith_hooks::{HookEvent, HookPayload, HookRunner, diff_fields, fire_hook};
use notesmith_index::VaultCache;
use tokio::sync::broadcast;

use crate::events::{EventReceiver, EventType, VaultEvent};

#[derive(Clone)]
pub struct HookVaultContext {
    pub vault_name: String,
    pub vault_root: PathBuf,
    pub hooks_config: HooksConfig,
    /// Cache handle used to snapshot note fields for on_field_change diffs.
    pub cache: Arc<VaultCache>,
}

/// Last-seen field values per note, per vault, for on_field_change diffing.
/// Seeded from the cache at listener start so the first change after a
/// daemon restart still fires.
type FieldBaselines = HashMap<String, HashMap<String, HashMap<String, String>>>;

pub fn start_hook_listener(
    mut event_rx: EventReceiver,
    vaults: Vec<HookVaultContext>,
    runner: HookRunner,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut baselines: FieldBaselines = vaults
            .iter()
            .filter(|ctx| ctx.hooks_config.on_field_change.is_some())
            .map(|ctx| (ctx.vault_name.clone(), load_vault_fields(ctx)))
            .collect();

        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    handle_event(&event, &vaults, &runner, &mut baselines).await;
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

async fn handle_event(
    event: &VaultEvent,
    vaults: &[HookVaultContext],
    runner: &HookRunner,
    baselines: &mut FieldBaselines,
) {
    let Some(ctx) = vaults.iter().find(|vault| vault.vault_name == event.vault) else {
        return;
    };

    dispatch_field_change(ctx, event, runner, baselines).await;

    let (hook_event, script) = match event.event_type {
        EventType::NoteCreated => (
            HookEvent::OnNoteCreate,
            ctx.hooks_config.on_note_create.as_deref(),
        ),
        EventType::NoteUpdated => (
            HookEvent::OnNoteUpdate,
            ctx.hooks_config.on_note_update.as_deref(),
        ),
        EventType::DailyCreated | EventType::PeriodicCreated => (
            HookEvent::OnPeriodicCreate,
            ctx.hooks_config
                .on_periodic_create
                .as_deref()
                .or(ctx.hooks_config.on_daily_create.as_deref()),
        ),
        _ => return,
    };

    let Some(script_path) = script else {
        return;
    };

    let periodic = if hook_event == HookEvent::OnPeriodicCreate {
        detect_periodic_note(ctx, &event.path)
    } else {
        None
    };

    let payload = HookPayload {
        event: hook_event.as_str().to_string(),
        vault: event.vault.clone(),
        path: event.path.clone(),
        frontmatter: None,
        source: None,
        rule_id: None,
        from_path: None,
        to_path: None,
        mutations: None,
        period_kind: periodic.as_ref().map(|periodic| periodic.kind.to_string()),
        period_key: periodic.as_ref().map(|periodic| periodic.key.clone()),
        old_status: None,
        new_status: None,
        task_text: None,
        changes: None,
    };

    fire_hook(runner, &ctx.vault_root, script_path, payload).await;
}

/// Diff the note's watched fields against the last-seen baseline and fire
/// on_field_change when they differ. Creation seeds the baseline without
/// firing; deletion drops it.
async fn dispatch_field_change(
    ctx: &HookVaultContext,
    event: &VaultEvent,
    runner: &HookRunner,
    baselines: &mut FieldBaselines,
) {
    let Some(script_path) = ctx.hooks_config.on_field_change.as_deref() else {
        return;
    };

    let vault_baseline = baselines.entry(ctx.vault_name.clone()).or_default();
    match event.event_type {
        EventType::NoteDeleted => {
            vault_baseline.remove(&event.path);
            return;
        }
        EventType::NoteCreated | EventType::NoteCaptured | EventType::NoteClipped => {
            let fields = load_note_fields(ctx, &event.path);
            vault_baseline.insert(event.path.clone(), fields);
            return;
        }
        EventType::NoteUpdated | EventType::NoteMoved => {}
        _ => return,
    }

    let new_fields = load_note_fields(ctx, &event.path);
    let old_fields = vault_baseline
        .insert(event.path.clone(), new_fields.clone())
        .unwrap_or_default();

    let watch = ctx.hooks_config.watch_fields.clone();
    let changes = diff_fields(&old_fields, &new_fields, watch.as_deref());
    if changes.is_empty() {
        return;
    }

    let payload = HookPayload {
        event: HookEvent::OnFieldChange.as_str().to_string(),
        vault: ctx.vault_name.clone(),
        path: event.path.clone(),
        frontmatter: None,
        source: None,
        rule_id: None,
        from_path: None,
        to_path: None,
        mutations: None,
        period_kind: None,
        period_key: None,
        old_status: None,
        new_status: None,
        task_text: None,
        changes: Some(changes),
    };

    fire_hook(runner, &ctx.vault_root, script_path, payload).await;
}

/// Load the (watched) fields of a single note from the cache, last value wins
/// per key.
fn load_note_fields(ctx: &HookVaultContext, path: &str) -> HashMap<String, String> {
    load_fields(ctx, Some(path))
        .remove(path)
        .unwrap_or_default()
}

/// Load the (watched) fields of every note in the vault, keyed by note path.
fn load_vault_fields(ctx: &HookVaultContext) -> HashMap<String, HashMap<String, String>> {
    load_fields(ctx, None)
}

fn load_fields(
    ctx: &HookVaultContext,
    path: Option<&str>,
) -> HashMap<String, HashMap<String, String>> {
    let watch = ctx.hooks_config.watch_fields.as_deref();
    let conn = ctx.cache.connection();
    let mut sql = String::from(
        "SELECT note_path, key, value FROM fields WHERE vault_name = ?1 AND source = 'frontmatter'",
    );
    if path.is_some() {
        sql.push_str(" AND note_path = ?2");
    }
    sql.push_str(" ORDER BY rowid");

    let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return result;
    };
    let rows = match path {
        Some(path) => stmt.query_map(rusqlite::params![ctx.vault_name, path], row_to_field),
        None => stmt.query_map(rusqlite::params![ctx.vault_name], row_to_field),
    };
    let Ok(rows) = rows else {
        return result;
    };
    for row in rows.flatten() {
        let (note_path, key, value) = row;
        if let Some(watch) = watch {
            if !watch.iter().any(|w| *w == key) {
                continue;
            }
        }
        result.entry(note_path).or_default().insert(key, value);
    }
    result
}

fn row_to_field(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn detect_periodic_note(ctx: &HookVaultContext, path: &str) -> Option<PeriodicNoteMatch> {
    let config = notesmith_config::migration::load_and_migrate(&ctx.vault_root).ok()?;
    config.periodic.match_note_path(path).or_else(|| {
        if path.ends_with(".md") {
            let stem = path.rsplit('/').next()?.strip_suffix(".md")?;
            let (period_start, period_end) = PeriodKind::Daily.bounds_for_key(stem)?;
            Some(PeriodicNoteMatch {
                kind: PeriodKind::Daily,
                key: stem.to_string(),
                period_start,
                period_end,
            })
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventType, VaultEvent};
    use notesmith_core::VaultEngine;
    use notesmith_vault::NativeVaultEngine;
    use std::time::Duration;

    fn reindex(cache: &VaultCache, root: &std::path::Path) {
        let notes = NativeVaultEngine.scan(root).unwrap();
        cache.reindex("test-vault", &notes).unwrap();
    }

    fn note_updated(path: &str) -> VaultEvent {
        VaultEvent {
            id: None,
            vault: "test-vault".to_string(),
            event_type: EventType::NoteUpdated,
            path: path.to_string(),
            timestamp: "2026-07-19T00:00:00Z".to_string(),
            config: None,
            hash: None,
        }
    }

    async fn wait_for(path: &std::path::Path, appear: bool) -> bool {
        for _ in 0..40 {
            if path.exists() == appear {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    #[tokio::test]
    async fn field_change_hook_fires_on_watched_field_transition() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("Streams")).unwrap();
        std::fs::write(
            root.join("Streams/renewal.md"),
            "---\nkind: stream\nstatus: active\n---\n# Renewal\n",
        )
        .unwrap();
        std::fs::write(root.join("hook.sh"), "cat > hook-out.json\n").unwrap();

        let cache = Arc::new(VaultCache::open_in_memory().unwrap());
        reindex(&cache, root);

        let ctx = HookVaultContext {
            vault_name: "test-vault".to_string(),
            vault_root: root.to_path_buf(),
            hooks_config: HooksConfig {
                on_field_change: Some("hook.sh".to_string()),
                watch_fields: Some(vec!["status".to_string()]),
                ..Default::default()
            },
            cache: cache.clone(),
        };
        let (tx, rx) = broadcast::channel(16);
        let _listener = start_hook_listener(rx, vec![ctx], HookRunner::default());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Body-only change: watched fields untouched, hook must not fire.
        std::fs::write(
            root.join("Streams/renewal.md"),
            "---\nkind: stream\nstatus: active\n---\n# Renewal\n\nMore notes.\n",
        )
        .unwrap();
        reindex(&cache, root);
        tx.send(note_updated("Streams/renewal.md")).unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            !root.join("hook-out.json").exists(),
            "hook must not fire for unwatched changes"
        );

        // Watched transition: active -> blocked fires with the diff. The
        // baseline was seeded from the cache at listener start, so this first
        // real transition is caught.
        std::fs::write(
            root.join("Streams/renewal.md"),
            "---\nkind: stream\nstatus: blocked\n---\n# Renewal\n\nMore notes.\n",
        )
        .unwrap();
        reindex(&cache, root);
        tx.send(note_updated("Streams/renewal.md")).unwrap();
        assert!(
            wait_for(&root.join("hook-out.json"), true).await,
            "hook should have fired for status change"
        );

        let payload: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("hook-out.json")).unwrap())
                .unwrap();
        assert_eq!(payload["event"], "on_field_change");
        assert_eq!(payload["path"], "Streams/renewal.md");
        let changes = payload["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["key"], "status");
        assert_eq!(changes[0]["old"], "active");
        assert_eq!(changes[0]["new"], "blocked");
        assert_eq!(changes[0]["action"], "change");
    }
}
