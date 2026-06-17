use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use notesmith_config::GlobalConfig;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
};

use crate::{
    events::{self, EventType},
    server::{SharedAppState, create_vault_state},
    watcher::watch_vault,
};

pub type SharedVaultWatchers = Arc<Mutex<HashMap<String, crate::watcher::VaultWatcher>>>;

const CONFIG_DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

pub struct GlobalConfigWatcher {
    _watcher: RecommendedWatcher,
    _task: JoinHandle<()>,
}

#[derive(Debug, PartialEq, Eq)]
struct VaultChangePlan {
    to_add: Vec<(String, PathBuf)>,
    to_remove: Vec<String>,
    event_targets: Vec<String>,
}

pub async fn watch_global_config(
    state: SharedAppState,
    vault_watchers: SharedVaultWatchers,
) -> anyhow::Result<Option<GlobalConfigWatcher>> {
    let config_path = {
        let state = state.read().await;
        state.global_config_path.clone()
    };
    let Some(config_dir) = config_path.parent().map(Path::to_path_buf) else {
        return Ok(None);
    };

    // Ensure the config directory exists so the filesystem watcher can attach
    // to it. On a fresh install this directory won't exist yet, and `notify`
    // returns an error when asked to watch a non-existent path.
    std::fs::create_dir_all(&config_dir)?;

    let (sender, mut receiver) = mpsc::channel::<notify::Result<notify::Event>>(64);
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = sender.blocking_send(event);
        },
        notify::Config::default(),
    )?;
    watcher.watch(&config_dir, RecursiveMode::NonRecursive)?;

    let task = tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let Ok(event) = event else {
                continue;
            };
            if !affects_config(&event, &config_path) {
                continue;
            }

            tokio::time::sleep(CONFIG_DEBOUNCE_WINDOW).await;
            while receiver.try_recv().is_ok() {}

            if let Err(error) = reconcile_vaults(&state, &vault_watchers, &config_path).await {
                tracing::error!("failed to reconcile vault config: {error}");
            }
        }
    });

    Ok(Some(GlobalConfigWatcher {
        _watcher: watcher,
        _task: task,
    }))
}

