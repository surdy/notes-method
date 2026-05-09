use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use notesmith_core::{Note, VaultEngine, VaultName, VaultPath};
use notesmith_vault::parse_note;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::{sync::mpsc, task::JoinHandle, time::timeout};

use crate::server::SharedAppState;

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(300);

pub struct VaultWatcher {
    _watcher: RecommendedWatcher,
    _task: JoinHandle<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeAction {
    Upsert,
    Delete,
}

pub async fn watch_all_vaults(state: SharedAppState) -> anyhow::Result<Vec<VaultWatcher>> {
    let vault_names = {
        let state = state.read().await;
        state.vaults.keys().cloned().collect::<Vec<_>>()
    };

    let mut watchers = Vec::with_capacity(vault_names.len());
    for vault_name in vault_names {
        watchers.push(watch_vault(state.clone(), vault_name).await?);
    }
    Ok(watchers)
}

pub async fn watch_vault(
    state: SharedAppState,
    vault_name: String,
) -> anyhow::Result<VaultWatcher> {
    let root = {
        let state = state.read().await;
        state
            .vaults
            .get(&vault_name)
            .map(|vault| vault.root.clone())
            .with_context(|| format!("vault not found: {vault_name}"))?
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = sender.send(event);
        },
        notify::Config::default(),
    )?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let task = tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let mut pending = HashMap::new();
            record_event(&mut pending, &root, event);

            while let Ok(Some(event)) = timeout(DEBOUNCE_WINDOW, receiver.recv()).await {
                record_event(&mut pending, &root, event);
            }

            if let Err(error) = process_pending(&state, &vault_name, &root, pending).await {
                tracing::warn!("watcher update failed for {vault_name}: {error}");
            }
        }
    });

    Ok(VaultWatcher {
        _watcher: watcher,
        _task: task,
    })
}

fn record_event(
    pending: &mut HashMap<PathBuf, ChangeAction>,
    root: &Path,
    event: notify::Result<Event>,
) {
    let Ok(event) = event else {
        return;
    };
    let Some(action) = classify_event(&event.kind) else {
        return;
    };

    for path in event.paths {
        if path.starts_with(root) && is_markdown_path(&path) {
            pending.insert(path, action);
        }
    }
}

fn classify_event(kind: &EventKind) -> Option<ChangeAction> {
    match kind {
        EventKind::Create(_) | EventKind::Modify(_) => Some(ChangeAction::Upsert),
        EventKind::Remove(_) => Some(ChangeAction::Delete),
        _ => None,
    }
}

async fn process_pending(
    state: &SharedAppState,
    vault_name: &str,
    root: &Path,
    pending: HashMap<PathBuf, ChangeAction>,
) -> anyhow::Result<()> {
    let state = state.read().await;
    let Some(vault) = state.vaults.get(vault_name) else {
        return Ok(());
    };
    for (absolute_path, action) in pending {
        let relative_path = absolute_path
            .strip_prefix(root)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .with_context(|| {
                format!(
                    "failed to compute relative path for {}",
                    absolute_path.display()
                )
            })?;

        match action {
            ChangeAction::Delete => {
                vault.cache.remove_note(vault_name, &relative_path)?;
            }
            ChangeAction::Upsert => {
                if !absolute_path.exists() {
                    vault.cache.remove_note(vault_name, &relative_path)?;
                    continue;
                }

                let note = read_note(
                    vault_name,
                    root,
                    &vault_path(relative_path.clone()),
                    &vault.engine,
                )?;
                vault.cache.update_note(vault_name, &note)?;
            }
        }
    }

    Ok(())
}

fn read_note(
    vault_name: &str,
    root: &Path,
    path: &VaultPath,
    engine: &impl VaultEngine,
) -> anyhow::Result<Note> {
    let content = engine.read(root, path).map_err(anyhow::Error::from)?;
    let vault_id = VaultName::new(vault_name.to_string());
    let parsed = parse_note(&content, &vault_id, path);

    Ok(Note {
        vault: vault_id,
        path: path.clone(),
        frontmatter: parsed.frontmatter,
        raw_frontmatter: parsed.raw_frontmatter,
        body: parsed.body,
        tasks: parsed.tasks,
        links: parsed.links,
        inline_fields: parsed.inline_fields,
        blocks: parsed.blocks,
        hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
    })
}

fn vault_path(path: String) -> VaultPath {
    VaultPath::new(path)
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}
