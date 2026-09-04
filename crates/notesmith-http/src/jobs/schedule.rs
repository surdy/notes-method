//! Pure schedule math for the generic job runner (ADR 0025) — and the one
//! timezone-aware "delay until HH:MM" helper the daily scheduler shares.
//!
//! Everything here is deterministic and side-effect free (callers inject
//! `now`), so the every/at/timezone/weekday/catch-up decisions are unit-tested
//! without clocks or sleeping.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Datelike, Local, LocalResult, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use notesmith_config::{JobAgentConfig, JobConfig};
use notesmith_git::timers::parse_duration;

/// Default subprocess budget when a `command` job omits `timeout`.
pub const DEFAULT_JOB_TIMEOUT: Duration = Duration::from_secs(120);

/// Default budget when an `agent` job omits `timeout`. Headless agent turns
/// legitimately take minutes (LLM latency, tool calls), so the connector
/// default would kill healthy runs.
pub const DEFAULT_AGENT_JOB_TIMEOUT: Duration = Duration::from_secs(600);

/// A validated job schedule.
#[derive(Debug, Clone, PartialEq)]
pub enum JobSchedule {
    /// Run every fixed interval.
    Every(Duration),
    /// Run at a local (or `timezone`) time of day, with catch-up-on-wake.
    At {
        time: NaiveTime,
        weekdays_only: bool,
        timezone: Option<Tz>,
    },
}

/// What a validated job executes: exactly one of `command` / `agent`.
#[derive(Debug, Clone, PartialEq)]
pub enum JobAction {
    /// Vault-relative connector executable.
    Command(String),
    /// Headless agent run with a named prompt (issue #282).
    Agent(JobAgentConfig),
}

/// A `[[jobs]]` entry validated for execution by the runner.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedJob {
    pub name: String,
    pub schedule: JobSchedule,
    pub action: JobAction,
    pub timeout: Duration,
    /// Same-day ordering prerequisites (validated against the sibling jobs).
    pub after: Vec<String>,
    /// Declared SELECT predicate evaluated against the vault index after the
    /// run; when present it is authoritative over the layer-A verdict (job
    /// success criteria, ADR 0025 amendment 2026-09-04). Passed through verbatim
    /// — the SELECT-only / read-only guard is enforced at evaluation time.
    pub success_when: Option<String>,
}

/// Validate one `[[jobs]]` entry into a runnable form. `siblings` is the
/// vault's full `[[jobs]]` list (used to resolve `after` references). Errors
/// are surfaced as human-readable strings the runner logs as warnings — a bad
/// entry is skipped, never fatal (ADR 0009).
pub fn validate_job(job: &JobConfig, siblings: &[JobConfig]) -> Result<ValidatedJob, String> {
    if job.name.trim().is_empty() {
        return Err("job is missing a name".to_string());
    }

    let action = validate_action(job)?;
    let after = validate_after(job, siblings)?;

    let schedule = match (job.every.as_deref(), job.at.as_deref()) {
        (Some(_), Some(_)) => {
            return Err("`every` and `at` are mutually exclusive; set exactly one".to_string());
        }
        (None, None) => {
            return Err("job has neither `every` nor `at`; set exactly one".to_string());
        }
        (Some(every), None) => {
            let interval = parse_duration(every)
                .ok_or_else(|| format!("invalid `every` interval {every:?} (use e.g. \"15m\")"))?;
            if interval.is_zero() {
                return Err("`every` interval must be greater than zero".to_string());
            }
            JobSchedule::Every(interval)
        }
        (None, Some(at)) => {
            let time = NaiveTime::parse_from_str(at, "%H:%M")
                .map_err(|_| format!("invalid `at` time {at:?} (use \"HH:MM\")"))?;
            let timezone = match job.timezone.as_deref() {
                Some(name) => Some(
                    name.parse::<Tz>()
                        .map_err(|_| format!("unknown timezone {name:?} (use an IANA name)"))?,
                ),
                None => None,
            };
            JobSchedule::At {
                time,
                weekdays_only: job.weekdays_only,
                timezone,
            }
        }
    };

    let timeout = match job.timeout.as_deref() {
        Some(raw) => parse_duration(raw)
            .ok_or_else(|| format!("invalid `timeout` {raw:?} (use e.g. \"120s\")"))?,
        None => match action {
            JobAction::Command(_) => DEFAULT_JOB_TIMEOUT,
            JobAction::Agent(_) => DEFAULT_AGENT_JOB_TIMEOUT,
        },
    };

    Ok(ValidatedJob {
        name: job.name.clone(),
        schedule,
        action,
        timeout,
        after,
        success_when: job.success_when.clone(),
    })
}

