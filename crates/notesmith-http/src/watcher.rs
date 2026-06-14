use std::{
    collections::HashMap,
    ffi::CString,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use anyhow::Context;
use notesmith_core::{Note, VaultEngine, VaultName, VaultPath};
use notesmith_vault::parse_note;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::timeout,
};

use crate::events::{ConfigDetail, EventType};
use crate::server::SharedAppState;
use notesmith_config::migration;

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(300);
const CANARY_INTERVAL: Duration = Duration::from_secs(300);
const NETWORK_DRIVE_MESSAGE: &str = "Network drive detected — updates may take up to 30s";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherHealth {
    Healthy = 0,
    Degraded = 1,
    Polling = 2,
}

impl WatcherHealth {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Polling => "polling",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Degraded,
            2 => Self::Polling,
            _ => Self::Healthy,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatcherState {
    health: Arc<AtomicU8>,
    message: Arc<Mutex<Option<String>>>,
}

impl WatcherState {
    pub fn new() -> Self {
        Self {
            health: Arc::new(AtomicU8::new(WatcherHealth::Healthy as u8)),
            message: Arc::new(Mutex::new(None)),
        }
    }

    pub fn health(&self) -> WatcherHealth {
        WatcherHealth::from_u8(self.health.load(Ordering::Relaxed))
    }

    pub fn message(&self) -> Option<String> {
        self.message
            .lock()
            .expect("watcher state mutex poisoned")
            .clone()
    }

    pub fn set_health(&self, health: WatcherHealth, message: Option<String>) {
        self.health.store(health as u8, Ordering::Relaxed);
        *self.message.lock().expect("watcher state mutex poisoned") = message;
    }
}

impl Default for WatcherState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VaultWatcher {
    _watcher: RecommendedWatcher,
    _task: JoinHandle<()>,
    _canary_task: Option<JoinHandle<()>>,
    pub state: WatcherState,
}

impl Drop for VaultWatcher {
    fn drop(&mut self) {
        self._task.abort();
        if let Some(canary_task) = self._canary_task.take() {
            canary_task.abort();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeAction {
    Upsert,
    Delete,
}

struct EventContext<'a> {
    vault_name: &'a str,
    event_tx: &'a crate::events::EventSender,
    event_buffer: &'a crate::events::EventBuffer,
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
    log_watch_limits();
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
    log_watch_limits();
    let (root, watcher_state, shutdown_rx) = {
        let state = state.read().await;
        let vault = state
            .vaults
            .get(&vault_name)
            .with_context(|| format!("vault not found: {vault_name}"))?;
        (
            vault.root.clone(),
            vault.watcher_state.clone(),
            state.shutdown_rx.clone(),
        )
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    if is_network_mount(&root) {
        tracing::warn!(
            "vault {vault_name} is on a network filesystem; watcher status set to polling"
        );
        watcher_state.set_health(
            WatcherHealth::Polling,
            Some(NETWORK_DRIVE_MESSAGE.to_string()),
        );
    } else {
        watcher_state.set_health(WatcherHealth::Healthy, None);
    }

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = sender.send(event);
        },
        notify::Config::default(),
    )?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let task_state = state.clone();
    let task_vault_name = vault_name.clone();
    let task_root = root.clone();
    let task = tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let mut pending = HashMap::new();
            record_event(&mut pending, &task_root, event);

            while let Ok(Some(event)) = timeout(DEBOUNCE_WINDOW, receiver.recv()).await {
                record_event(&mut pending, &task_root, event);
            }

            if let Err(error) =
                process_pending(&task_state, &task_vault_name, &task_root, pending).await
            {
                tracing::warn!("watcher update failed for {task_vault_name}: {error}");
            }
        }
    });
    let canary_task = Some(spawn_canary_check(
        state.clone(),
        vault_name.clone(),
        root.clone(),
        watcher_state.clone(),
        shutdown_rx,
    ));

    Ok(VaultWatcher {
        _watcher: watcher,
        _task: task,
        _canary_task: canary_task,
        state: watcher_state,
    })
}

fn spawn_canary_check(
    state: SharedAppState,
    vault_name: String,
    root: PathBuf,
    watcher_state: WatcherState,
    mut shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CANARY_INTERVAL);
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if !vault_exists(&state, &vault_name).await {
                        break;
                    }

                    if let Err(error) = run_canary_check_once(&state, &vault_name, &root, &watcher_state).await {
                        tracing::warn!("canary check failed for {vault_name}: {error}");
                    }
                }
                changed = shutdown_rx.changed() => match changed {
                    Ok(()) if *shutdown_rx.borrow() => break,
                    Ok(()) => {}
                    Err(_) => break,
                }
            }
        }
    })
}

