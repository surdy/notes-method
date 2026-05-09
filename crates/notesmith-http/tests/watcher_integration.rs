use std::{collections::HashMap, fs, time::Duration};

use notesmith_core::VaultEngine;
use notesmith_http::{AppState, SharedAppState, VaultState, watch_vault};
use notesmith_index::VaultCache;
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

    let state: SharedAppState = std::sync::Arc::new(RwLock::new(AppState {
        vaults: HashMap::from([(
            "test-vault".to_string(),
            VaultState {
                cache,
                engine,
                root: vault_root.clone(),
            },
        )]),
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
            if count == 1 {
                return;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("watcher did not index the new note within 2 seconds");
}