/// Validate just the executable part of a job — the requirement for *manual*
/// triggers, which deliberately work even when the schedule is absent or
/// invalid (useful while developing a connector). A job must declare exactly
/// one of `command` / `agent`.
pub fn validate_action(job: &JobConfig) -> Result<JobAction, String> {
    match (job.command.as_deref(), job.agent.as_ref()) {
        (Some(_), Some(_)) => {
            Err("`command` and `agent` are mutually exclusive; set exactly one".to_string())
        }
        (None, None) => Err("job needs a `command` or an `agent`; set exactly one".to_string()),
        (Some(command), None) => {
            let command = command.trim();
            if command.is_empty() {
                return Err("job is missing a `command`".to_string());
            }
            Ok(JobAction::Command(command.to_string()))
        }
        (None, Some(agent)) => match agent.config() {
            Some(config) if !config.prompt.trim().is_empty() => Ok(JobAction::Agent(config.clone())),
            Some(_) => Err("`agent.prompt` must not be empty".to_string()),
            None => Err(
                "invalid `agent` config: expected agent = { prompt = \"name\", allow_writes = false }"
                    .to_string(),
            ),
        },
    }
}

/// Validate a job's `after` list against its sibling jobs: every name must
/// exist, none may be the job itself, and the `after` graph must be acyclic
/// (a cycle would mean the jobs simply never fire, so it is rejected here
/// where it is cheap to see).
fn validate_after(job: &JobConfig, siblings: &[JobConfig]) -> Result<Vec<String>, String> {
    if job.after.is_empty() {
        return Ok(Vec::new());
    }

    let known: HashSet<&str> = siblings.iter().map(|entry| entry.name.as_str()).collect();
    for name in &job.after {
        if name == &job.name {
            return Err("`after` must not reference the job itself".to_string());
        }
        if !known.contains(name.as_str()) {
            return Err(format!("`after` references unknown job {name:?}"));
        }
    }

    // Walk the after-graph from this job's prerequisites; reaching the job
    // again means a cycle. Bounded by the number of jobs, so always cheap.
    let graph: HashMap<&str, &[String]> = siblings
        .iter()
        .map(|entry| (entry.name.as_str(), entry.after.as_slice()))
        .collect();
    let mut stack: Vec<&str> = job.after.iter().map(String::as_str).collect();
    let mut visited: HashSet<&str> = HashSet::new();
    while let Some(current) = stack.pop() {
        if current == job.name {
            return Err(format!(
                "`after` cycle detected involving {:?}; ordered jobs must not depend on each other",
                job.name
            ));
        }
        if visited.insert(current) {
            if let Some(nexts) = graph.get(current) {
                stack.extend(nexts.iter().map(String::as_str));
            }
        }
    }

    Ok(job.after.clone())
}

