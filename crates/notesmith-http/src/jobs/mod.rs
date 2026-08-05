//! Generic per-vault job runner (ADR 0025 Decision 2, issue #280).
//!
//! `[[jobs]]` entries in `vault.toml` declare scheduled work; the daemon runs
//! one supervised runner task per vault that executes `command`-kind jobs on
//! `every`/`at` schedules with catch-up-on-wake, emitting `job.*` events.
//! Agent-kind jobs and same-day `after` ordering are reserved for #282.
//!
//! Follows the `ingest_scheduler` supervisor pattern: a single reconciliation
//! task keeps one runner per live vault (vaults added/removed at runtime gain/
//! lose a runner without a restart; dead runners are respawned), and every
//! runner tick re-reads the vault's config through the `ArcSwap` in
//! `VaultState`, so toggling `enabled` (or editing schedules) takes effect
//! within one tick, no restart needed. A failing, timing-out, or unlaunchable
//! job logs WARN, emits `job.failed`, and never wedges the loop (ADR 0009).

pub mod run;
pub mod schedule;
pub mod state;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use notesmith_config::JobConfig;
use tokio::task::JoinHandle;

use crate::events::{EventType, VaultEvent};
use crate::server::SharedAppState;
use run::{JobEnv, run_command_job};
use schedule::{is_due, validate_command, validate_job};
use state::{JobRunRecord, JobRunStatus, JobStateStore};

/// Default seconds between runner passes. Bounds how late a job fires after
/// its schedule (and how fast `enabled` toggles apply). Overridable via
/// `NOTESMITH_JOBS_TICK_SECS` for tests/tuning.
const DEFAULT_TICK_SECS: u64 = 20;
/// Small delay before a runner's first pass so startup isn't contended.
const INITIAL_DELAY_SECS: u64 = 5;
/// How often the supervisor reconciles runners against the live vault map.
/// Overridable via `NOTESMITH_JOBS_SUPERVISE_SECS`.
const DEFAULT_SUPERVISE_SECS: u64 = 15;
/// Cap on the stderr tail included in failure log lines.
const LOG_STDERR_TAIL: usize = 500;

/// The running set of per-vault job runners, supervised for the process
/// lifetime. Dropping this aborts the supervisor.
pub struct JobRunners {
    _supervisor: JoinHandle<()>,
}

fn tick_interval() -> Duration {
    duration_from_env("NOTESMITH_JOBS_TICK_SECS", DEFAULT_TICK_SECS)
}

fn supervise_interval() -> Duration {
    duration_from_env("NOTESMITH_JOBS_SUPERVISE_SECS", DEFAULT_SUPERVISE_SECS)
}

fn duration_from_env(var: &str, default_secs: u64) -> Duration {
    let secs = std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_secs);
    Duration::from_secs(secs)
}

// ---------------------------------------------------------------------------
// Run registry: which (vault, job) pairs are executing right now. Shared by
// the scheduled runner and the manual REST trigger so a job never runs twice
// concurrently (the trigger returns 409 while a run is in flight).
// ---------------------------------------------------------------------------

fn registry() -> &'static Mutex<HashSet<(String, String)>> {
    static REGISTRY: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Atomically mark a job as running. `false` when it already is.
fn try_begin_run(vault: &str, job: &str) -> bool {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((vault.to_string(), job.to_string()))
}

/// Whether a job is currently executing (scheduled or manual).
pub fn is_running(vault: &str, job: &str) -> bool {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&(vault.to_string(), job.to_string()))
}

/// Releases the run reservation on drop, so no exit path can leak a
/// permanently-"running" job.
struct RunGuard {
    vault: String,
    job: String,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(self.vault.clone(), self.job.clone()));
    }
}

/// Daemon base URL exported to job subprocesses as `NOTESMITH_API_BASE`.
/// Recorded once at startup from the bound address.
fn api_base_cell() -> &'static OnceLock<String> {
    static API_BASE: OnceLock<String> = OnceLock::new();
    &API_BASE
}

fn api_base() -> String {
    api_base_cell()
        .get()
        .cloned()
        .unwrap_or_else(|| default_api_base(&notesmith_config::GlobalConfig::default().daemon.bind))
}

