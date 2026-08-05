//! Pure schedule math for the generic job runner (ADR 0025) — and the one
//! timezone-aware "delay until HH:MM" helper the daily scheduler shares.
//!
//! Everything here is deterministic and side-effect free (callers inject
//! `now`), so the every/at/timezone/weekday/catch-up decisions are unit-tested
//! without clocks or sleeping.

use std::time::Duration;

use chrono::{DateTime, Datelike, Local, LocalResult, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use notesmith_config::JobConfig;
use notesmith_git::timers::parse_duration;

/// Default subprocess budget when a job omits `timeout`.
pub const DEFAULT_JOB_TIMEOUT: Duration = Duration::from_secs(120);

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

/// A `[[jobs]]` entry validated for execution by the runner.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedJob {
    pub name: String,
    pub schedule: JobSchedule,
    pub command: String,
    pub timeout: Duration,
}

/// Validate one `[[jobs]]` entry into a runnable form. Errors are surfaced as
/// human-readable strings the runner logs as warnings — a bad entry is skipped,
/// never fatal (ADR 0009).
pub fn validate_job(job: &JobConfig) -> Result<ValidatedJob, String> {
    if job.name.trim().is_empty() {
        return Err("job is missing a name".to_string());
    }

    let command = validate_command(job)?;

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
        None => DEFAULT_JOB_TIMEOUT,
    };

    Ok(ValidatedJob {
        name: job.name.clone(),
        schedule,
        command,
        timeout,
    })
}

/// Validate just the executable part of a job — the requirement for *manual*
/// triggers, which deliberately work even when the schedule is absent or
/// invalid (useful while developing a connector).
pub fn validate_command(job: &JobConfig) -> Result<String, String> {
    if job.agent.is_some() && job.command.is_none() {
        return Err("agent-kind jobs are not supported yet (#282)".to_string());
    }
    match job.command.as_deref().map(str::trim) {
        Some(command) if !command.is_empty() => Ok(command.to_string()),
        _ => Err("job is missing a `command`".to_string()),
    }
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

    // ---- validation -------------------------------------------------------

    #[test]
    fn validate_every_job() {
        let validated = validate_job(&job(serde_json::json!({
            "name": "calendar-sync",
            "every": "15m",
            "command": "sync.py",
            "timeout": "60s"
        })))
        .unwrap();
        assert_eq!(
            validated.schedule,
            JobSchedule::Every(Duration::from_secs(900))
        );
        assert_eq!(validated.command, "sync.py");
        assert_eq!(validated.timeout, Duration::from_secs(60));
    }

    #[test]
    fn validate_at_job_with_timezone_and_weekdays() {
        let validated = validate_job(&job(serde_json::json!({
            "name": "email-digest",
            "at": "07:30",
            "weekdays_only": true,
            "timezone": "America/Vancouver",
            "command": "digest.py"
        })))
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
    fn validate_rejects_both_every_and_at() {
        let error = validate_job(&job(serde_json::json!({
            "name": "x", "every": "5m", "at": "07:30", "command": "c"
        })))
        .unwrap_err();
        assert!(error.contains("mutually exclusive"), "{error}");
    }

    #[test]
    fn validate_rejects_neither_every_nor_at() {
        let error =
            validate_job(&job(serde_json::json!({ "name": "x", "command": "c" }))).unwrap_err();
        assert!(error.contains("neither"), "{error}");
    }

    #[test]
    fn validate_rejects_bad_interval_time_timezone_and_timeout() {
        let bad_interval = job(serde_json::json!({ "name": "x", "every": "soon", "command": "c" }));
        assert!(validate_job(&bad_interval).unwrap_err().contains("every"));

        let bad_time = job(serde_json::json!({ "name": "x", "at": "7:3pm", "command": "c" }));
        assert!(validate_job(&bad_time).unwrap_err().contains("at"));

        let bad_tz = job(serde_json::json!({
            "name": "x", "at": "07:30", "timezone": "Mars/Olympus", "command": "c"
        }));
        assert!(validate_job(&bad_tz).unwrap_err().contains("timezone"));

        let bad_timeout = job(serde_json::json!({
            "name": "x", "every": "5m", "command": "c", "timeout": "forever"
        }));
        assert!(validate_job(&bad_timeout).unwrap_err().contains("timeout"));
    }

    #[test]
    fn validate_rejects_missing_name_or_command() {
        let no_name = job(serde_json::json!({ "every": "5m", "command": "c" }));
        assert!(validate_job(&no_name).unwrap_err().contains("name"));

        let no_command = job(serde_json::json!({ "name": "x", "every": "5m" }));
        assert!(validate_job(&no_command).unwrap_err().contains("command"));
    }

    #[test]
    fn validate_reports_agent_jobs_as_unsupported() {
        let agent_job = job(serde_json::json!({
            "name": "daily-briefing",
            "at": "07:30",
            "agent": { "prompt": "daily-note" }
        }));
        let error = validate_job(&agent_job).unwrap_err();
        assert!(error.contains("#282"), "{error}");
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
