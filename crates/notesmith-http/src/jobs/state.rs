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
    /// The scheduled fire was skipped because its `after` prerequisites were
    /// never met that day (issue #282). No subprocess ran.
    Missed,
    /// An agent job with `allow_writes = true` exited 0 but wrote nothing to
    /// the vault (job success criteria, ADR 0025 amendment 2026-09-04). NOT a
    /// success: it did not deliver, so it does not advance `last_success` and
    /// does not satisfy an `after` prerequisite.
    NoWrites,
}

impl JobRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobRunStatus::Succeeded => "succeeded",
            JobRunStatus::Failed => "failed",
            JobRunStatus::TimedOut => "timed_out",
            JobRunStatus::Missed => "missed",
            JobRunStatus::NoWrites => "no_writes",
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
    /// How many vault writes this run performed, when the run was write-tracked
    /// (an agent job with `allow_writes = true`; job success criteria, ADR 0025
    /// amendment 2026-09-04). `None` for command jobs and read-only agent jobs,
    /// which are not attributed. Diagnostic metadata; the verdict is in
    /// `status` (`writes == 0` on an exit-0 tracked run yields `NoWrites`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writes: Option<u32>,
    /// Which managed sections this run wrote via `update_managed_section`
    /// (sorted, deduped), for write-tracked agent runs (job success criteria,
    /// ADR 0025 amendment 2026-09-04). Diagnostic metadata only — it does NOT
    /// change the verdict (a partial briefing is not failed). `None` for
    /// command jobs, read-only agent jobs, and write-tracked runs that touched
    /// no managed section (they may still have `writes >= 1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections_written: Option<Vec<String>>,
    /// When this job last SUCCEEDED (start time of that run), kept separately
    /// from `last_run` so a later failure does not erase it — same-day `after`
    /// gating (issue #282) needs "has this job succeeded today". Maintained by
    /// [`JobStateStore::record`]; callers never set it. Absent in pre-#282
    /// state files (defaults to `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<DateTime<Utc>>,
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
    ///
    /// `last_success` is derived here, not by callers: a succeeded run stamps
    /// its own `last_run`; any other outcome carries the previous value
    /// forward so the last success survives failures.
    pub fn record(&self, job: &str, mut record: JobRunRecord) -> anyhow::Result<()> {
        let mut state = self.load();
        record.last_success = if record.status == JobRunStatus::Succeeded {
            Some(record.last_run)
        } else {
            state.jobs.get(job).and_then(|prior| prior.last_success)
        };
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
        record_at(status, "2026-08-05T07:30:00Z")
    }

    fn record_at(status: JobRunStatus, at: &str) -> JobRunRecord {
        JobRunRecord {
            last_run: at.parse().expect("valid timestamp"),
            status,
            exit_code: Some(0),
            duration_ms: Some(1234),
            writes: None,
            sections_written: None,
            last_success: None,
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
        assert_eq!(
            serde_json::to_value(JobRunStatus::Missed).unwrap(),
            serde_json::json!("missed")
        );
    }

    #[test]
    fn no_writes_round_trips_through_its_string_form() {
        assert_eq!(JobRunStatus::NoWrites.as_str(), "no_writes");
        let json = serde_json::to_value(JobRunStatus::NoWrites).unwrap();
        assert_eq!(json, serde_json::json!("no_writes"));
        let parsed: JobRunStatus = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, JobRunStatus::NoWrites);
    }

    #[test]
    fn no_writes_does_not_advance_last_success() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);

        // A success stamps last_success.
        store
            .record(
                "briefing",
                record_at(JobRunStatus::Succeeded, "2026-08-05T07:00:00Z"),
            )
            .unwrap();
        let success_at: DateTime<Utc> = "2026-08-05T07:00:00Z".parse().unwrap();
        assert_eq!(store.get("briefing").unwrap().last_success, Some(success_at));

        // A later NoWrites run does NOT advance last_success (it did not
        // deliver) — the earlier success is carried forward, like a failure.
        let mut no_writes = record_at(JobRunStatus::NoWrites, "2026-08-06T07:00:00Z");
        no_writes.writes = Some(0);
        store.record("briefing", no_writes).unwrap();
        let after = store.get("briefing").unwrap();
        assert_eq!(after.status, JobRunStatus::NoWrites);
        assert_eq!(after.writes, Some(0));
        assert_eq!(after.last_success, Some(success_at));

        // And a fresh job whose only run is NoWrites has never succeeded.
        let mut fresh = record_at(JobRunStatus::NoWrites, "2026-08-06T08:00:00Z");
        fresh.writes = Some(0);
        store.record("fresh", fresh).unwrap();
        assert_eq!(store.get("fresh").unwrap().last_success, None);
    }

    #[test]
    fn writes_metadata_round_trips() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);
        let mut record = record(JobRunStatus::Succeeded);
        record.writes = Some(3);
        store.record("briefing", record).unwrap();
        assert_eq!(store.get("briefing").unwrap().writes, Some(3));
    }

    #[test]
    fn sections_written_metadata_round_trips() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);
        let mut record = record(JobRunStatus::Succeeded);
        record.writes = Some(4);
        record.sections_written = Some(vec![
            "briefing/meetings".to_string(),
            "briefing/tasks".to_string(),
        ]);
        store.record("briefing", record).unwrap();
        let loaded = store.get("briefing").unwrap();
        assert_eq!(loaded.writes, Some(4));
        assert_eq!(
            loaded.sections_written,
            Some(vec![
                "briefing/meetings".to_string(),
                "briefing/tasks".to_string()
            ])
        );
    }

    #[test]
    fn last_success_is_stamped_on_success_and_survives_failures() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);

        // A success stamps its own start time.
        store
            .record(
                "sync",
                record_at(JobRunStatus::Succeeded, "2026-08-05T07:00:00Z"),
            )
            .unwrap();
        let success_at: DateTime<Utc> = "2026-08-05T07:00:00Z".parse().unwrap();
        assert_eq!(store.get("sync").unwrap().last_success, Some(success_at));

        // A later failure keeps the earlier success timestamp.
        store
            .record(
                "sync",
                record_at(JobRunStatus::Failed, "2026-08-05T08:00:00Z"),
            )
            .unwrap();
        let after_failure = store.get("sync").unwrap();
        assert_eq!(after_failure.status, JobRunStatus::Failed);
        assert_eq!(after_failure.last_success, Some(success_at));

        // Missed and timed-out runs also carry it forward.
        store
            .record(
                "sync",
                record_at(JobRunStatus::Missed, "2026-08-06T00:00:00Z"),
            )
            .unwrap();
        assert_eq!(store.get("sync").unwrap().last_success, Some(success_at));

        // Callers cannot smuggle in their own last_success on a failure.
        let mut forged = record_at(JobRunStatus::Failed, "2026-08-06T09:00:00Z");
        forged.last_success = Some("2030-01-01T00:00:00Z".parse().unwrap());
        store.record("fresh", forged).unwrap();
        assert_eq!(store.get("fresh").unwrap().last_success, None);
    }

    #[test]
    fn pre_282_state_files_without_last_success_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = store_in(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(
            store.path(),
            r#"{ "jobs": { "sync": {
                "last_run": "2026-08-05T07:30:00Z",
                "status": "succeeded",
                "exit_code": 0
            } } }"#,
        )
        .unwrap();

        let loaded = store.get("sync").unwrap();
        assert_eq!(loaded.status, JobRunStatus::Succeeded);
        assert_eq!(loaded.last_success, None);
    }
}