/// Derive a connectable base URL from the daemon bind address: wildcard hosts
/// are rewritten to loopback since jobs run on the daemon host.
fn default_api_base(bind: &str) -> String {
    let bind = match bind.rsplit_once(':') {
        Some(("0.0.0.0" | "::" | "[::]", port)) => format!("127.0.0.1:{port}"),
        _ => bind.to_string(),
    };
    format!("http://{bind}")
}

// ---------------------------------------------------------------------------
// Per-vault runner
// ---------------------------------------------------------------------------

/// Everything a runner pass needs besides shared app state. Constructed from
/// the vault's durable data dir in production; injectable for tests.
#[derive(Debug, Clone)]
struct RunnerCtx {
    vault_name: String,
    store: JobStateStore,
    /// Root under which each job gets its `NOTESMITH_STATE_DIR`
    /// (`<connector_root>/<job>`), created before the run.
    connector_root: PathBuf,
}

impl RunnerCtx {
    fn for_vault(vault_name: &str) -> anyhow::Result<Self> {
        let data_dir = crate::server::vault_data_dir(vault_name)?;
        Ok(Self {
            vault_name: vault_name.to_string(),
            store: JobStateStore::at_path(data_dir.join("jobs-state.json")),
            connector_root: data_dir.join("connector-state"),
        })
    }

    fn state_dir_for(&self, job: &str) -> PathBuf {
        self.connector_root.join(sanitize_component(job))
    }
}