async fn run_canary_check_once(
    state: &SharedAppState,
    vault_name: &str,
    root: &Path,
    watcher_state: &WatcherState,
) -> anyhow::Result<()> {
    let disk_count = count_md_files(root)?;
    let cached_count = cached_note_count(state, vault_name).await;
    let diff = disk_count.abs_diff(cached_count);

    if diff == 0 {
        return Ok(());
    }

    tracing::warn!(
        "canary check for {vault_name}: disk has {disk_count} notes, cache has {cached_count} (diff: {diff}); triggering rescan"
    );

    let previous_health = watcher_state.health();
    let previous_message = watcher_state.message();
    watcher_state.set_health(
        WatcherHealth::Degraded,
        Some(format!("Detected {diff} missed changes, resyncing…")),
    );

    match trigger_rescan(state, vault_name, root).await {
        Ok(()) => {
            watcher_state.set_health(previous_health, previous_message);
            Ok(())
        }
        Err(error) => {
            watcher_state.set_health(
                WatcherHealth::Degraded,
                Some(format!(
                    "Detected {diff} missed changes; automatic refresh failed"
                )),
            );
            Err(error)
        }
    }
}

async fn vault_exists(state: &SharedAppState, vault_name: &str) -> bool {
    state.read().await.vaults.contains_key(vault_name)
}

fn count_md_files(root: &Path) -> anyhow::Result<usize> {
    let mut count = 0;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0 || !entry.file_name().to_string_lossy().starts_with('.')
        })
    {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            count += 1;
        }
    }
    Ok(count)
}

async fn cached_note_count(state: &SharedAppState, vault_name: &str) -> usize {
    let state = state.read().await;
    state
        .vaults
        .get(vault_name)
        .and_then(|vault| {
            vault
                .cache
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM notes WHERE vault_name = ?1",
                    [vault_name],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
        })
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or(0)
}

async fn trigger_rescan(
    state: &SharedAppState,
    vault_name: &str,
    root: &Path,
) -> anyhow::Result<()> {
    let engine = notesmith_vault::NativeVaultEngine;
    let notes = engine
        .scan(root)
        .with_context(|| format!("failed to rescan vault {vault_name}"))?;
    let state = state.read().await;
    let vault = state
        .vaults
        .get(vault_name)
        .with_context(|| format!("vault not found: {vault_name}"))?;
    vault
        .cache
        .reindex_with_periodic(vault_name, &notes, &vault.vault_config.load().periodic)?;
    vault.search_index.reindex(vault_name, &notes)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn is_network_mount(path: &Path) -> bool {
    use std::{ffi::CStr, mem::MaybeUninit};

    let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
        Ok(path) => path,
        Err(_) => return false,
    };

    let mut stat = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: statfs writes the provided buffer on success and does not retain the pointer.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return false;
    }
    // SAFETY: statfs succeeded, so the structure is initialized.
    let stat = unsafe { stat.assume_init() };
    // SAFETY: f_fstypename is a null-terminated C string provided by the kernel.
    let fs_type = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) };
    matches!(
        fs_type.to_string_lossy().as_ref(),
        "nfs" | "smbfs" | "afpfs" | "webdav" | "cifs"
    )
}

#[cfg(target_os = "linux")]
fn is_network_mount(path: &Path) -> bool {
    use std::mem::MaybeUninit;

    const NFS_SUPER_MAGIC: libc::c_long = 0x6969;
    const SMB_SUPER_MAGIC: libc::c_long = 0x517B;
    const CIFS_MAGIC: libc::c_long = 0xFF53_4D42;
    const SMB2_MAGIC: libc::c_long = 0xFE53_4D42;

    let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
        Ok(path) => path,
        Err(_) => return false,
    };

    let mut stat = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: statfs writes the provided buffer on success and does not retain the pointer.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return false;
    }
    // SAFETY: statfs succeeded, so the structure is initialized.
    let stat = unsafe { stat.assume_init() };

    matches!(
        stat.f_type,
        NFS_SUPER_MAGIC | SMB_SUPER_MAGIC | CIFS_MAGIC | SMB2_MAGIC
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn is_network_mount(_path: &Path) -> bool {
    false
}

fn log_watch_limits() {
    #[cfg(target_os = "linux")]
    {
        static LOG_ONCE: std::sync::Once = std::sync::Once::new();

        LOG_ONCE.call_once(|| {
            if let Ok(limit) = std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches") {
                let limit = limit.trim();
                tracing::info!("inotify max_user_watches: {limit}");
                if let Ok(num) = limit.parse::<u64>() {
                    if num < 50_000 {
                        tracing::warn!(
                            "inotify watch limit ({num}) is low. Large vaults may miss events. Increase with: echo 524288 | sudo tee /proc/sys/fs/inotify/max_user_watches"
                        );
                    }
                }
            }
        });
    }
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
    let event_context = EventContext {
        vault_name,
        event_tx: &state.event_tx,
        event_buffer: &state.event_buffer,
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
                    &event_context,
                    &absolute_path,
                    &relative_path,
                    root,
                    action,
                )?;
            }
            WatchTarget::Config(key) => {
                handle_config_change(vault, &event_context, root, key, action);
            }
        }
    }

    Ok(())
}

