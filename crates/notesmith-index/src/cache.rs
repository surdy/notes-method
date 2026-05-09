use std::path::{Path, PathBuf};

use notesmith_core::Note;
use rusqlite::Connection;

use crate::indexer::CacheIndexer;
use crate::schema::create_schema;

pub struct VaultCache {
    conn: Connection,
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
            conn,
            cache_path: cache_path.to_path_buf(),
        })
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        create_schema(&conn)?;

        Ok(Self {
            conn,
            cache_path: PathBuf::from(":memory:"),
        })
    }

    pub fn reindex(&self, vault_name: &str, notes: &[Note]) -> anyhow::Result<()> {
        let indexer = CacheIndexer::new(&self.conn);
        indexer.index_all(vault_name, notes)
    }

    pub fn update_note(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let indexer = CacheIndexer::new(&self.conn);
        indexer.index_note(vault_name, note)
    }

    pub fn remove_note(&self, vault_name: &str, path: &str) -> anyhow::Result<()> {
        let indexer = CacheIndexer::new(&self.conn);
        indexer.remove_note(vault_name, path)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }
}