fn sanitize_component(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

/// One scheduling pass over a vault's `[[jobs]]`: re-reads the live config,
/// validates each enabled entry (invalid ones are skipped with a once-per-
/// reason WARN), decides due-ness, and runs due jobs sequentially.
async fn runner_pass(
    state: &SharedAppState,
    ctx: &RunnerCtx,
    runner_started: DateTime<Utc>,
    warned: &mut HashSet<String>,
) {
    // Re-read the vault's config each pass (ArcSwap kept fresh by the config
    // watcher) so `enabled`/schedule edits apply without a restart.
    let jobs: Vec<JobConfig> = {
        let app = state.read().await;
        let Some(vault) = app.vaults.get(&ctx.vault_name) else {
            return;
        };
        vault.vault_config.load().jobs.clone()
    };

    let mut seen = HashSet::new();
    for job in jobs {
        if !seen.insert(job.name.clone()) {
            warn_once(
                warned,
                &ctx.vault_name,
                &job.name,
                "duplicate job name; only the first entry is scheduled",
            );
            continue;
        }
        if !job.enabled {
            continue;
        }
        let validated = match validate_job(&job) {
            Ok(validated) => validated,
            Err(reason) => {
                warn_once(warned, &ctx.vault_name, &job.name, &reason);
                continue;
            }
        };
        let last_run = ctx.store.get(&validated.name).map(|record| record.last_run);
        if !is_due(&validated.schedule, Utc::now(), last_run, runner_started) {
            continue;
        }
        if !try_begin_run(&ctx.vault_name, &validated.name) {
            continue; // a manual run is in flight
        }
        let _guard = RunGuard {
            vault: ctx.vault_name.clone(),
            job: validated.name.clone(),
        };
        execute_job(
            state,
            ctx,
            &validated.name,
            &validated.command,
            validated.timeout,
        )
        .await;
    }
}

/// Log a config problem once per (job, reason) so a bad entry doesn't spam
/// the log every tick; clears naturally when the reason changes.
fn warn_once(warned: &mut HashSet<String>, vault: &str, job: &str, reason: &str) {
    let key = format!("{job}\u{0}{reason}");
    if warned.insert(key) {
        tracing::warn!(vault = %vault, job = %job, reason = %reason, "skipping [[jobs]] entry");
    }
}

/// Execute one job run end to end: `job.started` event, subprocess with the
/// connector env, state record, `job.succeeded`/`job.failed` event. All
/// failure modes log WARN and return normally — the caller's loop survives.
/// Callers must hold the run reservation.
async fn execute_job(
    state: &SharedAppState,
    ctx: &RunnerCtx,
    job_name: &str,
    command: &str,
    timeout: Duration,
) {
    let (root, event_tx, event_buffer) = {
        let app = state.read().await;
        let Some(vault) = app.vaults.get(&ctx.vault_name) else {
            return; // vault removed since scheduling
        };
        (
            vault.root.clone(),
            app.event_tx.clone(),
            app.event_buffer.clone(),
        )
    };

    let started_at = Utc::now();
    crate::events::emit(
        &event_tx,
        &event_buffer,
        VaultEvent::new(&ctx.vault_name, EventType::JobStarted, job_name),
    );

    let state_dir = ctx.state_dir_for(job_name);
    if let Err(error) = std::fs::create_dir_all(&state_dir) {
        tracing::warn!(
            vault = %ctx.vault_name,
            job = %job_name,
            reason = %error,
            "could not create connector state dir; running job anyway"
        );
    }
    let env = JobEnv {
        api_base: api_base(),
        vault_name: ctx.vault_name.clone(),
        state_dir,
    };

    let (status, exit_code, duration, failure) =
        match run_command_job(&root, command, timeout, &env).await {
            Ok(outcome) => {
                let status = if outcome.timed_out {
                    JobRunStatus::TimedOut
                } else if outcome.succeeded() {
                    JobRunStatus::Succeeded
                } else {
                    JobRunStatus::Failed
                };
                let failure = match status {
                    JobRunStatus::Succeeded => None,
                    JobRunStatus::TimedOut => {
                        Some(format!("timed out after {}s", timeout.as_secs()))
                    }
                    // Missed is recorded by the gating path, never by a run.
                    JobRunStatus::Failed | JobRunStatus::Missed => Some(format!(
                        "exited with {:?}: {}",
                        outcome.exit_code,
                        tail(&outcome.stderr, LOG_STDERR_TAIL)
                    )),
                };
                (status, outcome.exit_code, outcome.duration, failure)
            }
            Err(error) => (
                JobRunStatus::Failed,
                None,
                Duration::ZERO,
                Some(error.to_string()),
            ),
        };

    if let Err(error) = ctx.store.record(
        job_name,
        JobRunRecord {
            last_run: started_at,
            status,
            exit_code,
            duration_ms: Some(duration.as_millis().min(u64::MAX as u128) as u64),
            last_success: None, // derived by the store
        },
    ) {
        tracing::warn!(
            vault = %ctx.vault_name,
            job = %job_name,
            reason = %error,
            "could not persist job run state"
        );
    }

    match failure {
        None => {
            tracing::info!(
                vault = %ctx.vault_name,
                job = %job_name,
                duration_ms = duration.as_millis() as u64,
                "job succeeded"
            );
            crate::events::emit(
                &event_tx,
                &event_buffer,
                VaultEvent::new(&ctx.vault_name, EventType::JobSucceeded, job_name),
            );
        }
        Some(reason) => {
            tracing::warn!(
                vault = %ctx.vault_name,
                job = %job_name,
                reason = %reason,
                "job failed"
            );
            crate::events::emit(
                &event_tx,
                &event_buffer,
                VaultEvent::new(&ctx.vault_name, EventType::JobFailed, job_name),
            );
        }
    }
}

fn tail(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let char_count = trimmed.chars().count();
    if char_count <= max_chars {
        return trimmed.to_string();
    }
    let skip = char_count - max_chars;
    format!("…{}", trimmed.chars().skip(skip).collect::<String>())
}

// ---------------------------------------------------------------------------
// Manual trigger (REST `POST /api/v/{vault}/jobs/{name}/run`)
// ---------------------------------------------------------------------------

/// Result of a manual trigger request.
#[derive(Debug, PartialEq, Eq)]
pub enum TriggerOutcome {
    /// The run was started in the background; watch `job.*` events / the
    /// jobs list for the outcome.
    Started,
    /// A run of this job is already in flight (HTTP 409).
    AlreadyRunning,
    UnknownVault,
    UnknownJob,
    /// The job cannot be executed (no `command`, or agent-kind — #282).
    NotRunnable(String),
}

/// Manually trigger one job by name, bypassing its schedule. Deliberately
/// works for jobs with a missing or invalid *schedule* (as long as they have
/// a runnable `command`), which is the workflow for developing connectors.
pub async fn trigger_job(
    state: SharedAppState,
    vault_name: &str,
    job_name: &str,
) -> TriggerOutcome {
    let job: Option<JobConfig> = {
        let app = state.read().await;
        let Some(vault) = app.vaults.get(vault_name) else {
            return TriggerOutcome::UnknownVault;
        };
        vault
            .vault_config
            .load()
            .jobs
            .iter()
            .find(|job| job.name == job_name)
            .cloned()
    };
    let Some(job) = job else {
        return TriggerOutcome::UnknownJob;
    };

    let command = match validate_command(&job) {
        Ok(command) => command,
        Err(reason) => return TriggerOutcome::NotRunnable(reason),
    };
    let timeout = job
        .timeout
        .as_deref()
        .and_then(notesmith_git::timers::parse_duration)
        .unwrap_or(schedule::DEFAULT_JOB_TIMEOUT);

    let ctx = match RunnerCtx::for_vault(vault_name) {
        Ok(ctx) => ctx,
        Err(error) => return TriggerOutcome::NotRunnable(error.to_string()),
    };

    if !try_begin_run(vault_name, job_name) {
        return TriggerOutcome::AlreadyRunning;
    }
    let guard = RunGuard {
        vault: vault_name.to_string(),
        job: job_name.to_string(),
    };

    let state = state.clone();
    let job_name = job_name.to_string();
    tokio::spawn(async move {
        let _guard = guard; // released when the run finishes
        execute_job(&state, &ctx, &job_name, &command, timeout).await;
    });

    TriggerOutcome::Started
}

// ---------------------------------------------------------------------------
// Supervisor (mirrors ingest_scheduler)
// ---------------------------------------------------------------------------

/// Spawn one supervised runner task for a vault: initial delay, then a
/// scheduling pass every `tick`.
fn spawn_job_runner(vault_name: String, state: SharedAppState, tick: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let ctx = match RunnerCtx::for_vault(&vault_name) {
            Ok(ctx) => ctx,
            Err(error) => {
                tracing::warn!(
                    vault = %vault_name,
                    reason = %error,
                    "could not resolve job state dir; job runner disabled for this vault"
                );
                return;
            }
        };
        tokio::time::sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;
        let runner_started = Utc::now();
        let mut warned = HashSet::new();
        let mut ticker = tokio::time::interval(tick);
        loop {
            ticker.tick().await;
            runner_pass(&state, &ctx, runner_started, &mut warned).await;
        }
    })
}