fn handle_note_change(
    vault: &crate::server::VaultState,
    event_context: &EventContext<'_>,
    absolute_path: &Path,
    relative_path: &str,
    root: &Path,
    action: ChangeAction,
) -> anyhow::Result<()> {
    match action {
        ChangeAction::Delete => {
            vault
                .cache
                .remove_note(event_context.vault_name, relative_path)?;
            vault
                .search_index
                .remove_note(event_context.vault_name, relative_path)?;
            crate::events::emit(
                event_context.event_tx,
                event_context.event_buffer,
                crate::events::VaultEvent::new(
                    event_context.vault_name,
                    EventType::NoteDeleted,
                    relative_path,
                ),
            );
        }
        ChangeAction::Upsert => {
            if !absolute_path.exists() {
                vault
                    .cache
                    .remove_note(event_context.vault_name, relative_path)?;
                vault
                    .search_index
                    .remove_note(event_context.vault_name, relative_path)?;
                crate::events::emit(
                    event_context.event_tx,
                    event_context.event_buffer,
                    crate::events::VaultEvent::new(
                        event_context.vault_name,
                        EventType::NoteDeleted,
                        relative_path,
                    ),
                );
                return Ok(());
            }

            let note = read_note(
                event_context.vault_name,
                root,
                &vault_path(relative_path.to_string()),
                &vault.engine,
            )?;
            vault.cache.update_note_with_periodic(
                event_context.vault_name,
                &note,
                &vault.vault_config.load().periodic,
            )?;
            vault
                .search_index
                .update_note(event_context.vault_name, &note)?;
            crate::events::emit(
                event_context.event_tx,
                event_context.event_buffer,
                crate::events::VaultEvent::new(
                    event_context.vault_name,
                    EventType::NoteUpdated,
                    relative_path,
                )
                .with_hash(note.hash.clone()),
            );
        }
    }
    Ok(())
}

