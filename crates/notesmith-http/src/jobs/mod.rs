//! Generic per-vault job runner (ADR 0025 Decision 2, issues #280/#282).
//!
//! `[[jobs]]` entries in `vault.toml` declare scheduled work; the daemon runs
//! one supervised runner task per vault that executes `command`-kind jobs
//! (connector subprocesses) and `agent`-kind jobs (headless `notesmith ai
//! prompt` runs via the colocated CLI — never an in-daemon ACP session) on
//! `every`/`at` schedules with catch-up-on-wake, honoring same-day `after`
//! ordering (see `gate`), emitting `job.*` events.
//!
//! Follows the `ingest_scheduler` supervisor pattern: a single reconciliation
//! task keeps one runner per live vault (vaults added/removed at runtime gain/
//! lose a runner without a restart; dead runners are respawned), and every
//! runner tick re-reads the vault's config through the `ArcSwap` in
//! `VaultState`, so toggling `enabled` (or editing schedules) takes effect
//! within one tick, no restart needed. A failing, timing-out, or unlaunchable
//! job logs WARN, emits `job.failed`, and never wedges the loop (ADR 0009).

pub mod gate;
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
use gate::{GateDecision, evaluate_gate};
use run::{JobEnv, run_agent_job, run_command_job};
use schedule::{JobAction, is_due, validate_action, validate_job};
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
    /// The colocated `notesmith` CLI binary agent-kind jobs shell out to —
    /// the daemon's own executable in production (the daemon IS the CLI's
    /// `daemon start`), a stub in tests.
    notesmith_bin: PathBuf,
}

impl RunnerCtx {
    fn for_vault(vault_name: &str) -> anyhow::Result<Self> {
        let data_dir = crate::server::vault_data_dir(vault_name)?;
        Ok(Self {
            vault_name: vault_name.to_string(),
            store: JobStateStore::at_path(data_dir.join("jobs-state.json")),
            connector_root: data_dir.join("connector-state"),
            notesmith_bin: notesmith_bin(),
        })
    }

    fn state_dir_for(&self, job: &str) -> PathBuf {
        self.connector_root.join(sanitize_component(job))
    }
}

/// The `notesmith` binary agent-kind jobs invoke. `NOTESMITH_JOBS_AGENT_BIN`
/// overrides for tests (where `current_exe` is the test harness, which must
/// never be re-invoked as a CLI).
fn notesmith_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("NOTESMITH_JOBS_AGENT_BIN") {
        return PathBuf::from(bin);
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("notesmith"))
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
    for job in &jobs {
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
        let validated = match validate_job(job, &jobs) {
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
        // Same-day `after` ordering (#282): a due job with unmet
        // prerequisites waits (re-checked every tick); a fire whose day
        // ended without them is forfeited — recorded as `missed`, never run
        // late on a following day. Manual triggers bypass this gate.
        if !validated.after.is_empty() {
            match evaluate_gate(
                &validated.after,
                &validated.schedule,
                &ctx.store.all(),
                Utc::now(),
            ) {
                GateDecision::Ready => {}
                GateDecision::Blocked { waiting_on } => {
                    tracing::debug!(
                        vault = %ctx.vault_name,
                        job = %validated.name,
                        waiting_on = ?waiting_on,
                        "job due but blocked on `after` prerequisites"
                    );
                    continue;
                }
                GateDecision::Missed => {
                    record_missed_run(ctx, &validated.name, &validated.after);
                    continue;
                }
            }
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
            &validated.action,
            validated.timeout,
            validated.success_when.as_deref(),
        )
        .await;
    }
}