/// Reconcile `runners` against the live vault map exactly once: reap runners
/// for removed vaults (or dead tasks) and spawn runners for new vaults.
async fn supervise_once(
    runners: &mut HashMap<String, JoinHandle<()>>,
    state: &SharedAppState,
    tick: Duration,
) {
    let desired: HashSet<String> = {
        let app = state.read().await;
        app.vaults.keys().cloned().collect()
    };

    runners.retain(|name, handle| {
        if !desired.contains(name) {
            handle.abort();
            tracing::info!(vault = %name, "stopped job runner for removed vault");
            false
        } else if handle.is_finished() {
            tracing::warn!(vault = %name, "job runner exited; will respawn");
            false
        } else {
            true
        }
    });

    for name in desired {
        if !runners.contains_key(&name) {
            tracing::info!(vault = %name, "starting job runner");
            runners.insert(name.clone(), spawn_job_runner(name, state.clone(), tick));
        }
    }
}

/// Start the job-runner supervisor. `bind` is the daemon's bound address,
/// used to derive the `NOTESMITH_API_BASE` exported to job subprocesses.
pub async fn start_job_runners(state: SharedAppState, bind: &str) -> JobRunners {
    let _ = api_base_cell().set(default_api_base(bind));
    let tick = tick_interval();
    let supervise = supervise_interval();

    let supervisor = tokio::spawn(async move {
        let mut runners: HashMap<String, JoinHandle<()>> = HashMap::new();
        let mut ticker = tokio::time::interval(supervise);
        loop {
            ticker.tick().await;
            supervise_once(&mut runners, &state, tick).await;
        }
    });

    JobRunners {
        _supervisor: supervisor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{build_app_state, create_vault_state};
    use notesmith_config::GlobalConfig;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    fn write_executable(path: &Path, content: &str) {
        std::fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).unwrap();
        }
    }

    fn write_vault_config(root: &Path, body: &str) {
        let dir = root.join(".notesmith");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vault.toml"), body).unwrap();
    }

    async fn state_with_vault(vault_name: &str, root: &Path) -> SharedAppState {
        let state = Arc::new(RwLock::new(
            build_app_state(&GlobalConfig::default()).unwrap(),
        ));
        let vs = create_vault_state(vault_name, root).unwrap();
        state
            .write()
            .await
            .vaults
            .insert(vault_name.to_string(), vs);
        state
    }

    fn test_ctx(vault_name: &str, dir: &TempDir) -> RunnerCtx {
        RunnerCtx {
            vault_name: vault_name.to_string(),
            store: JobStateStore::at_path(dir.path().join("jobs-state.json")),
            connector_root: dir.path().join("connector-state"),
        }
    }

    #[tokio::test]
    async fn runner_pass_runs_due_job_and_records_state_and_events() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(
            &vault.path().join("job.sh"),
            "#!/bin/sh\ntouch ran-marker\necho \"$NOTESMITH_VAULT\" > vault-env\n",
        );
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-runner-vault"