fn handle_config_change(
    vault: &crate::server::VaultState,
    event_context: &EventContext<'_>,
    root: &Path,
    key: ConfigKey,
    action: ChangeAction,
) {
    let key_str = key.as_str();
    let rel_path = key.relative_path();

    match action {
        ChangeAction::Delete => {
            crate::events::emit(
                event_context.event_tx,
                event_context.event_buffer,
                crate::events::VaultEvent::config_event(
                    event_context.vault_name,
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
                        event_context.event_tx,
                        event_context.event_buffer,
                        crate::events::VaultEvent::config_event(
                            event_context.vault_name,
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
                        event_context.event_tx,
                        event_context.event_buffer,
                        crate::events::VaultEvent::config_event(
                            event_context.vault_name,
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
            ConfigKey::Vault => match migration::load_and_migrate(root) {
                Ok(new_config) => {
                    vault.vault_config.store(std::sync::Arc::new(new_config));
                    crate::events::emit(
                        event_context.event_tx,
                        event_context.event_buffer,
                        crate::events::VaultEvent::config_event(
                            event_context.vault_name,
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
                        event_context.event_tx,
                        event_context.event_buffer,
                        crate::events::VaultEvent::config_event(
                            event_context.vault_name,
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
    Ok(parse_note(&vault_id, path, &content))
}

fn vault_path(path: String) -> VaultPath {
    VaultPath::new(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        sync::{Arc, atomic::AtomicUsize},
    };

    use arc_swap::ArcSwap;
    use chrono::Utc;
    use notesmith_config::VaultConfig;
    use notesmith_index::{SearchIndex, VaultCache};
    use notesmith_vault::NativeVaultEngine;
    use tokio::sync::RwLock;

    use crate::server::{AppState, VaultState};

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
    fn watcher_health_as_str_and_from_u8() {
        assert_eq!(WatcherHealth::Healthy.as_str(), "healthy");
        assert_eq!(WatcherHealth::Degraded.as_str(), "degraded");
        assert_eq!(WatcherHealth::Polling.as_str(), "polling");
        assert_eq!(WatcherHealth::from_u8(0), WatcherHealth::Healthy);
        assert_eq!(WatcherHealth::from_u8(1), WatcherHealth::Degraded);
        assert_eq!(WatcherHealth::from_u8(2), WatcherHealth::Polling);
        assert_eq!(WatcherHealth::from_u8(99), WatcherHealth::Healthy);
    }

    #[test]
    fn watcher_state_tracks_health_and_message() {
        let state = WatcherState::new();
        assert_eq!(state.health(), WatcherHealth::Healthy);
        assert_eq!(state.message(), None);

        state.set_health(
            WatcherHealth::Degraded,
            Some("Detected missed changes".to_string()),
        );

        assert_eq!(state.health(), WatcherHealth::Degraded);
        assert_eq!(state.message().as_deref(), Some("Detected missed changes"));
    }

    #[test]
    fn is_network_mount_returns_false_for_local_temp_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(!is_network_mount(temp_dir.path()));
    }

    #[test]
    fn count_md_files_ignores_hidden_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp_dir.path().join("Inbox")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".git")).unwrap();
        fs::write(temp_dir.path().join("Inbox/One.md"), "# One\n").unwrap();
        fs::write(temp_dir.path().join("Inbox/Two.MD"), "# Two\n").unwrap();
        fs::write(temp_dir.path().join("Inbox/skip.txt"), "skip").unwrap();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden\n").unwrap();
        fs::write(temp_dir.path().join(".git/ignored.md"), "# Ignored\n").unwrap();

        assert_eq!(count_md_files(temp_dir.path()).unwrap(), 2);
    }

    #[tokio::test]
    async fn run_canary_check_once_reindexes_when_counts_drift() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(
            vault_root.join("Inbox/Canary.md"),
            "# Canary\n\nWatcher drift recovery.\n",
        )
        .unwrap();

        let cache = VaultCache::open_in_memory().unwrap();
        let search_index = SearchIndex::open_in_memory().unwrap();
        let watcher_state = WatcherState::new();
        let (event_tx, _) = crate::events::create_event_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let state = Arc::new(RwLock::new(AppState {
            vaults: HashMap::from([(
                "work".to_string(),
                VaultState {
                    cache: Arc::new(cache),
                    search_index: Arc::new(search_index),
                    engine: NativeVaultEngine,
                    root: vault_root.clone(),
                    vault_config: ArcSwap::from_pointee(VaultConfig {
                        name: "work".to_string(),
                        ..Default::default()
                    }),
                    watcher_state: watcher_state.clone(),
                    rebuilding: std::sync::atomic::AtomicBool::new(false),
                    template_engine: Arc::new(notesmith_templates::TemplateEngine::new(
                        vault_root.clone(),
                        None,
                    )),
                },
            )]),
            event_tx,
            event_buffer: Arc::new(crate::events::EventBuffer::new(
                crate::events::EVENT_BUFFER_CAPACITY,
            )),
            global_config_path: vault_root.join(".notesmith-http-test-config.toml"),
            started_at: Utc::now(),
            sse_connection_count: Arc::new(AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
            mcp_services: Default::default(),
            transcripts: Default::default(),
        }));

        run_canary_check_once(&state, "work", &vault_root, &watcher_state)
            .await
            .unwrap();

        let state = state.read().await;
        let vault = state.vaults.get("work").unwrap();
        let count: i64 = vault
            .cache
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM notes WHERE vault_name = ?1",
                ["work"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert!(
            vault
                .search_index
                .search("recovery", 10)
                .unwrap()
                .iter()
                .any(|result| result.path == "Inbox/Canary.md")
        );
        assert_eq!(watcher_state.health(), WatcherHealth::Healthy);
        assert_eq!(watcher_state.message(), None);
    }

    #[test]
    fn vault_config_store_updates_arcswap() {
        use arc_swap::ArcSwap;
        use std::sync::Arc;

        let config = VaultConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        let swappable = ArcSwap::from_pointee(config);
        assert_eq!(swappable.load().name, "test");

        let new_config = VaultConfig {
            name: "updated".to_string(),
            ..Default::default()
        };
        swappable.store(Arc::new(new_config));
        assert_eq!(swappable.load().name, "updated");
    }

    #[test]
    fn arcswap_preserves_value_when_not_stored() {
        use arc_swap::ArcSwap;

        let config = VaultConfig {
            name: "original".to_string(),
            ..Default::default()
        };
        let swappable = ArcSwap::from_pointee(config);

        // Simulate error path: don't call store
        assert_eq!(swappable.load().name, "original");
    }
}
