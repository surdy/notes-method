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

use crate::events::{ConfigDetail, EventType};
use crate::server::SharedAppState;
use notesmith_config::VaultConfig;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WatchTarget {
    Note,
    Config(ConfigKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigKey {
    Sidebar,
    Vault,
}

impl ConfigKey {
    fn as_str(self) -> &'static str {
        match self {
            ConfigKey::Sidebar => "sidebar",
            ConfigKey::Vault => "vault",
        }
    }

    fn relative_path(self) -> &'static str {
        match self {
            ConfigKey::Sidebar => ".notesmith/sidebar.yaml",
            ConfigKey::Vault => ".notesmith/vault.toml",
        }
    }
}

pub(crate) fn classify_path(root: &Path, path: &Path) -> Option<WatchTarget> {
    let relative = path.strip_prefix(root).ok()?;
    let notesmith_dir = Path::new(".notesmith");

    if relative.starts_with(notesmith_dir) {
        let after = relative.strip_prefix(notesmith_dir).ok()?;
        if after == Path::new("sidebar.yaml") {
            return Some(WatchTarget::Config(ConfigKey::Sidebar));
        }
        if after == Path::new("vault.toml") {
            return Some(WatchTarget::Config(ConfigKey::Vault));
        }
        return None;
    }

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Some(WatchTarget::Note);
    }

    None
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
    pending: &mut HashMap<PathBuf, (ChangeAction, WatchTarget)>,
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
        if path.starts_with(root) {
            if let Some(target) = classify_path(root, &path) {
                pending.insert(path, (action, target));
            }
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
    pending: HashMap<PathBuf, (ChangeAction, WatchTarget)>,
) -> anyhow::Result<()> {
    let state = state.read().await;
    let Some(vault) = state.vaults.get(vault_name) else {
        return Ok(());
    };
    for (absolute_path, (action, target)) in pending {
        let relative_path = absolute_path
            .strip_prefix(root)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .with_context(|| {
                format!(
                    "failed to compute relative path for {}",
                    absolute_path.display()
                )
            })?;

        match target {
            WatchTarget::Note => {
                handle_note_change(
                    vault,
                    vault_name,
                    &state.event_tx,
                    &absolute_path,
                    &relative_path,
                    root,
                    action,
                )?;
            }
            WatchTarget::Config(key) => {
                handle_config_change(vault, vault_name, &state.event_tx, root, key, action);
            }
        }
    }

    Ok(())
}

fn handle_note_change(
    vault: &crate::server::VaultState,
    vault_name: &str,
    event_tx: &crate::events::EventSender,
    absolute_path: &Path,
    relative_path: &str,
    root: &Path,
    action: ChangeAction,
) -> anyhow::Result<()> {
    match action {
        ChangeAction::Delete => {
            vault.cache.remove_note(vault_name, relative_path)?;
            vault.search_index.remove_note(vault_name, relative_path)?;
            crate::events::emit(
                event_tx,
                crate::events::VaultEvent::new(vault_name, EventType::NoteDeleted, relative_path),
            );
        }
        ChangeAction::Upsert => {
            if !absolute_path.exists() {
                vault.cache.remove_note(vault_name, relative_path)?;
                vault.search_index.remove_note(vault_name, relative_path)?;
                crate::events::emit(
                    event_tx,
                    crate::events::VaultEvent::new(
                        vault_name,
                        EventType::NoteDeleted,
                        relative_path,
                    ),
                );
                return Ok(());
            }

            let note = read_note(
                vault_name,
                root,
                &vault_path(relative_path.to_string()),
                &vault.engine,
            )?;
            vault.cache.update_note(vault_name, &note)?;
            vault.search_index.update_note(vault_name, &note)?;
            crate::events::emit(
                event_tx,
                crate::events::VaultEvent::new(vault_name, EventType::NoteUpdated, relative_path),
            );
        }
    }
    Ok(())
}