[[jobs]]
name = "toucher"
every = "1s"
command = "job.sh"
timeout = "10s"
"#,
        );

        let vault_name = "jobs-runner-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let mut event_rx = state.read().await.event_tx.subscribe();
        let ctx = test_ctx(vault_name, &scratch);
        let mut warned = HashSet::new();

        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;

        assert!(vault.path().join("ran-marker").exists());
        assert_eq!(
            std::fs::read_to_string(vault.path().join("vault-env"))
                .unwrap()
                .trim(),
            vault_name
        );
        // Per-job connector state dir was created.
        assert!(
            scratch
                .path()
                .join("connector-state")
                .join("toucher")
                .is_dir()
        );

        let record = ctx.store.get("toucher").unwrap();
        assert_eq!(record.status, JobRunStatus::Succeeded);
        assert_eq!(record.exit_code, Some(0));

        let started = event_rx.recv().await.unwrap();
        assert_eq!(started.event_type, EventType::JobStarted);
        assert_eq!(started.path, "toucher");
        let finished = event_rx.recv().await.unwrap();
        assert_eq!(finished.event_type, EventType::JobSucceeded);

        // Immediately due again? No: the 1s interval hasn't elapsed.
        std::fs::remove_file(vault.path().join("ran-marker")).unwrap();
        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(!vault.path().join("ran-marker").exists());
    }

    #[tokio::test]
    async fn disabled_and_invalid_jobs_are_skipped_and_failures_do_not_wedge() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("ok.sh"), "#!/bin/sh\ntouch ok-marker\n");
        write_executable(&vault.path().join("boom.sh"), "#!/bin/sh\nexit 7\n");
        write_executable(
            &vault.path().join("off.sh"),
            "#!/bin/sh\ntouch off-marker\n",
        );
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-skip-vault"

[[jobs]]
name = "invalid"
every = "5m"
at = "07:30"
command = "ok.sh"

[[jobs]]
name = "disabled"
enabled = false
every = "1s"
command = "off.sh"

[[jobs]]
name = "failing"
every = "1s"
command = "boom.sh"

[[jobs]]
name = "survivor"
every = "1s"
command = "ok.sh"
"#,
        );

        let vault_name = "jobs-skip-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);
        let mut warned = HashSet::new();

        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;

        // The failing job ran and was recorded, and did not stop the pass.
        assert_eq!(
            ctx.store.get("failing").unwrap().status,
            JobRunStatus::Failed
        );
        assert_eq!(ctx.store.get("failing").unwrap().exit_code, Some(7));
        assert!(vault.path().join("ok-marker").exists(), "survivor ran");
        assert!(
            !vault.path().join("off-marker").exists(),
            "disabled skipped"
        );
        assert!(ctx.store.get("invalid").is_none(), "invalid never ran");
        assert!(ctx.store.get("disabled").is_none());
    }

    #[tokio::test]
    async fn timed_out_job_is_recorded_as_timed_out() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("slow.sh"), "#!/bin/sh\nsleep 30\n");
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-timeout-vault"