/// Whether a job is due, given the current instant, the persisted last run,
/// and the instant this runner started.
///
/// - `Every` jobs are due when the interval has elapsed since the last run
///   (or immediately when no run is recorded).
/// - `At` jobs are due when the most recent scheduled fire instant is newer
///   than the last recorded run — which covers both live firing and catch-up
///   after the daemon slept through the fire time. With no recorded last run
///   (first schedule, or a corrupt state file degraded to empty) only fire
///   times after `runner_started` trigger: unknown history never causes a
///   surprise catch-up run.
pub fn is_due(
    schedule: &JobSchedule,
    now: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
    runner_started: DateTime<Utc>,
) -> bool {
    match schedule {
        JobSchedule::Every(interval) => match last_run {
            None => true,
            Some(last) => {
                now.signed_duration_since(last)
                    >= chrono::Duration::from_std(*interval)
                        .unwrap_or_else(|_| chrono::Duration::seconds(i64::MAX / 1_000))
            }
        },
        JobSchedule::At {
            time,
            weekdays_only,
            timezone,
        } => {
            let fire = match timezone {
                Some(tz) => most_recent_fire(now.with_timezone(tz), *time, *weekdays_only)
                    .map(|f| f.with_timezone(&Utc)),
                None => most_recent_fire(now.with_timezone(&Local), *time, *weekdays_only)
                    .map(|f| f.with_timezone(&Utc)),
            };
            let Some(fire) = fire else {
                return false;
            };
            fire > last_run.unwrap_or(runner_started)
        }
    }
}

/// The most recent scheduled fire instant at or before `now` for a
/// time-of-day schedule, honoring `weekdays_only`. Searches back one week;
/// `None` when no eligible fire time exists in that window (or every
/// candidate falls in a DST gap, which resolves within a day in practice).
pub fn most_recent_fire<Z: TimeZone>(
    now: DateTime<Z>,
    time: NaiveTime,
    weekdays_only: bool,
) -> Option<DateTime<Z>> {
    for days_back in 0..=7 {
        let date = now.date_naive() - chrono::Duration::days(days_back);
        if weekdays_only && matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            continue;
        }
        let Some(candidate) = resolve_local(&now.timezone(), date.and_time(time)) else {
            continue;
        };
        if candidate <= now {
            return Some(candidate);
        }
    }
    None
}

/// The next scheduled fire instant strictly after `now`. Searches forward a
/// week (covers `weekdays_only` weekends and DST gaps).
pub fn next_fire<Z: TimeZone>(
    now: DateTime<Z>,
    time: NaiveTime,
    weekdays_only: bool,
) -> Option<DateTime<Z>> {
    for days_ahead in 0..=7 {
        let date = now.date_naive() + chrono::Duration::days(days_ahead);
        if weekdays_only && matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            continue;
        }
        let Some(candidate) = resolve_local(&now.timezone(), date.and_time(time)) else {
            continue;
        };
        if candidate > now {
            return Some(candidate);
        }
    }
    None
}

/// Resolve a wall-clock datetime in a timezone. Ambiguous times (DST fall
/// back) take the earlier instant; nonexistent times (DST spring forward)
/// resolve to `None` so callers skip to the next day.
fn resolve_local<Z: TimeZone>(tz: &Z, local: chrono::NaiveDateTime) -> Option<DateTime<Z>> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => None,
    }
}

/// Parse an optional IANA timezone name, logging a warning and falling back
/// to local time when the name is unknown.
pub fn resolve_timezone(name: Option<&str>) -> Option<Tz> {
    let name = name?;
    match name.parse::<Tz>() {
        Ok(tz) => Some(tz),
        Err(_) => {
            tracing::warn!(timezone = %name, "unknown timezone; falling back to local time");
            None
        }
    }
}