fn handle_config_change(
    vault: &crate::server::VaultState,
    vault_name: &str,
    event_tx: &crate::events::EventSender,
    root: &Path,
    key: ConfigKey,
    action: ChangeAction,
) {
    let key_str = key.as_str();
    let rel_path = key.relative_path();

    match action {
        ChangeAction::Delete => {
            crate::events::emit(
                event_tx,
                crate::events::VaultEvent::config_event(
                    vault_name,
                    EventType::ConfigRemoved,
                    rel_path,
                    ConfigDetail {
                        key: key_str.to_string(),
                        status: "removed".to_string(),
                        error: None,
                    },
                ),
            );
        }
        ChangeAction::Upsert => match key {
            ConfigKey::Sidebar => match crate::config_io::load_sidebar_config_from_root(root) {
                Ok(_) => {
                    crate::events::emit(
                        event_tx,
                        crate::events::VaultEvent::config_event(
                            vault_name,
                            EventType::ConfigChanged,
                            rel_path,
                            ConfigDetail {
                                key: key_str.to_string(),
                                status: "changed".to_string(),
                                error: None,
                            },
                        ),
                    );
                }
                Err(err) => {
                    crate::events::emit(
                        event_tx,
                        crate::events::VaultEvent::config_event(
                            vault_name,
                            EventType::ConfigError,
                            rel_path,
                            ConfigDetail {
                                key: key_str.to_string(),
                                status: "error".to_string(),
                                error: Some(err.to_string()),
                            },
                        ),
                    );
                }
            },
            ConfigKey::Vault => match VaultConfig::load_from_vault(root) {
                Ok(new_config) => {
                    vault.vault_config.store(std::sync::Arc::new(new_config));
                    crate::events::emit(
                        event_tx,
                        crate::events::VaultEvent::config_event(
                            vault_name,
                            EventType::ConfigChanged,
                            rel_path,
                            ConfigDetail {
                                key: key_str.to_string(),
                                status: "changed".to_string(),
                                error: None,
                            },
                        ),
                    );
                }
                Err(err) => {
                    crate::events::emit(
                        event_tx,
                        crate::events::VaultEvent::config_event(
                            vault_name,
                            EventType::ConfigError,
                            rel_path,
                            ConfigDetail {
                                key: key_str.to_string(),
                                status: "error".to_string(),
                                error: Some(err.to_string()),
                            },
                        ),
                    );
                }
            },
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classify_path_sidebar_yaml() {
        let root = PathBuf::from("/vault");
        let path = root.join(".notesmith/sidebar.yaml");
        assert_eq!(
            classify_path(&root, &path),
            Some(WatchTarget::Config(ConfigKey::Sidebar))
        );
    }

    #[test]
    fn classify_path_vault_toml() {
        let root = PathBuf::from("/vault");
        let path = root.join(".notesmith/vault.toml");
        assert_eq!(
            classify_path(&root, &path),
            Some(WatchTarget::Config(ConfigKey::Vault))
        );
    }

    #[test]
    fn classify_path_swap_file_ignored() {
        let root = PathBuf::from("/vault");
        let path = root.join(".notesmith/sidebar.yaml.swp");
        assert_eq!(classify_path(&root, &path), None);
    }

    #[test]
    fn classify_path_markdown_note() {
        let root = PathBuf::from("/vault");
        let path = root.join("Notes/foo.md");
        assert_eq!(classify_path(&root, &path), Some(WatchTarget::Note));
    }

    #[test]
    fn classify_path_txt_ignored() {
        let root = PathBuf::from("/vault");
        let path = root.join("Notes/foo.txt");
        assert_eq!(classify_path(&root, &path), None);
    }

    #[test]
    fn classify_path_other_notesmith_file_ignored() {
        let root = PathBuf::from("/vault");
        let path = root.join(".notesmith/other.yaml");
        assert_eq!(classify_path(&root, &path), None);
    }

    #[test]
    fn classify_path_outside_root_returns_none() {
        let root = PathBuf::from("/vault");
        let path = PathBuf::from("/other/foo.md");
        assert_eq!(classify_path(&root, &path), None);
    }

    #[test]
    fn config_key_as_str() {
        assert_eq!(ConfigKey::Sidebar.as_str(), "sidebar");
        assert_eq!(ConfigKey::Vault.as_str(), "vault");
    }

    #[test]
    fn config_key_relative_path() {
        assert_eq!(
            ConfigKey::Sidebar.relative_path(),
            ".notesmith/sidebar.yaml"
        );
        assert_eq!(ConfigKey::Vault.relative_path(), ".notesmith/vault.toml");
    }

    #[test]
    fn vault_config_store_updates_arcswap() {
        use arc_swap::ArcSwap;
        use std::sync::Arc;

        let config = VaultConfig {
            name: "test".to_string(),
            capture: Default::default(),
            daily: Default::default(),
            editor: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
            homepage: None,
        };
        let swappable = ArcSwap::from_pointee(config);
        assert_eq!(swappable.load().name, "test");

        let new_config = VaultConfig {
            name: "updated".to_string(),
            capture: Default::default(),
            daily: Default::default(),
            editor: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
            homepage: None,
        };
        swappable.store(Arc::new(new_config));
        assert_eq!(swappable.load().name, "updated");
    }

    #[test]
    fn arcswap_preserves_value_when_not_stored() {
        use arc_swap::ArcSwap;

        let config = VaultConfig {
            name: "original".to_string(),
            capture: Default::default(),
            daily: Default::default(),
            editor: Default::default(),
            git: Default::default(),
            hooks: Default::default(),
            homepage: None,
        };
        let swappable = ArcSwap::from_pointee(config);

        // Simulate error path: don't call store
        assert_eq!(swappable.load().name, "original");
    }
}
