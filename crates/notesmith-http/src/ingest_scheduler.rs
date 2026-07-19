//! Daemon-supervised drop-folder ingestion scheduling (ADR 0022 §7).
//!
//! Unlike the embed scheduler, ingestion (bursty, CPU/IO-heavy extraction —
//! PDF/EPUB today, Whisper later) must **never run inside the interactive
//! daemon** (ADR 0022 §7). So the daemon does not run [`notesmith_ingest`]
//! in-process: instead it supervises one long-lived task per vault that, on an
//! interval, shells out to the colocated `notesmith ingest` CLI worker as a
//! **subprocess**, keeping heavy work out of the daemon process. The supervisor
//! reconciles its worker set against the live vault map so vaults added/removed
//! at runtime gain/lose a scheduler without a restart (mirrors #258).
//!
//! Each pass is gated per vault by the `vault.toml` `[ingest] enabled` flag,
//! re-read fresh every tick so runtime toggling takes effect within one
//! interval. A pass that fails is logged and the loop continues (supervision).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::server::SharedAppState;

/// Default interval between ingest passes. Overridable via
/// `NOTESMITH_INGEST_INTERVAL_SECS` (mostly for tests / tuning).
const DEFAULT_INGEST_INTERVAL_SECS: u64 = 300;
/// Small delay before the first pass so startup isn't contended.
const INITIAL_DELAY_SECS: u64 = 15;
/// How often the supervisor reconciles its worker set against the live vault
/// map. Overridable via `NOTESMITH_INGEST_SUPERVISE_SECS` for tests.
const DEFAULT_SUPERVISE_SECS: u64 = 15;

/// A running set of per-vault ingest scheduler tasks, supervised for the process
/// lifetime. Dropping this aborts the supervisor (individual workers are
/// detached tokio tasks reaped by the supervisor while it runs).
pub struct IngestSchedulers {
    _supervisor: JoinHandle<()>,
}

fn ingest_interval() -> Duration {
    let secs = std::env::var("NOTESMITH_INGEST_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_INGEST_INTERVAL_SECS);
    Duration::from_secs(secs)
}

fn supervise_interval() -> Duration {
    let secs = std::env::var("NOTESMITH_INGEST_SUPERVISE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_SUPERVISE_SECS);
    Duration::from_secs(secs)
}

/// Whether this vault currently has drop-folder ingestion enabled via its
/// `vault.toml` `[ingest] enabled` flag (ADR 0022). Read fresh from disk on
/// every tick so toggling at runtime takes effect within one interval without a
/// daemon restart. Defaults to `false` (no ingest work) when the config can't be
/// loaded — a per-vault error must never enable ingestion by accident or abort
/// the scheduler (resilience policy, ADR 0009).
fn vault_ingest_enabled(root: &Path) -> bool {
    match notesmith_config::VaultConfig::load_from_vault(root) {
        Ok(config) => config.ingest.enabled,
        Err(error) => {
            tracing::warn!(
                vault = %root.display(),
                reason = %error,
                "could not load vault config; skipping ingest pass"
            );
            false
        }
    }
}

/// Resolve the `notesmith` binary the daemon should invoke for ingest passes.
/// The daemon is the same binary as the CLI (it was launched from it), so its
/// own path carries the `ingest` subcommand. Falls back to the bare name so a
/// `PATH` lookup can still succeed if the exe path is unavailable.
fn resolve_ingest_binary() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("notesmith"))
}

