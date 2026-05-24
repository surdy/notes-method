use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use notesmith_config::PeriodicConfig;
use notesmith_core::Note;
use rusqlite::Connection;

use crate::indexer::CacheIndexer;
use crate::schema::create_schema;
use crate::user_views::load_user_views;

pub struct VaultCache {
    conn: Mutex<Connection>,
    cache_path: PathBuf,
}

impl VaultCache {
    pub fn open(cache_path: &Path) -> anyhow::Result<Self> {
        Self::open_with_initializer(cache_path, |_| {})
    }

    pub fn open_for_vault(cache_path: &Path, vault_root: &Path) -> anyhow::Result<Self> {
        Self::open_with_initializer(cache_path, |conn| {
            load_user_views(conn, vault_root);
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

    fn open_with_initializer(
        cache_path: &Path,
        initialize: impl FnOnce(&Connection),
    ) -> anyhow::Result<Self> {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(cache_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        create_schema(&conn)?;
        initialize(&conn);

        Ok(Self {
            conn: Mutex::new(conn),
            cache_path: cache_path.to_path_buf(),
        })
    }

    pub fn reindex(&self, vault_name: &str, notes: &[Note]) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::new(&conn);
        indexer.index_all(vault_name, notes)
    }

    pub fn reindex_with_periodic(
        &self,
        vault_name: &str,
        notes: &[Note],
        periodic: &PeriodicConfig,
    ) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::with_periodic_config(&conn, periodic);
        indexer.index_all(vault_name, notes)
    }

    pub fn check_integrity(&self) -> anyhow::Result<bool> {
        let conn = self.connection();
        match conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)) {
            Ok(result) => Ok(result == "ok"),
            Err(_) => Ok(false),
        }
    }

    pub fn update_note(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::new(&conn);
        indexer.index_note(vault_name, note)
    }

    pub fn update_note_with_periodic(
        &self,
        vault_name: &str,
        note: &Note,
        periodic: &PeriodicConfig,
    ) -> anyhow::Result<()> {
        let conn = self.connection();
        let indexer = CacheIndexer::with_periodic_config(&conn, periodic);
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

#[cfg(test)]
mod tests {
    use notesmith_core::Note;
    use rusqlite::Connection;

    use super::VaultCache;

    #[test]
    fn check_integrity_reports_healthy_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_path = temp_dir.path().join("cache.sqlite");
        let cache = VaultCache::open(&cache_path).unwrap();

        cache
            .reindex("work", &[sample_note("Inbox/healthy.md", "healthy cache")])
            .unwrap();

        assert!(cache.check_integrity().unwrap());
    }

    #[test]
    fn check_integrity_reports_corrupt_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_path = temp_dir.path().join("cache.sqlite");
        {
            let cache = VaultCache::open(&cache_path).unwrap();
            let notes = (0..32)
                .map(|index| {
                    sample_note(
                        &format!("Inbox/corrupt-{index}.md"),
                        &"before corruption ".repeat(256),
                    )
                })
                .collect::<Vec<_>>();
            cache.reindex("work", &notes).unwrap();
            cache
                .connection()
                .execute_batch("PRAGMA wal_checkpoint(FULL);")
                .unwrap();
        }

        {
            let cache = VaultCache::open(&cache_path).unwrap();
            cache
                .connection()
                .execute_batch(
                    "PRAGMA writable_schema=ON;
                     UPDATE sqlite_schema SET rootpage = -1 WHERE name = 'notes';
                     PRAGMA writable_schema=OFF;",
                )
                .unwrap();
        }
        let cache = VaultCache {
            conn: std::sync::Mutex::new(Connection::open(&cache_path).unwrap()),
            cache_path,
        };

        assert!(!cache.check_integrity().unwrap());
    }

    fn sample_note(path: &str, body: &str) -> Note {
        Note {
            vault: notesmith_core::VaultName::new("work"),
            path: path.into(),
            frontmatter: None,
            raw_frontmatter: None,
            body: body.to_string(),
            tasks: Vec::new(),
            links: Vec::new(),
            inline_fields: Vec::new(),
            blocks: Vec::new(),
            hash: format!("hash-{path}"),
        }
    }
}