fn plan_vault_changes(
    current_vaults: &HashMap<String, PathBuf>,
    new_config: &GlobalConfig,
) -> VaultChangePlan {
    let mut to_add = new_config
        .vaults
        .iter()
        .filter(|(name, registration)| {
            current_vaults
                .get(*name)
                .is_none_or(|current_root| current_root != &registration.path)
        })
        .map(|(name, registration)| (name.clone(), registration.path.clone()))
        .collect::<Vec<_>>();
    to_add.sort_by(|left, right| left.0.cmp(&right.0));

    let mut to_remove = current_vaults
        .iter()
        .filter(|(name, root)| {
            new_config
                .vaults
                .get(*name)
                .map(|registration| &registration.path)
                != Some(root)
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    to_remove.sort();

    let event_targets = current_vaults
        .keys()
        .cloned()
        .chain(to_add.iter().map(|(name, _)| name.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    VaultChangePlan {
        to_add,
        to_remove,
        event_targets,
    }
}

/// Load a single registered vault into the live engine map and start its
/// filesystem watcher. Shared by the config-watcher reconcile loop and the
/// add-vault HTTP handler so vaults registered via the API are usable on
/// `/api/v/{name}` immediately, without waiting for the config-watcher debounce.
///
/// On success the vault is present in `state.vaults` and a watcher is recorded
/// in `vault_watchers`. On failure (engine could not be built, or the watcher
/// could not start) the vault is not left in the live map.
pub(crate) async fn add_vault_live(
    state: &SharedAppState,
    vault_watchers: &SharedVaultWatchers,
    vault_name: &str,
    vault_path: &Path,
) -> anyhow::Result<()> {
    let vault_state = create_vault_state(vault_name, vault_path)?;
    {
        let mut state = state.write().await;
        state.vaults.insert(vault_name.to_string(), vault_state);
    }

    match watch_vault(state.clone(), vault_name.to_string()).await {
        Ok(watcher) => {
            vault_watchers
                .lock()
                .await
                .insert(vault_name.to_string(), watcher);
            Ok(())
        }
        Err(error) => {
            let mut state = state.write().await;
            state.vaults.remove(vault_name);
            Err(error)
        }
    }
}

async fn reconcile_vaults(
    state: &SharedAppState,
    vault_watchers: &SharedVaultWatchers,
    config_path: &Path,
) -> anyhow::Result<()> {
    let new_config = GlobalConfig::load_from(config_path)?;
    let current_vaults = {
        let state = state.read().await;
        state
            .vaults
            .iter()
            .map(|(name, vault)| (name.clone(), vault.root.clone()))
            .collect::<HashMap<_, _>>()
    };
    let plan = plan_vault_changes(&current_vaults, &new_config);
    if plan.to_add.is_empty() && plan.to_remove.is_empty() {
        return Ok(());
    }

    for vault_name in &plan.to_remove {
        tracing::info!("removing vault: {vault_name}");
        {
            let mut state = state.write().await;
            state.vaults.remove(vault_name);
            if let Ok(mut services) = state.mcp_services.lock() {
                services.remove(&(vault_name.clone(), true));
                services.remove(&(vault_name.clone(), false));
            }
        }
        vault_watchers.lock().await.remove(vault_name);
    }

    for (vault_name, vault_path) in &plan.to_add {
        tracing::info!("adding vault: {vault_name}");
        if let Err(error) = add_vault_live(state, vault_watchers, vault_name, vault_path).await {
            tracing::error!("failed to load vault {vault_name}: {error}");
            continue;
        }
    }

    let (event_tx, event_buffer) = {
        let state = state.read().await;
        (state.event_tx.clone(), state.event_buffer.clone())
    };
    for vault_name in plan.event_targets {
        events::emit(
            &event_tx,
            &event_buffer,
            crate::events::VaultEvent::new(vault_name, EventType::VaultsChanged, ""),
        );
    }

    Ok(())
}

fn affects_config(event: &notify::Event, config_path: &Path) -> bool {
    let Some(config_name) = config_path.file_name() else {
        return false;
    };

    event
        .paths
        .iter()
        .any(|path| path == config_path || path.file_name() == Some(config_name))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs};

    use notesmith_config::{GlobalConfig, VaultRegistration};
    use tokio::{sync::RwLock, time::Duration};

    use super::*;
    use crate::{EventType, build_app_state, watch_vault};

    #[test]
    fn plan_vault_changes_detects_additions_and_removals() {
        let current_vaults = HashMap::from([
            ("old".to_string(), PathBuf::from("/vaults/old")),
            ("work".to_string(), PathBuf::from("/vaults/work")),
        ]);
        let new_config = GlobalConfig {
            daemon: Default::default(),
            default_vault: Some("work".to_string()),
            vaults: BTreeMap::from([
                (
                    "home".to_string(),
                    VaultRegistration {
                        path: PathBuf::from("/vaults/home"),
                    },
                ),
                (
                    "work".to_string(),
                    VaultRegistration {
                        path: PathBuf::from("/vaults/work"),
                    },
                ),
            ]),
            agents: Default::default(),
            mcp: Default::default(),
        };

        let plan = plan_vault_changes(&current_vaults, &new_config);

        assert_eq!(
            plan.to_add,
            vec![("home".to_string(), PathBuf::from("/vaults/home"))]
        );
        assert_eq!(plan.to_remove, vec!["old".to_string()]);
        assert_eq!(
            plan.event_targets,
            vec!["home".to_string(), "old".to_string(), "work".to_string()]
        );
    }

    #[tokio::test]
    async fn reconcile_vaults_updates_state_watchers_and_events() {
        let temp_dir = tempfile::tempdir().unwrap();
        let suffix = temp_dir.path().file_name().unwrap().to_string_lossy();
        let work_name = format!("work-{suffix}");
        let home_name = format!("home-{suffix}");
        let work_root = temp_dir.path().join("work");
        let home_root = temp_dir.path().join("home");
        fs::create_dir_all(work_root.join("Inbox")).unwrap();
        fs::create_dir_all(home_root.join("Inbox")).unwrap();
        fs::write(work_root.join("Inbox/Old.md"), "# Old\n\nlegacy vault\n").unwrap();
        fs::write(home_root.join("Inbox/New.md"), "# New\n\nfresh vault\n").unwrap();

        let config_path = temp_dir.path().join("config").join("config.toml");
        let initial_config = GlobalConfig {
            daemon: Default::default(),
            default_vault: Some(work_name.clone()),
            vaults: BTreeMap::from([(
                work_name.clone(),
                VaultRegistration {
                    path: work_root.clone(),
                },
            )]),
            agents: Default::default(),
            mcp: Default::default(),
        };
        initial_config.save_to(&config_path).unwrap();

        let mut app_state = build_app_state(&initial_config).unwrap();
        app_state.global_config_path = config_path.clone();
        let state = Arc::new(RwLock::new(app_state));
        let vault_watchers: SharedVaultWatchers = Arc::new(Mutex::new(HashMap::new()));

        let watcher = watch_vault(state.clone(), work_name.clone()).await.unwrap();
        vault_watchers
            .lock()
            .await
            .insert(work_name.clone(), watcher);

        let mut event_rx = {
            let state = state.read().await;
            state.event_tx.subscribe()
        };

        let updated_config = GlobalConfig {
            daemon: Default::default(),
            default_vault: Some(home_name.clone()),
            vaults: BTreeMap::from([(
                home_name.clone(),
                VaultRegistration {
                    path: home_root.clone(),
                },
            )]),
            agents: Default::default(),
            mcp: Default::default(),
        };
        updated_config.save_to(&config_path).unwrap();

        reconcile_vaults(&state, &vault_watchers, &config_path)
            .await
            .unwrap();

        let state = state.read().await;
        assert!(!state.vaults.contains_key(&work_name));
        assert!(state.vaults.contains_key(&home_name));
        drop(state);

        let watchers = vault_watchers.lock().await;
        assert!(!watchers.contains_key(&work_name));
        assert!(watchers.contains_key(&home_name));
        drop(watchers);

        let first = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let second = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        let mut received = vec![first, second];
        received.sort_by(|left, right| left.vault.cmp(&right.vault));

        assert_eq!(received[0].event_type, EventType::VaultsChanged);
        assert_eq!(received[0].vault, home_name);
        assert_eq!(received[1].event_type, EventType::VaultsChanged);
        assert_eq!(received[1].vault, work_name);
    }
}