/// Build the argument vector for one vault's ingest subprocess. Pure and
/// unit-testable: `notesmith <args>` runs a single incremental pass for one
/// vault and emits a JSON report the daemon logs.
fn ingest_command_args(vault_name: &str) -> Vec<String> {
    vec![
        "ingest".to_string(),
        "--vault".to_string(),
        vault_name.to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}

/// Run one incremental ingest pass for a vault by spawning the colocated
/// `notesmith ingest` CLI as a subprocess (ADR 0022 §7 keeps heavy extraction
/// out of the daemon process). Returns the parsed JSON report on success. A
/// non-zero exit or unparseable output is surfaced as an error the caller logs.
async fn run_ingest_pass(binary: &Path, vault_name: &str) -> anyhow::Result<serde_json::Value> {
    let output = tokio::process::Command::new(binary)
        .args(ingest_command_args(vault_name))
        // Ingestion is a purely local worker; never let an inherited
        // NOTESMITH_URL route the pass at a remote daemon.
        .env_remove("NOTESMITH_URL")
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "ingest subprocess exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|error| anyhow::anyhow!("could not parse ingest report: {error}"))?;
    Ok(report)
}

/// Summarise a JSON ingest report (a one-element array, one object per vault)
/// into `(ingested, reingested, renamed, failed, unsupported)` counts for
/// logging. Best-effort: missing fields count as zero.
fn summarise_report(report: &serde_json::Value) -> (u64, u64, u64, u64, u64) {
    let obj = report.as_array().and_then(|a| a.first()).unwrap_or(report);
    let get = |key: &str| obj.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    (
        get("ingested"),
        get("reingested"),
        get("renamed"),
        get("failed"),
        get("unsupported"),
    )
}

/// Spawn one supervised per-vault ingest scheduler task. The task sleeps a short
/// startup delay, then every `interval` re-reads the vault's `ingest.enabled`
/// flag and, if enabled, shells out to `notesmith ingest` for one pass. A pass
/// that errors is logged and the loop continues (supervision).
fn spawn_ingest_worker(vault_name: String, root: PathBuf, interval: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;
        let mut ticker = tokio::time::interval(interval);
        let binary = resolve_ingest_binary();
        loop {
            ticker.tick().await;

            // Re-read the per-vault flag each tick so runtime toggling takes
            // effect within one interval. A disabled vault does no ingest work.
            if !vault_ingest_enabled(&root) {
                continue;
            }

            match run_ingest_pass(&binary, &vault_name).await {
                Ok(report) => {
                    let (ingested, reingested, renamed, failed, unsupported) =
                        summarise_report(&report);
                    if ingested + reingested + renamed + failed + unsupported > 0 {
                        tracing::info!(
                            vault = %vault_name,
                            ingested,
                            reingested,
                            renamed,
                            failed,
                            unsupported,
                            "ingest pass complete"
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        vault = %vault_name,
                        reason = %error,
                        "ingest pass failed; will retry next interval"
                    );
                }
            }
        }
    })
}

/// Read the current set of live vaults (name → root) from shared state.
async fn desired_vaults(state: &SharedAppState) -> HashMap<String, PathBuf> {
    let state = state.read().await;
    state
        .vaults
        .iter()
        .map(|(name, vs)| (name.clone(), vs.root.clone()))
        .collect()
}

/// Reconcile `workers` against the live vault map exactly once: reap workers for
/// vaults that were removed (or whose task died) and spawn workers for vaults
/// that don't have one yet. Idempotent, so calling it on an interval keeps the
/// worker set in sync with runtime vault add/remove.
async fn supervise_once(
    workers: &mut HashMap<String, JoinHandle<()>>,
    state: &SharedAppState,
    interval: Duration,
) {
    let desired = desired_vaults(state).await;

    workers.retain(|name, handle| {
        if !desired.contains_key(name) {
            handle.abort();
            tracing::info!(vault = %name, "stopped ingest scheduler for removed vault");
            false
        } else if handle.is_finished() {
            tracing::warn!(vault = %name, "ingest scheduler exited; will respawn");
            false
        } else {
            true
        }
    });

    for (name, root) in desired {
        workers.entry(name.clone()).or_insert_with(|| {
            tracing::info!(vault = %name, "starting ingest scheduler");
            spawn_ingest_worker(name.clone(), root, interval)
        });
    }
}