/// Persist a `missed` run for a fire whose day ended with unmet `after`
/// prerequisites, and WARN (once per fire — recording advances `last_run`, so
/// the same fire is never re-evaluated). Never runs anything.
fn record_missed_run(ctx: &RunnerCtx, job_name: &str, after: &[String]) {
    tracing::warn!(
        vault = %ctx.vault_name,
        job = %job_name,
        after = ?after,
        "missed scheduled run: `after` prerequisites were not met before the day ended"
    );
    if let Err(error) = ctx.store.record(
        job_name,
        JobRunRecord {
            last_run: Utc::now(),
            status: JobRunStatus::Missed,
            exit_code: None,
            duration_ms: None,
            writes: None,
            sections_written: None,
            last_success: None, // derived by the store
        },
    ) {
        tracing::warn!(
            vault = %ctx.vault_name,
            job = %job_name,
            reason = %error,
            "could not persist missed-run state"
        );
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

/// Refine a write-tracked agent run's verdict with what it actually wrote to
/// the vault (job success criteria, ADR 0025 amendment 2026-09-04). Layer A
/// applies ONLY to agent jobs with `allow_writes = true`; for command jobs and
/// read-only agent jobs it is a no-op (returns the base status unchanged and no
/// `writes` metadata), so those behave exactly as before.
///
/// - exit 0 (`Succeeded`) with 0 writes → `NoWrites` (it did not deliver)
/// - exit 0 with >= 1 write → `Succeeded`
/// - a nonzero exit / timeout (`Failed` / `TimedOut`) is unchanged; the writes
///   count is still recorded as diagnostic metadata.
///
/// `writes` is `Some(count)` for a write-tracked run (from the per-run tally —
/// zero when the run never wrote) and `None` for an untracked run.
fn refine_agent_outcome(
    status: JobRunStatus,
    action: &JobAction,
    writes: Option<u32>,
) -> (JobRunStatus, Option<u32>) {
    match action {
        JobAction::Agent(agent) if agent.allow_writes => {
            let count = writes.unwrap_or(0);
            let status = if status == JobRunStatus::Succeeded && count == 0 {
                JobRunStatus::NoWrites
            } else {
                status
            };
            (status, Some(count))
        }
        _ => (status, None),
    }
}

/// Decide a job's final status from its declared `success_when` predicate's
/// outcome (layer C, ADR 0025 amendment 2026-09-04). Pure over the query result
/// so the status logic is unit-testable without a live vault; the SQL execution
/// itself is done by the caller against the vault index.
///
/// - a SQL / execution error (including a non-SELECT statement, which the
///   read-only guard rejects) → `Failed`, reason carrying the error — a broken
///   predicate is a job-config failure, surfaced not swallowed.
/// - a single scalar result (one row, one column) → `Succeeded` when the scalar
///   is truthy, `Failed` (predicate not satisfied) when it is falsy (null,
///   `false`, `0`, `0.0`, or an empty string).
/// - any other non-empty result set (≥1 row) → `Succeeded`.
/// - an empty result set → `Failed` (predicate not satisfied).
fn evaluate_success_when(
    result: Result<notesmith_query::QueryResult, notesmith_query::QueryError>,
) -> (JobRunStatus, Option<String>) {
    const NOT_SATISFIED: &str = "success_when predicate not satisfied";
    match result {
        Err(error) => (
            JobRunStatus::Failed,
            Some(format!("success_when query failed: {error}")),
        ),
        Ok(result) => {
            let satisfied = if result.rows.len() == 1 && result.columns.len() == 1 {
                json_is_truthy(&result.rows[0][0])
            } else {
                result.row_count >= 1
            };
            if satisfied {
                (JobRunStatus::Succeeded, None)
            } else {
                (JobRunStatus::Failed, Some(NOT_SATISFIED.to_string()))
            }
        }
    }
}

/// Truthiness of a single scalar cell for a `success_when` predicate: null,
/// `false`, a zero number, and the empty string are falsy; everything else is
/// truthy.
fn json_is_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        serde_json::Value::String(text) => !text.is_empty(),
        // Arrays/objects cannot come out of the SQL scalar path; treat a present
        // structured value as truthy.
        _ => true,
    }
}