[[jobs]]
name = "slowpoke"
every = "1s"
command = "slow.sh"
timeout = "1s"
"#,
        );

        let vault_name = "jobs-timeout-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let mut event_rx = state.read().await.event_tx.subscribe();
        let ctx = test_ctx(vault_name, &scratch);

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        assert_eq!(
            ctx.store.get("slowpoke").unwrap().status,
            JobRunStatus::TimedOut
        );
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobStarted
        );
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobFailed
        );
    }

    #[tokio::test]
    async fn enabled_toggle_applies_between_passes_without_restart() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("job.sh"), "#!/bin/sh\ntouch toggled\n");
        write_vault_config(
            vault.path(),
            "name = \"jobs-toggle-vault\"\n\n[[jobs]]\nname = \"t\"\nenabled = false\nevery = \"1s\"\ncommand = \"job.sh\"\n",
        );

        let vault_name = "jobs-toggle-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);
        let mut warned = HashSet::new();

        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(!vault.path().join("toggled").exists());

        // Flip enabled in the live config (as the config watcher would).
        {
            let app = state.read().await;
            let vs = app.vaults.get(vault_name).unwrap();
            let mut config = vs.vault_config.load().as_ref().clone();
            config.jobs[0].enabled = true;
            vs.vault_config.store(Arc::new(config));
        }

        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(vault.path().join("toggled").exists());
    }

    #[tokio::test]
    async fn at_job_catch_up_runs_once_from_persisted_state() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("job.sh"), "#!/bin/sh\ntouch caught-up\n");
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-catchup-vault"

[[jobs]]
name = "daily"
at = "00:00"
timezone = "UTC"
command = "job.sh"
"#,
        );

        let vault_name = "jobs-catchup-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);

        // Simulate a run recorded two days ago: today's 00:00 fire was missed
        // while the daemon was down, so the first pass catches up.
        ctx.store
            .record(
                "daily",
                JobRunRecord {
                    last_run: Utc::now() - chrono::Duration::days(2),
                    status: JobRunStatus::Succeeded,
                    exit_code: Some(0),
                    duration_ms: Some(1),
                    last_success: None,
                },
            )
            .unwrap();

        let mut warned = HashSet::new();
        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(vault.path().join("caught-up").exists());

        // And only once.
        std::fs::remove_file(vault.path().join("caught-up")).unwrap();
        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(!vault.path().join("caught-up").exists());
    }

    #[tokio::test]
    async fn supervise_once_spawns_and_reaps_runtime_vaults() {
        let state = Arc::new(RwLock::new(
            build_app_state(&GlobalConfig::default()).unwrap(),
        ));
        let mut runners: HashMap<String, JoinHandle<()>> = HashMap::new();
        let tick = Duration::from_secs(3600);

        supervise_once(&mut runners, &state, tick).await;
        assert!(runners.is_empty(), "no vaults => no runners");

        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("a.md"), "# A\n\ncontent").unwrap();
        // Unique name: shared per-name search-index locks across tests.
        let vault_name = "jobs-sched-vault";
        {
            let vs = create_vault_state(vault_name, vault.path()).unwrap();
            state
                .write()
                .await
                .vaults
                .insert(vault_name.to_string(), vs);
        }

        supervise_once(&mut runners, &state, tick).await;
        assert!(runners.contains_key(vault_name));

        supervise_once(&mut runners, &state, tick).await;
        assert_eq!(runners.len(), 1, "idempotent: no double-spawn");

        state.write().await.vaults.remove(vault_name);
        supervise_once(&mut runners, &state, tick).await;
        assert!(runners.is_empty(), "removed vault loses its runner");
    }

    #[test]
    fn default_api_base_rewrites_wildcard_hosts() {
        assert_eq!(
            default_api_base("127.0.0.1:27183"),
            "http://127.0.0.1:27183"
        );
        assert_eq!(default_api_base("0.0.0.0:8080"), "http://127.0.0.1:8080");
        assert_eq!(default_api_base("[::]:8080"), "http://127.0.0.1:8080");
    }

    #[test]
    fn tail_keeps_the_end_of_long_output() {
        assert_eq!(tail("  short  ", 500), "short");
        let long = "x".repeat(600);
        let tailed = tail(&long, 500);
        assert!(tailed.starts_with('…'));
        assert_eq!(tailed.chars().count(), 501);
    }

    #[test]
    fn run_registry_prevents_concurrent_runs() {
        assert!(!is_running("reg-vault", "reg-job"));
        assert!(try_begin_run("reg-vault", "reg-job"));
        assert!(is_running("reg-vault", "reg-job"));
        assert!(!try_begin_run("reg-vault", "reg-job"));
        {
            let _guard = RunGuard {
                vault: "reg-vault".to_string(),
                job: "reg-job".to_string(),
            };
        }
        assert!(!is_running("reg-vault", "reg-job"));
    }
}
