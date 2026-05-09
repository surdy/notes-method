use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use notesmith_core::Note;
use rusqlite::Connection;

use crate::indexer::CacheIndexer;
use crate::schema::create_schema;

pub struct VaultCache {
    conn: Mutex<Connection>,
    cache_path: PathBuf,
}

impl VaultCache {
    pub fn open(cache_path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(cache_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        create_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            cache_path: cache_path.to_path_buf(),
        })
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        create_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            cache_path: PathBuf::from(":memory:"),
        })
    }

    pub fn reindex(&self, vault_name: &str, notes: &[Note]) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::new(&conn);
        indexer.index_all(vault_name, notes)
    }

    pub fn update_note(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(vault_name, note)
    }

    pub fn remove_note(&self, vault_name: &str, path: &str) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::new(&conn);
        indexer.remove_note(vault_name, path)
    }

    pub fn connection(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("vault cache mutex poisoned")
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }
}