/// Execute one job run end to end: `job.started` event, subprocess with the
/// connector env, state record, then a `job.succeeded` / `job.no_writes` /
/// `job.failed` event keyed off the refined status. All failure modes log WARN
/// and return normally — the caller's loop survives.
/// Callers must hold the run reservation.
async fn execute_job(
    state: &SharedAppState,
    ctx: &RunnerCtx,
    job_name: &str,
    action: &JobAction,
    timeout: Duration,
    success_when: Option<&str>,
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

    // Write-tracked agent runs (allow_writes) get a unique run id: the CLI
    // stamps it on its daemon HTTP vault binding as `X-Notesmith-Run-Id`, and
    // this same daemon process tallies the run's writes under it (job success
    // criteria, ADR 0025 amendment 2026-09-04). Command jobs and read-only
    // agent jobs are not attributed — layer A does not apply to them.
    let run_id = match action {
        JobAction::Agent(agent) if agent.allow_writes => Some(uuid::Uuid::new_v4().to_string()),
        _ => None,
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
        run_id: run_id.clone(),
    };

    let run_result = match action {
        JobAction::Command(command) => run_command_job(&root, command, timeout, &env).await,
        JobAction::Agent(agent) => {
            run_agent_job(&ctx.notesmith_bin, &root, agent, timeout, &env).await
        }
    };
    let (base_status, exit_code, duration, base_failure) = match run_result {
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
                JobRunStatus::TimedOut => Some(format!("timed out after {}s", timeout.as_secs())),
                // Missed is recorded by the gating path; NoWrites is derived
                // below, never observed here.
                JobRunStatus::Failed | JobRunStatus::Missed | JobRunStatus::NoWrites => {
                    Some(format!(
                        "exited with {:?}: {}",
                        outcome.exit_code,
                        tail(&outcome.stderr, LOG_STDERR_TAIL)
                    ))
                }
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

    // Layer A: for a write-tracked agent run, read the writes this run's id
    // accumulated in this daemon (removing the entry), then refine the verdict —
    // an exit-0 run that wrote nothing becomes `NoWrites`. The tally is read
    // only AFTER the subprocess has fully exited, so every write it made is in.
    let run_writes = run_id
        .as_deref()
        .map(|id| notesmith_mcp::take_run_writes(id).unwrap_or_default());
    let writes_count = run_writes.as_ref().map(|writes| writes.count);
    let (status, writes) = refine_agent_outcome(base_status, action, writes_count);
    // Per-section attribution (diagnostic only; does not change the verdict).
    // `Some(non-empty)` only for a write-tracked run that touched at least one
    // managed section; a run that wrote nothing section-shaped is left `None` so
    // the record stays clean.
    let sections_written = run_writes
        .map(|writes| writes.sections)
        .filter(|sections| !sections.is_empty());
    let failure = match status {
        JobRunStatus::NoWrites => {
            Some("agent run exited 0 but wrote nothing to the vault".to_string())
        }
        _ => base_failure,
    };

    // Layer C: a declared `success_when` predicate is authoritative — when set,
    // it OVERRIDES layer A's verdict (including `NoWrites` and `Succeeded`),
    // running SELECT-only against the vault index which already reflects the
    // run's writes (Ops reindexes the shared cache synchronously, and this runs
    // after the subprocess exited). The write/section metadata is still kept.
    let (status, failure) = match success_when {
        Some(sql) => {
            let result = {
                let app = state.read().await;
                app.vaults
                    .get(&ctx.vault_name)
                    .map(|vault| notesmith_query::execute_sql(&vault.cache, sql))
            };
            match result {
                Some(result) => evaluate_success_when(result),
                None => (
                    JobRunStatus::Failed,
                    Some("vault removed before success_when could be evaluated".to_string()),
                ),
            }
        }
        None => (status, failure),
    };

    if let Err(error) = ctx.store.record(
        job_name,
        JobRunRecord {
            last_run: started_at,
            status,
            exit_code,
            duration_ms: Some(duration.as_millis().min(u64::MAX as u128) as u64),
            writes,
            sections_written,
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

    // The emitted event keys off the refined status, not just the failure
    // reason: `NoWrites` carries a reason (for the record) but is deliberately
    // NOT `job.failed` — a quiet run must not trip failure alerting, while
    // still being visible as its own signal (ADR 0025, 2026-09-04 amendment).
    match status {
        JobRunStatus::NoWrites => {
            tracing::warn!(
                vault = %ctx.vault_name,
                job = %job_name,
                "job exited 0 but wrote nothing to the vault"
            );
            crate::events::emit(
                &event_tx,
                &event_buffer,
                VaultEvent::new(&ctx.vault_name, EventType::JobNoWrites, job_name),
            );
        }
        _ if failure.is_none() => {
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
        _ => {
            tracing::warn!(
                vault = %ctx.vault_name,
                job = %job_name,
                reason = failure.as_deref().unwrap_or("unknown"),
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
    /// The job cannot be executed (neither a runnable `command` nor a valid
    /// `agent` config).
    NotRunnable(String),
}

/// Manually trigger one job by name, bypassing its schedule AND its `after`
/// gating — a human asking for a run is the decision. Deliberately works for
/// jobs with a missing or invalid *schedule* (as long as their
/// `command`/`agent` action is runnable), which is the workflow for
/// developing connectors and prompts.
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

    let action = match validate_action(&job) {
        Ok(action) => action,
        Err(reason) => return TriggerOutcome::NotRunnable(reason),
    };
    let timeout = job
        .timeout
        .as_deref()
        .and_then(notesmith_git::timers::parse_duration)
        .unwrap_or(match action {
            JobAction::Command(_) => schedule::DEFAULT_JOB_TIMEOUT,
            JobAction::Agent(_) => schedule::DEFAULT_AGENT_JOB_TIMEOUT,
        });

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
    let success_when = job.success_when.clone();
    tokio::spawn(async move {
        let _guard = guard; // released when the run finishes
        execute_job(
            &state,
            &ctx,
            &job_name,
            &action,
            timeout,
            success_when.as_deref(),
        )
        .await;
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
    use notesmith_query::QueryResult;
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
            // Tests that exercise agent jobs point this at a stub script;
            // it must never be the test harness itself.
            notesmith_bin: dir.path().join("notesmith-stub"),
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
                    writes: None,
                    sections_written: None,
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
    async fn agent_job_runs_through_the_stub_cli_and_records_state() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-agent-vault"

[[jobs]]
name = "daily-briefing"
every = "1s"
agent = { prompt = "daily-note", allow_writes = true }
"#,
        );

        let vault_name = "jobs-agent-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let mut event_rx = state.read().await.event_tx.subscribe();
        let ctx = test_ctx(vault_name, &scratch);
        // The stub stands in for the notesmith CLI: no LLM runs in tests.
        write_executable(
            &ctx.notesmith_bin.clone(),
            "#!/bin/sh\necho \"$@\" > agent-invocation\n",
        );

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        let invocation = std::fs::read_to_string(vault.path().join("agent-invocation")).unwrap();
        assert_eq!(
            invocation.trim(),
            "--vault jobs-agent-vault ai prompt daily-note --allow-writes"
        );
        // The stub exits 0 but makes no MCP writes: layer A records this
        // write-tracked run as `NoWrites` (0 writes), NOT a false `succeeded` —
        // the exact daily-briefing incident this feature fixes.
        let record = ctx.store.get("daily-briefing").unwrap();
        assert_eq!(record.status, JobRunStatus::NoWrites);
        assert_eq!(record.writes, Some(0));
        assert_eq!(record.last_success, None, "NoWrites never stamps success");
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobStarted
        );
        // A no-write run surfaces as the distinct job.no_writes event — visible,
        // but not job.failed, so failure alerting does not fire on a quiet run.
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobNoWrites
        );
    }

    #[tokio::test]
    async fn read_only_agent_job_exit_zero_stays_succeeded() {
        // Layer A does not apply to read-only agent jobs (allow_writes = false):
        // an exit-0 run is `Succeeded` regardless of writes, as before.
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_vault_config(
            vault.path(),
            "name = \"jobs-agent-ro-vault\"\n\n[[jobs]]\nname = \"digest\"\nevery = \"1s\"\nagent = { prompt = \"daily-note\" }\n",
        );

        let vault_name = "jobs-agent-ro-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let mut event_rx = state.read().await.event_tx.subscribe();
        let ctx = test_ctx(vault_name, &scratch);
        write_executable(&ctx.notesmith_bin.clone(), "#!/bin/sh\nexit 0\n");

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        let record = ctx.store.get("digest").unwrap();
        assert_eq!(record.status, JobRunStatus::Succeeded);
        assert_eq!(record.writes, None, "read-only runs are not write-tracked");
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobStarted
        );
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobSucceeded
        );
    }

    #[tokio::test]
    async fn failing_agent_job_records_a_failed_run() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_vault_config(
            vault.path(),
            "name = \"jobs-agent-fail-vault\"\n\n[[jobs]]\nname = \"briefing\"\nevery = \"1s\"\nagent = { prompt = \"daily-note\" }\n",
        );

        let vault_name = "jobs-agent-fail-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);
        write_executable(&ctx.notesmith_bin.clone(), "#!/bin/sh\nexit 9\n");

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        let record = ctx.store.get("briefing").unwrap();
        assert_eq!(record.status, JobRunStatus::Failed);
        assert_eq!(record.exit_code, Some(9));
    }

    #[tokio::test]
    async fn after_gating_blocks_until_the_prereq_succeeds_today() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(
            &vault.path().join("gated.sh"),
            "#!/bin/sh\ntouch gated-ran\n",
        );
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-gating-vault"

[[jobs]]
name = "calendar-sync"
enabled = false
every = "1h"
command = "gated.sh"

[[jobs]]
name = "briefing"
every = "1s"
command = "gated.sh"
after = ["calendar-sync"]
"#,
        );

        let vault_name = "jobs-gating-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);
        let mut warned = HashSet::new();

        // Prereq has never succeeded: due but blocked, nothing recorded.
        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(!vault.path().join("gated-ran").exists());
        assert!(ctx.store.get("briefing").is_none(), "blocked, not missed");

        // Prereq succeeded yesterday: still blocked (same-DAY ordering).
        ctx.store
            .record(
                "calendar-sync",
                JobRunRecord {
                    last_run: Utc::now() - chrono::Duration::days(1),
                    status: JobRunStatus::Succeeded,
                    exit_code: Some(0),
                    duration_ms: Some(1),
                    writes: None,
                    sections_written: None,
                    last_success: None,
                },
            )
            .unwrap();
        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(!vault.path().join("gated-ran").exists());

        // Prereq succeeds today: the gate opens on the next tick.
        ctx.store
            .record(
                "calendar-sync",
                JobRunRecord {
                    last_run: Utc::now(),
                    status: JobRunStatus::Succeeded,
                    exit_code: Some(0),
                    duration_ms: Some(1),
                    writes: None,
                    sections_written: None,
                    last_success: None,
                },
            )
            .unwrap();
        runner_pass(&state, &ctx, Utc::now(), &mut warned).await;
        assert!(vault.path().join("gated-ran").exists());
        assert_eq!(
            ctx.store.get("briefing").unwrap().status,
            JobRunStatus::Succeeded
        );
    }

    #[tokio::test]
    async fn unknown_after_reference_invalidates_the_job() {
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("job.sh"), "#!/bin/sh\ntouch oops\n");
        write_vault_config(
            vault.path(),
            "name = \"jobs-bad-after-vault\"\n\n[[jobs]]\nname = \"briefing\"\nevery = \"1s\"\ncommand = \"job.sh\"\nafter = [\"no-such-job\"]\n",
        );

        let vault_name = "jobs-bad-after-vault";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;
        assert!(!vault.path().join("oops").exists());
        assert!(ctx.store.get("briefing").is_none());
    }

    #[tokio::test]
    async fn manual_trigger_bypasses_after_gating() {
        let vault = TempDir::new().unwrap();
        write_executable(
            &vault.path().join("gated.sh"),
            "#!/bin/sh\ntouch manual-bypass\n",
        );
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-manual-bypass-vault"

[[jobs]]
name = "calendar-sync"
enabled = false
every = "1h"
command = "gated.sh"

[[jobs]]
name = "briefing"
at = "07:30"
command = "gated.sh"
after = ["calendar-sync"]
"#,
        );

        let vault_name = "jobs-manual-bypass-vault";
        let state = state_with_vault(vault_name, vault.path()).await;

        // The prereq has never succeeded, yet a manual trigger runs anyway.
        let outcome = trigger_job(state.clone(), vault_name, "briefing").await;
        assert_eq!(outcome, TriggerOutcome::Started);
        for _ in 0..100 {
            if vault.path().join("manual-bypass").exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(vault.path().join("manual-bypass").exists());
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

    #[tokio::test]
    async fn success_when_overrides_no_writes_to_succeeded() {
        // Layer C precedence: an allow_writes agent that wrote NOTHING would be
        // `NoWrites` under layer A, but a satisfied `success_when` (here keyed on
        // pre-existing vault state) overrides that to `Succeeded`.
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        std::fs::write(
            vault.path().join("seed.md"),
            "---\ntype: note\n---\nseed body\n",
        )
        .unwrap();
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-success-when-nw"

[[jobs]]
name = "briefing"
every = "1s"
agent = { prompt = "daily-note", allow_writes = true }
success_when = "SELECT path FROM v_notes WHERE path = 'seed.md'"
"#,
        );
        let vault_name = "jobs-success-when-nw";
        let state = state_with_vault(vault_name, vault.path()).await;
        let mut event_rx = state.read().await.event_tx.subscribe();
        let ctx = test_ctx(vault_name, &scratch);
        // Stub agent exits 0 but performs no MCP writes → base verdict NoWrites.
        write_executable(&ctx.notesmith_bin.clone(), "#!/bin/sh\nexit 0\n");

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        let record = ctx.store.get("briefing").unwrap();
        assert_eq!(record.status, JobRunStatus::Succeeded);
        // The write metadata is still recorded (the run wrote nothing).
        assert_eq!(record.writes, Some(0));
        assert!(
            record.last_success.is_some(),
            "a success_when success advances last_success"
        );
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobStarted
        );
        assert_eq!(
            event_rx.recv().await.unwrap().event_type,
            EventType::JobSucceeded
        );
    }

    #[tokio::test]
    async fn success_when_overrides_a_succeeded_run_to_failed() {
        // The inverse precedence: a command job exits 0 (would be `Succeeded`)
        // but an unsatisfied `success_when` (empty result) overrides to `Failed`
        // — and this IS a real failure, so it emits `job.failed`.
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("ok.sh"), "#!/bin/sh\nexit 0\n");
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-success-when-fail"

