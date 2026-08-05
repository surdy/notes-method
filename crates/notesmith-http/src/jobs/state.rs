//! Durable per-vault job-run state (issue #280).
//!
//! Last-run timestamps live OUTSIDE the vault (they must not clutter or sync)
//! in the daemon's durable per-vault data dir — alongside `embeddings.db`,
//! not the rebuildable cache dir — because catch-up correctness depends on
//! them surviving restarts and reindexes. A small JSON file keyed by job name
//! is plenty at this scale; a corrupt or unreadable file degrades to "no
//! recorded runs" with a WARN (which the schedule math treats as
//! no-catch-up), never a panic (ADR 0009).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Terminal status of one job run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobRunStatus {
    Succeeded,
    Failed,
    TimedOut,
}

impl JobRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobRunStatus::Succeeded => "succeeded",
            JobRunStatus::Failed => "failed",
            JobRunStatus::TimedOut => "timed_out",
        }
    }
}

/// The persisted outcome of a job's most recent run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRunRecord {
    /// When the run started (UTC). The value catch-up decisions compare
    /// against fire times.
    pub last_run: DateTime<Utc>,
    pub status: JobRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    jobs: BTreeMap<String, JobRunRecord>,
}

/// Load/record interface over one vault's `jobs-state.json`.
#[derive(Debug, Clone)]
pub struct JobStateStore {
    path: PathBuf,
}

impl JobStateStore {
    /// The store for a vault's durable data dir:
    /// `<data_dir>/<vault>/jobs-state.json`.
    pub fn for_vault(vault_name: &str) -> anyhow::Result<Self> {
        Ok(Self {
            path: crate::server::vault_data_dir(vault_name)?.join("jobs-state.json"),
        })
    }

    /// A store at an explicit path (tests).
    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The recorded last run for `job`, if any. Missing or corrupt state
    /// degrades to `None` (no catch-up) with a WARN — never an error.
    pub fn get(&self, job: &str) -> Option<JobRunRecord> {
        self.load().jobs.get(job).cloned()
    }

    /// All recorded runs, keyed by job name.
    pub fn all(&self) -> BTreeMap<String, JobRunRecord> {
        self.load().jobs
    }

    /// Persist the outcome of a run. Errors are returned for the caller to
    /// log; a failed write must not stop the runner.
    pub fn record(&self, job: &str, record: JobRunRecord) -> anyhow::Result<()> {
        let mut state = self.load();
        state.jobs.insert(job.to_string(), record);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    fn load(&self) -> StateFile {
        let raw = match std::fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return StateFile::default();
            }
            Err(error) => {
                tracing::warn!(
                    path = %self.path.display(),
                    reason = %error,
                    "could not read job state file; treating as empty (no catch-up)"
                );
                return StateFile::default();
            }
        };
        match serde_json::from_str(&raw) {
            Ok(state) => state,
            Err(error) => {
                tracing::warn!(
                    path = %self.path.display(),
                    reason = %error,
                    "corrupt job state file; treating as empty (no catch-up)"
                );
                StateFile::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> JobStateStore {
        JobStateStore::at_path(dir.path().join("nested").join("jobs-state.json"))
    }

    fn record(status: JobRunStatus) -> JobRunRecord {
        JobRunRecord {
            last_run: "2026-08-05T07:30:00Z".parse().expect("valid timestamp"),
            status,
            exit_code: Some(0),
            duration_ms: Some(1234),
        }
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);
        assert_eq!(store.get("calendar-sync"), None);
        assert!(store.all().is_empty());
    }

    #[test]
    fn record_round_trips_and_creates_parent_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);

        store
            .record("calendar-sync", record(JobRunStatus::Succeeded))
            .unwrap();
        store
            .record("email-digest", record(JobRunStatus::Failed))
            .unwrap();

        let loaded = store.get("calendar-sync").unwrap();
        assert_eq!(loaded.status, JobRunStatus::Succeeded);
        assert_eq!(loaded.exit_code, Some(0));
        assert_eq!(loaded.duration_ms, Some(1234));
        assert_eq!(store.all().len(), 2);

        // Re-recording overwrites in place.
        store
            .record("calendar-sync", record(JobRunStatus::TimedOut))
            .unwrap();
        assert_eq!(
            store.get("calendar-sync").unwrap().status,
            JobRunStatus::TimedOut
        );
    }

    #[test]
    fn corrupt_file_degrades_to_empty_without_panicking() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "{ not json !!").unwrap();

        assert_eq!(store.get("calendar-sync"), None);

        // Recording over a corrupt file recovers it.
        store
            .record("calendar-sync", record(JobRunStatus::Succeeded))
            .unwrap();
        assert!(store.get("calendar-sync").is_some());
    }

    #[test]
    fn status_serializes_snake_case() {
        let json = serde_json::to_value(JobRunStatus::TimedOut).unwrap();
        assert_eq!(json, serde_json::json!("timed_out"));
        assert_eq!(JobRunStatus::TimedOut.as_str(), "timed_out");
    }
}