/// How long to sleep until the next occurrence of `time` in `timezone`
/// (daemon-local time when `timezone` is `None`). The tz-aware core shared by
/// the daily scheduler's `compute_delay_until` and the job runner.
pub fn delay_until_time_of_day(time: NaiveTime, timezone: Option<Tz>) -> Duration {
    let next = match timezone {
        Some(tz) => {
            next_fire(Utc::now().with_timezone(&tz), time, false).map(|dt| dt.with_timezone(&Utc))
        }
        None => next_fire(Local::now(), time, false).map(|dt| dt.with_timezone(&Utc)),
    };
    match next {
        Some(next) => next
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO),
        // Pathological (a whole week of DST gaps cannot happen); retry hourly.
        None => Duration::from_secs(3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn job(config: serde_json::Value) -> JobConfig {
        serde_json::from_value(config).expect("test job config must deserialize")
    }

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, mo, d)
            .and_then(|date| date.and_hms_opt(h, mi, 0))
            .expect("valid test datetime")
            .and_utc()
    }

    fn hhmm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).expect("valid test time")
    }

    fn validate(config: serde_json::Value) -> Result<ValidatedJob, String> {
        let entry = job(config);
        let siblings = vec![entry.clone()];
        validate_job(&entry, &siblings)
    }

    // ---- validation -------------------------------------------------------

    #[test]
    fn validate_every_job() {
        let validated = validate(serde_json::json!({
            "name": "calendar-sync",
            "every": "15m",
            "command": "sync.py",
            "timeout": "60s"
        }))
        .unwrap();
        assert_eq!(
            validated.schedule,
            JobSchedule::Every(Duration::from_secs(900))
        );
        assert_eq!(validated.action, JobAction::Command("sync.py".to_string()));
        assert_eq!(validated.timeout, Duration::from_secs(60));
        assert!(validated.after.is_empty());
    }

    #[test]
    fn validate_at_job_with_timezone_and_weekdays() {
        let validated = validate(serde_json::json!({
            "name": "email-digest",
            "at": "07:30",
            "weekdays_only": true,
            "timezone": "America/Vancouver",
            "command": "digest.py"
        }))
        .unwrap();
        assert_eq!(
            validated.schedule,
            JobSchedule::At {
                time: hhmm(7, 30),
                weekdays_only: true,
                timezone: Some(chrono_tz::America::Vancouver),
            }
        );
        assert_eq!(validated.timeout, DEFAULT_JOB_TIMEOUT);
    }

    #[test]
    fn validate_agent_job_with_after() {
        let briefing = job(serde_json::json!({
            "name": "daily-briefing",
            "at": "07:30",
            "weekdays_only": true,
            "after": ["calendar-sync"],
            "agent": { "prompt": "daily-note", "allow_writes": true }
        }));
        let sync = job(serde_json::json!({
            "name": "calendar-sync", "every": "15m", "command": "sync.py"
        }));
        let validated = validate_job(&briefing, &[sync, briefing.clone()]).unwrap();

        assert_eq!(
            validated.action,
            JobAction::Agent(notesmith_config::JobAgentConfig {
                prompt: "daily-note".to_string(),
                allow_writes: true,
            })
        );
        assert_eq!(validated.after, vec!["calendar-sync".to_string()]);
        // Agent jobs get the larger default budget.
        assert_eq!(validated.timeout, DEFAULT_AGENT_JOB_TIMEOUT);
    }

    #[test]
    fn validate_rejects_both_every_and_at() {
        let error = validate(serde_json::json!({
            "name": "x", "every": "5m", "at": "07:30", "command": "c"
        }))
        .unwrap_err();
        assert!(error.contains("mutually exclusive"), "{error}");
    }

    #[test]
    fn validate_rejects_neither_every_nor_at() {
        let error = validate(serde_json::json!({ "name": "x", "command": "c" })).unwrap_err();
        assert!(error.contains("neither"), "{error}");
    }

    #[test]
    fn validate_rejects_bad_interval_time_timezone_and_timeout() {
        let bad_interval = serde_json::json!({ "name": "x", "every": "soon", "command": "c" });
        assert!(validate(bad_interval).unwrap_err().contains("every"));

        let bad_time = serde_json::json!({ "name": "x", "at": "7:3pm", "command": "c" });
        assert!(validate(bad_time).unwrap_err().contains("at"));

        let bad_tz = serde_json::json!({
            "name": "x", "at": "07:30", "timezone": "Mars/Olympus", "command": "c"
        });
        assert!(validate(bad_tz).unwrap_err().contains("timezone"));

        let bad_timeout = serde_json::json!({
            "name": "x", "every": "5m", "command": "c", "timeout": "forever"
        });
        assert!(validate(bad_timeout).unwrap_err().contains("timeout"));
    }

    #[test]
    fn validate_rejects_missing_name_or_action() {
        let no_name = serde_json::json!({ "every": "5m", "command": "c" });
        assert!(validate(no_name).unwrap_err().contains("name"));

        let no_action = serde_json::json!({ "name": "x", "every": "5m" });
        let error = validate(no_action).unwrap_err();
        assert!(error.contains("`command` or an `agent`"), "{error}");
    }

    #[test]
    fn validate_rejects_both_command_and_agent() {
        let error = validate(serde_json::json!({
            "name": "x",
            "every": "5m",
            "command": "c.sh",
            "agent": { "prompt": "daily-note" }
        }))
        .unwrap_err();
        assert!(
            error.contains("`command` and `agent` are mutually exclusive"),
            "{error}"
        );
    }

    #[test]
    fn validate_rejects_malformed_or_empty_agent_config() {
        let malformed = serde_json::json!({
            "name": "x", "at": "07:30", "agent": "daily-note"
        });
        let error = validate(malformed).unwrap_err();
        assert!(error.contains("invalid `agent` config"), "{error}");

        let empty_prompt = serde_json::json!({
            "name": "x", "at": "07:30", "agent": { "prompt": "  " }
        });
        let error = validate(empty_prompt).unwrap_err();
        assert!(error.contains("`agent.prompt`"), "{error}");
    }

    #[test]
    fn validate_after_rejects_unknown_self_and_cycles() {
        let sync = job(serde_json::json!({
            "name": "calendar-sync", "every": "15m", "command": "sync.py"
        }));

        let unknown = job(serde_json::json!({
            "name": "briefing", "at": "07:30", "command": "b.sh", "after": ["nope"]
        }));
        let error = validate_job(&unknown, &[sync.clone(), unknown.clone()]).unwrap_err();
        assert!(error.contains("unknown job \"nope\""), "{error}");

        let selfish = job(serde_json::json!({
            "name": "briefing", "at": "07:30", "command": "b.sh", "after": ["briefing"]
        }));
        let error = validate_job(&selfish, &[selfish.clone()]).unwrap_err();
        assert!(
            error.contains("must not reference the job itself"),
            "{error}"
        );

        // a -> b -> a: both report the cycle.
        let a = job(serde_json::json!({
            "name": "a", "at": "07:30", "command": "a.sh", "after": ["b"]
        }));
        let b = job(serde_json::json!({
            "name": "b", "at": "07:40", "command": "b.sh", "after": ["a"]
        }));
        let siblings = vec![a.clone(), b.clone()];
        assert!(validate_job(&a, &siblings).unwrap_err().contains("cycle"));
        assert!(validate_job(&b, &siblings).unwrap_err().contains("cycle"));

        // Transitive cycle a -> b -> c -> a.
        let a = job(serde_json::json!({
            "name": "a", "at": "07:30", "command": "a.sh", "after": ["b"]
        }));
        let b = job(serde_json::json!({
            "name": "b", "at": "07:40", "command": "b.sh", "after": ["c"]
        }));
        let c = job(serde_json::json!({
            "name": "c", "at": "07:50", "command": "c.sh", "after": ["a"]
        }));
        let siblings = vec![a.clone(), b, c];
        assert!(validate_job(&a, &siblings).unwrap_err().contains("cycle"));

        // A diamond (shared prerequisite) is NOT a cycle.
        let root = job(serde_json::json!({
            "name": "root", "every": "15m", "command": "r.sh"
        }));
        let left = job(serde_json::json!({
            "name": "left", "at": "07:30", "command": "l.sh", "after": ["root"]
        }));
        let right = job(serde_json::json!({
            "name": "right", "at": "07:30", "command": "r2.sh", "after": ["root"]
        }));
        let top = job(serde_json::json!({
            "name": "top", "at": "08:00", "command": "t.sh", "after": ["left", "right"]
        }));
        let siblings = vec![root, left, right, top.clone()];
        assert!(validate_job(&top, &siblings).is_ok());
    }

    // ---- every jobs -------------------------------------------------------

    #[test]
    fn every_job_is_due_without_history_and_after_interval() {
        let schedule = JobSchedule::Every(Duration::from_secs(900));
        let now = utc(2026, 8, 5, 12, 0);
        let started = utc(2026, 8, 5, 11, 0);

        assert!(is_due(&schedule, now, None, started));
        assert!(is_due(
            &schedule,
            now,
            Some(utc(2026, 8, 5, 11, 45)),
            started
        ));
        assert!(!is_due(
            &schedule,
            now,
            Some(utc(2026, 8, 5, 11, 50)),
            started
        ));
    }

    // ---- at jobs: live firing and catch-up --------------------------------

    #[test]
    fn at_job_fires_once_after_target_time_passes() {
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: false,
            timezone: Some(chrono_tz::UTC),
        };
        let started = utc(2026, 8, 5, 6, 0);

        // Before the fire time: not due.
        assert!(!is_due(&schedule, utc(2026, 8, 5, 7, 0), None, started));
        // Just after: due (fire time passed while the runner was alive).
        assert!(is_due(&schedule, utc(2026, 8, 5, 7, 31), None, started));
        // Once it ran, no longer due until tomorrow.
        assert!(!is_due(
            &schedule,
            utc(2026, 8, 5, 8, 0),
            Some(utc(2026, 8, 5, 7, 31)),
            started
        ));
        // Next day it fires again.
        assert!(is_due(
            &schedule,
            utc(2026, 8, 6, 7, 31),
            Some(utc(2026, 8, 5, 7, 31)),
            started
        ));
    }

    #[test]
    fn at_job_catches_up_after_daemon_slept_through_fire_time() {
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: false,
            timezone: Some(chrono_tz::UTC),
        };
        // Daemon was down over the 07:30 fire; runner starts at 09:00.
        let started = utc(2026, 8, 5, 9, 0);
        let last_run_yesterday = Some(utc(2026, 8, 4, 7, 30));

        assert!(is_due(
            &schedule,
            utc(2026, 8, 5, 9, 0),
            last_run_yesterday,
            started
        ));
    }

    #[test]
    fn at_job_without_history_does_not_catch_up_on_startup() {
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: false,
            timezone: Some(chrono_tz::UTC),
        };
        // Runner starts after today's fire time with no recorded history
        // (fresh job or corrupt state file): no surprise run.
        let started = utc(2026, 8, 5, 9, 0);
        assert!(!is_due(&schedule, utc(2026, 8, 5, 9, 5), None, started));
        // But tomorrow's fire triggers normally.
        assert!(is_due(&schedule, utc(2026, 8, 6, 7, 31), None, started));
    }

    #[test]
    fn at_job_honors_weekdays_only() {
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: true,
            timezone: Some(chrono_tz::UTC),
        };
        let started = utc(2026, 8, 1, 0, 0);
        // 2026-08-08 is a Saturday, 2026-08-09 a Sunday, 2026-08-10 a Monday.
        let ran_friday = Some(utc(2026, 8, 7, 7, 30));

        assert!(!is_due(
            &schedule,
            utc(2026, 8, 8, 8, 0),
            ran_friday,
            started
        ));
        assert!(!is_due(
            &schedule,
            utc(2026, 8, 9, 8, 0),
            ran_friday,
            started
        ));
        assert!(is_due(
            &schedule,
            utc(2026, 8, 10, 7, 31),
            ran_friday,
            started
        ));
    }

    #[test]
    fn weekend_catch_up_lands_on_friday_fire_not_weekend() {
        // Missed Friday while asleep; waking Sunday still owes Friday's run.
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: true,
            timezone: Some(chrono_tz::UTC),
        };
        let started = utc(2026, 8, 9, 10, 0); // Sunday
        let ran_thursday = Some(utc(2026, 8, 6, 7, 30));
        assert!(is_due(
            &schedule,
            utc(2026, 8, 9, 10, 0),
            ran_thursday,
            started
        ));
    }

    #[test]
    fn at_job_honors_timezone() {
        // 07:30 in Vancouver (PDT, UTC-7 in August) is 14:30 UTC.
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: false,
            timezone: Some(chrono_tz::America::Vancouver),
        };
        let started = utc(2026, 8, 5, 0, 0);
        let ran_yesterday = Some(utc(2026, 8, 4, 14, 31));

        // 13:00 UTC = 06:00 Vancouver: not yet.
        assert!(!is_due(
            &schedule,
            utc(2026, 8, 5, 13, 0),
            ran_yesterday,
            started
        ));
        // 14:31 UTC = 07:31 Vancouver: due.
        assert!(is_due(
            &schedule,
            utc(2026, 8, 5, 14, 31),
            ran_yesterday,
            started
        ));
    }

    // ---- fire-time helpers ------------------------------------------------

    #[test]
    fn most_recent_fire_looks_back_across_days() {
        let now = chrono_tz::UTC
            .with_ymd_and_hms(2026, 8, 5, 6, 0, 0)
            .unwrap();
        let fire = most_recent_fire(now, hhmm(7, 30), false).unwrap();
        assert_eq!(
            fire,
            chrono_tz::UTC
                .with_ymd_and_hms(2026, 8, 4, 7, 30, 0)
                .unwrap()
        );
    }

    #[test]
    fn most_recent_fire_skips_weekends_when_weekdays_only() {
        // Sunday 2026-08-09 06:00: most recent weekday fire is Friday 07:30.
        let now = chrono_tz::UTC
            .with_ymd_and_hms(2026, 8, 9, 6, 0, 0)
            .unwrap();
        let fire = most_recent_fire(now, hhmm(7, 30), true).unwrap();
        assert_eq!(
            fire,
            chrono_tz::UTC
                .with_ymd_and_hms(2026, 8, 7, 7, 30, 0)
                .unwrap()
        );
    }

    #[test]
    fn next_fire_rolls_to_tomorrow_and_skips_weekends() {
        let now = chrono_tz::UTC
            .with_ymd_and_hms(2026, 8, 7, 8, 0, 0)
            .unwrap(); // Friday
        let fire = next_fire(now, hhmm(7, 30), true).unwrap();
        assert_eq!(
            fire,
            chrono_tz::UTC
                .with_ymd_and_hms(2026, 8, 10, 7, 30, 0)
                .unwrap()
        );
    }

    #[test]
    fn next_fire_skips_nonexistent_dst_times() {
        // 02:30 does not exist on 2026-03-08 in Vancouver (spring forward).
        let tz = chrono_tz::America::Vancouver;
        let now = tz.with_ymd_and_hms(2026, 3, 8, 0, 0, 0).unwrap();
        let fire = next_fire(now, hhmm(2, 30), false).unwrap();
        assert_eq!(
            fire.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 9).unwrap()
        );
    }

    #[test]
    fn resolve_timezone_falls_back_to_local_on_unknown_names() {
        assert_eq!(resolve_timezone(None), None);
        assert_eq!(resolve_timezone(Some("Mars/Olympus")), None);
        assert_eq!(
            resolve_timezone(Some("America/Vancouver")),
            Some(chrono_tz::America::Vancouver)
        );
    }

    #[test]
    fn delay_until_time_of_day_is_within_a_day() {
        let delay = delay_until_time_of_day(hhmm(23, 59), None);
        assert!(delay <= Duration::from_secs(86_400 + 3600));

        let delay = delay_until_time_of_day(hhmm(12, 0), Some(chrono_tz::Asia::Tokyo));
        assert!(delay > Duration::ZERO);
        assert!(delay <= Duration::from_secs(86_400 + 3600));
    }

    #[test]
    fn delay_until_time_of_day_honors_timezone() {
        // The same wall-clock target in two zones 12h apart must produce
        // delays that differ by ~12h (mod 24h).
        let tokyo = delay_until_time_of_day(hhmm(12, 0), Some(chrono_tz::Asia::Tokyo));
        let utc = delay_until_time_of_day(hhmm(12, 0), Some(chrono_tz::UTC));
        let diff_secs = (tokyo.as_secs() as i64 - utc.as_secs() as i64).rem_euclid(86_400);
        // Tokyo is UTC+9: its noon comes 9h earlier than UTC noon.
        assert!((diff_secs - 15 * 3600).abs() <= 2, "diff was {diff_secs}s");
    }
}