[[jobs]]
name = "check"
every = "1s"
command = "ok.sh"
success_when = "SELECT path FROM v_notes WHERE path = 'never-exists.md'"
"#,
        );
        let vault_name = "jobs-success-when-fail";
        let state = state_with_vault(vault_name, vault.path()).await;
        let mut event_rx = state.read().await.event_tx.subscribe();
        let ctx = test_ctx(vault_name, &scratch);

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        let record = ctx.store.get("check").unwrap();
        assert_eq!(record.status, JobRunStatus::Failed);
        assert_eq!(record.writes, None, "command jobs are not write-tracked");
        assert_eq!(record.last_success, None);
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
    async fn success_when_sql_error_fails_the_run() {
        // A broken predicate (querying a nonexistent table) is a job-config
        // failure, surfaced as `Failed` — not swallowed.
        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("ok.sh"), "#!/bin/sh\nexit 0\n");
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-success-when-sqlerr"

[[jobs]]
name = "check"
every = "1s"
command = "ok.sh"
success_when = "SELECT * FROM no_such_view"
"#,
        );
        let vault_name = "jobs-success-when-sqlerr";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        assert_eq!(ctx.store.get("check").unwrap().status, JobRunStatus::Failed);
    }

    #[tokio::test]
    async fn success_when_observes_a_write_reflected_in_the_shared_cache() {
        // Index freshness: agent writes go through Ops, which reindexes the
        // shared vault cache synchronously, and the predicate runs only after
        // the run — so a write made "during" the run is visible to success_when.
        // The write here goes through the same shared cache the runner reads.
        use notesmith_ops::Ops;

        let vault = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        write_executable(&vault.path().join("ok.sh"), "#!/bin/sh\nexit 0\n");
        write_vault_config(
            vault.path(),
            r#"
name = "jobs-success-when-fresh"

[[jobs]]
name = "check"
every = "1s"
command = "ok.sh"
success_when = "SELECT path FROM v_notes WHERE path = 'Inbox/Fresh.md'"
"#,
        );
        let vault_name = "jobs-success-when-fresh";
        let state = state_with_vault(vault_name, vault.path()).await;
        let ctx = test_ctx(vault_name, &scratch);

        // The note does not exist at config time; write it through Ops (as an
        // MCP write would), reindexing the shared cache the predicate reads.
        {
            let app = state.read().await;
            let vault_state = app.vaults.get(vault_name).unwrap();
            let ops = crate::server::local_ops_for(vault_name, vault_state);
            ops.create_note("Fresh", Some("body"), Some("Inbox"), None)
                .unwrap();
        }

        runner_pass(&state, &ctx, Utc::now(), &mut HashSet::new()).await;

        // The predicate observes the freshly written note → Succeeded.
        assert_eq!(
            ctx.store.get("check").unwrap().status,
            JobRunStatus::Succeeded
        );
    }

    fn agent_action(allow_writes: bool) -> JobAction {
        JobAction::Agent(notesmith_config::JobAgentConfig {
            prompt: "daily-note".to_string(),
            allow_writes,
        })
    }

    #[test]
    fn refine_agent_outcome_flags_a_no_write_writeable_run() {
        // Write-tracked agent (allow_writes), exit 0, 0 writes → NoWrites.
        let write_agent = agent_action(true);
        assert_eq!(
            refine_agent_outcome(JobRunStatus::Succeeded, &write_agent, Some(0)),
            (JobRunStatus::NoWrites, Some(0))
        );
        // Absent tally (never wrote) is treated as zero.
        assert_eq!(
            refine_agent_outcome(JobRunStatus::Succeeded, &write_agent, None),
            (JobRunStatus::NoWrites, Some(0))
        );
        // Exit 0 with >= 1 write stays Succeeded, tally recorded.
        assert_eq!(
            refine_agent_outcome(JobRunStatus::Succeeded, &write_agent, Some(3)),
            (JobRunStatus::Succeeded, Some(3))
        );
        // Nonzero exit / timeout are never rewritten; the tally is still kept.
        assert_eq!(
            refine_agent_outcome(JobRunStatus::Failed, &write_agent, Some(0)),
            (JobRunStatus::Failed, Some(0))
        );
        assert_eq!(
            refine_agent_outcome(JobRunStatus::TimedOut, &write_agent, Some(2)),
            (JobRunStatus::TimedOut, Some(2))
        );
    }

    #[test]
    fn refine_agent_outcome_leaves_untracked_jobs_untouched() {
        // Read-only agent job: layer A does not apply, no writes metadata.
        assert_eq!(
            refine_agent_outcome(JobRunStatus::Succeeded, &agent_action(false), Some(0)),
            (JobRunStatus::Succeeded, None)
        );
        // Command job: unchanged, no writes metadata.
        assert_eq!(
            refine_agent_outcome(
                JobRunStatus::Succeeded,
                &JobAction::Command("sync.sh".to_string()),
                None
            ),
            (JobRunStatus::Succeeded, None)
        );
    }

    fn query_result(columns: &[&str], rows: Vec<Vec<serde_json::Value>>) -> QueryResult {
        let row_count = rows.len();
        QueryResult {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
            row_count,
            truncated: false,
        }
    }

    #[test]
    fn evaluate_success_when_succeeds_on_non_empty_and_truthy_scalar() {
        use serde_json::json;

        // A single truthy scalar (e.g. COUNT(*) = 4) → Succeeded, no reason.
        assert_eq!(
            evaluate_success_when(Ok(query_result(&["c"], vec![vec![json!(4)]]))),
            (JobRunStatus::Succeeded, None)
        );
        // A truthy boolean / non-empty string scalar.
        assert_eq!(
            evaluate_success_when(Ok(query_result(&["ok"], vec![vec![json!(true)]]))).0,
            JobRunStatus::Succeeded
        );
        assert_eq!(
            evaluate_success_when(Ok(query_result(&["p"], vec![vec![json!("daily/x.md")]]))).0,
            JobRunStatus::Succeeded
        );
        // A multi-column, multi-row non-empty result → Succeeded regardless of
        // any individual value's truthiness (existence of rows is the signal).
        assert_eq!(
            evaluate_success_when(Ok(query_result(
                &["path", "n"],
                vec![vec![json!("a.md"), json!(0)]]
            )))
            .0,
            JobRunStatus::Succeeded
        );
    }

    #[test]
    fn evaluate_success_when_fails_on_empty_and_falsy_scalar() {
        use serde_json::json;

        // Empty result set → Failed with the predicate reason.
        let (status, reason) = evaluate_success_when(Ok(query_result(&["c"], vec![])));
        assert_eq!(status, JobRunStatus::Failed);
        assert_eq!(reason.as_deref(), Some("success_when predicate not satisfied"));

        // Falsy scalars: 0, false, null, empty string → Failed.
        for falsy in [json!(0), json!(false), json!(null), json!("")] {
            let (status, reason) =
                evaluate_success_when(Ok(query_result(&["v"], vec![vec![falsy.clone()]])));
            assert_eq!(status, JobRunStatus::Failed, "value {falsy} should be falsy");
            assert_eq!(reason.as_deref(), Some("success_when predicate not satisfied"));
        }
    }

    #[test]
    fn evaluate_success_when_surfaces_sql_errors_as_failures() {
        // A non-SELECT statement is rejected by the read-only guard; the error
        // is carried into the failure reason (a broken predicate is a
        // job-config failure, not swallowed).
        let (status, reason) =
            evaluate_success_when(Err(notesmith_query::QueryError::NotReadOnly));
        assert_eq!(status, JobRunStatus::Failed);
        assert!(
            reason.as_deref().unwrap().contains("Only SELECT"),
            "{reason:?}"
        );

        let (status, reason) = evaluate_success_when(Err(
            notesmith_query::QueryError::ExecutionError("no such table: bogus".to_string()),
        ));
        assert_eq!(status, JobRunStatus::Failed);
        assert!(reason.as_deref().unwrap().contains("no such table"), "{reason:?}");
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