/// Spawn the ingest supervisor: a single task that owns the per-vault scheduler
/// registry and reconciles it against the live vault map on an interval, so
/// vaults created after daemon startup gain a scheduler (and removed vaults lose
/// theirs) without a restart.
pub async fn start_ingest_workers(state: SharedAppState) -> IngestSchedulers {
    let interval = ingest_interval();
    let supervise = supervise_interval();

    let supervisor = tokio::spawn(async move {
        let mut workers: HashMap<String, JoinHandle<()>> = HashMap::new();
        let mut ticker = tokio::time::interval(supervise);
        loop {
            ticker.tick().await;
            supervise_once(&mut workers, &state, interval).await;
        }
    });

    IngestSchedulers {
        _supervisor: supervisor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_vault_config(root: &Path, body: &str) {
        let dir = root.join(".notesmith");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vault.toml"), body).unwrap();
    }

    #[test]
    fn ingest_command_args_targets_one_vault_as_json() {
        assert_eq!(
            ingest_command_args("work"),
            vec!["ingest", "--vault", "work", "--format", "json"]
        );
    }

    #[test]
    fn summarise_report_sums_from_array_payload() {
        let report = serde_json::json!([{
            "vault": "work",
            "ingested": 2,
            "unchanged": 5,
            "reingested": 1,
            "renamed": 0,
            "failed": 1,
            "unsupported": 3,
            "orphaned": []
        }]);
        assert_eq!(summarise_report(&report), (2, 1, 0, 1, 3));
    }

    #[test]
    fn summarise_report_defaults_missing_fields_to_zero() {
        let report = serde_json::json!([{ "vault": "work" }]);
        assert_eq!(summarise_report(&report), (0, 0, 0, 0, 0));
    }

    #[test]
    fn interval_respects_env_override() {
        // SAFETY: single-threaded test.
        unsafe {
            std::env::set_var("NOTESMITH_INGEST_INTERVAL_SECS", "42");
        }
        assert_eq!(ingest_interval(), Duration::from_secs(42));
        unsafe {
            std::env::remove_var("NOTESMITH_INGEST_INTERVAL_SECS");
        }
    }

    #[test]
    fn supervise_interval_respects_env_override() {
        // SAFETY: single-threaded test.
        unsafe {
            std::env::set_var("NOTESMITH_INGEST_SUPERVISE_SECS", "7");
        }
        assert_eq!(supervise_interval(), Duration::from_secs(7));
        unsafe {
            std::env::remove_var("NOTESMITH_INGEST_SUPERVISE_SECS");
        }
    }

    #[test]
    fn vault_ingest_enabled_true_when_flag_set() {
        let vault = TempDir::new().unwrap();
        write_vault_config(
            vault.path(),
            "name = \"enabled\"\n\n[ingest]\nenabled = true\n",
        );
        assert!(vault_ingest_enabled(vault.path()));
    }

    #[test]
    fn vault_ingest_enabled_false_when_flag_absent() {
        let vault = TempDir::new().unwrap();
        write_vault_config(vault.path(), "name = \"no-ingest\"\n");
        assert!(!vault_ingest_enabled(vault.path()));
    }

    #[test]
    fn vault_ingest_enabled_false_when_flag_disabled() {
        let vault = TempDir::new().unwrap();
        write_vault_config(
            vault.path(),
            "name = \"disabled\"\n\n[ingest]\nenabled = false\n",
        );
        assert!(!vault_ingest_enabled(vault.path()));
    }

    #[test]
    fn vault_ingest_enabled_false_when_config_missing() {
        // No vault.toml on disk: must default to disabled, never panic.
        let vault = TempDir::new().unwrap();
        assert!(!vault_ingest_enabled(vault.path()));
    }

    #[tokio::test]
    async fn supervise_once_spawns_and_reaps_runtime_vaults() {
        use crate::server::{build_app_state, create_vault_state};
        use notesmith_config::GlobalConfig;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let state = Arc::new(RwLock::new(
            build_app_state(&GlobalConfig::default()).unwrap(),
        ));
        let mut workers: HashMap<String, JoinHandle<()>> = HashMap::new();
        // A long interval keeps spawned workers parked on the startup delay so
        // they never actually exec a subprocess during the test.
        let interval = Duration::from_secs(3600);

        supervise_once(&mut workers, &state, interval).await;
        assert!(workers.is_empty(), "no vaults => no workers");

        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("a.md"), "# A\n\ncontent").unwrap();
        // Unique per scheduler test: create_vault_state resolves the search
        // index from the global cache dir by vault name, so twin tests
        // sharing a name collide on the Tantivy writer lock.
        let vault_name = "ingest-sched-vault";
        {
            let vs = create_vault_state(vault_name, vault.path()).unwrap();
            state
                .write()
                .await
                .vaults
                .insert(vault_name.to_string(), vs);
        }

        supervise_once(&mut workers, &state, interval).await;
        assert!(
            workers.contains_key(vault_name),
            "runtime-added vault gains a scheduler without a restart"
        );

        supervise_once(&mut workers, &state, interval).await;
        assert_eq!(workers.len(), 1, "idempotent: no double-spawn");
        assert!(!workers[vault_name].is_finished());

        state.write().await.vaults.remove(vault_name);
        supervise_once(&mut workers, &state, interval).await;
        assert!(workers.is_empty(), "removed vault loses its scheduler");
    }
}
