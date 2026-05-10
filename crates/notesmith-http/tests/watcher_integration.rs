use std::{collections::HashMap, fs, time::Duration};

use notesmith_config::VaultConfig;
use notesmith_core::VaultEngine;
use notesmith_http::{AppState, SharedAppState, VaultState, watch_vault};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_vault::NativeVaultEngine;
use tokio::sync::RwLock;

#[tokio::test]
async fn watcher_indexes_new_markdown_files() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let vault_root = temp_dir.path().join("vault");
    fs::create_dir_all(&vault_root).unwrap();
    fs::create_dir_all(vault_root.join("Inbox")).unwrap();

    let engine = NativeVaultEngine;
    let notes = engine.scan(&vault_root).unwrap();
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("test-vault", &notes).unwrap();
    let search_index = SearchIndex::open_in_memory().unwrap();
    search_index.reindex("test-vault", &notes).unwrap();

    let (event_tx, _) = notesmith_http::create_event_channel();

    let state: SharedAppState = std::sync::Arc::new(RwLock::new(AppState {
        vaults: HashMap::from([(
            "test-vault".to_string(),
            VaultState {
                cache,
                search_index,
                engine,
                root: vault_root.clone(),
                vault_config: VaultConfig {
                    name: "test-vault".to_string(),
                    inbox: Default::default(),
                    daily: Default::default(),
                    editor: Default::default(),
                    git: Default::default(),
                    hooks: Default::default(),
                    homepage: None,
                },
                template_engine: notesmith_templates::TemplateEngine::new(vault_root.clone(), None),
            },
        )]),
        event_tx,
    }));

    let _watcher = watch_vault(state.clone(), "test-vault".to_string())
        .await
        .unwrap();

    fs::write(
        vault_root.join("Inbox/New Note.md"),
        "# New Note\n\nHello from the watcher.\n",
    )
    .unwrap();

    for _ in 0..20 {
        {
            let state = state.read().await;
            let vault = state.vaults.get("test-vault").unwrap();
            let count: i64 = vault
                .cache
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM notes WHERE path = 'Inbox/New Note.md'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap();
            let search_results = vault.search_index.search("watcher", 10).unwrap();
            let search_hit = search_results
                .iter()
                .any(|result| result.path == "Inbox/New Note.md");
            if count == 1 && search_hit {
                return;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("watcher did not index the new note in both caches within 2 seconds");
}
