//! Same-day `after` ordering (issue #282, ADR 0025 Decision 2).
//!
//! A job with `after = ["a", "b"]` only runs once every named job has a
//! SUCCESSFUL run *today* (in the gated job's timezone; daemon-local when it
//! has none). Everything here is pure — callers inject `now` and the
//! persisted run records — so the met/unmet/missed decisions are unit-tested
//! without clocks.
//!
//! Semantics:
//! - Prerequisites are checked on every runner tick: a due job whose
//!   prerequisites are not yet met is *blocked* (not run, not recorded) and
//!   runs as soon as they are met that day.
//! - An `at` fire whose day ends without the prerequisites being met is
//!   *missed*: it never runs late on a following day (the runner records a
//!   `missed` run so catch-up does not resurrect it, and warns once).
//! - `every` jobs are never missed — each interval simply stays blocked until
//!   the prerequisites succeed today.
//! - Manual triggers bypass this gate entirely: a human asking for a run is
//!   the decision.

use std::collections::BTreeMap;

use chrono::{DateTime, Local, NaiveDate, Utc};
use chrono_tz::Tz;

use super::schedule::{JobSchedule, most_recent_fire};
use super::state::JobRunRecord;

/// The gate's verdict for a due job, evaluated on one runner tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// All prerequisites succeeded today — run now.
    Ready,
    /// Some prerequisites have not succeeded today — skip this tick and
    /// re-check on the next one.
    Blocked { waiting_on: Vec<String> },
    /// The due fire belongs to a previous day whose prerequisites were never
    /// met — the run is forfeited. Record it as `missed` (and warn once) so
    /// catch-up does not run it on a later day.
    Missed,
}

/// The timezone in which "today" is evaluated for a schedule: an `at` job's
/// configured timezone, daemon-local otherwise.
fn gate_timezone(schedule: &JobSchedule) -> Option<Tz> {
    match schedule {
        JobSchedule::At { timezone, .. } => *timezone,
        JobSchedule::Every(_) => None,
    }
}

/// The calendar date of `instant` in `tz` (daemon-local when `None`).
fn date_in_zone(instant: DateTime<Utc>, tz: Option<Tz>) -> NaiveDate {
    match tz {
        Some(tz) => instant.with_timezone(&tz).date_naive(),
        None => instant.with_timezone(&Local).date_naive(),
    }
}

/// The prerequisites in `after` that do NOT have a successful run today
/// (preserving `after`'s order). Empty means the gate is open.
pub fn unmet_prereqs(
    after: &[String],
    records: &BTreeMap<String, JobRunRecord>,
    tz: Option<Tz>,
    now: DateTime<Utc>,
) -> Vec<String> {
    let today = date_in_zone(now, tz);
    after
        .iter()
        .filter(|name| {
            let succeeded_today = records
                .get(name.as_str())
                .and_then(|record| record.last_success)
                .is_some_and(|success| date_in_zone(success, tz) == today);
            !succeeded_today
        })
        .cloned()
        .collect()
}

/// The prerequisites currently unmet for a job, in its schedule's timezone —
/// the `waiting_on` surfaced by `GET /jobs` and `notesmith job list`
/// regardless of whether the job is due right now.
pub fn waiting_on(
    after: &[String],
    schedule: &JobSchedule,
    records: &BTreeMap<String, JobRunRecord>,
    now: DateTime<Utc>,
) -> Vec<String> {
    if after.is_empty() {
        return Vec::new();
    }
    unmet_prereqs(after, records, gate_timezone(schedule), now)
}

/// Evaluate the `after` gate for a job the scheduler already considers due.
pub fn evaluate_gate(
    after: &[String],
    schedule: &JobSchedule,
    records: &BTreeMap<String, JobRunRecord>,
    now: DateTime<Utc>,
) -> GateDecision {
    if after.is_empty() {
        return GateDecision::Ready;
    }
    let tz = gate_timezone(schedule);
    let waiting_on = unmet_prereqs(after, records, tz, now);
    if waiting_on.is_empty() {
        return GateDecision::Ready;
    }

    match schedule {
        JobSchedule::Every(_) => GateDecision::Blocked { waiting_on },
        JobSchedule::At {
            time,
            weekdays_only,
            timezone,
        } => {
            // The fire this due-ness came from. When it is from a previous
            // day (its day ended without the prerequisites), the run is
            // missed rather than made up late.
            let fire = match timezone {
                Some(tz) => most_recent_fire(now.with_timezone(tz), *time, *weekdays_only)
                    .map(|fire| fire.with_timezone(&Utc)),
                None => most_recent_fire(now.with_timezone(&Local), *time, *weekdays_only)
                    .map(|fire| fire.with_timezone(&Utc)),
            };
            match fire {
                Some(fire) if date_in_zone(fire, tz) < date_in_zone(now, tz) => {
                    GateDecision::Missed
                }
                _ => GateDecision::Blocked { waiting_on },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::JobRunStatus;
    use super::*;
    use chrono::{NaiveDate, NaiveTime};
    use std::time::Duration;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, mo, d)
            .and_then(|date| date.and_hms_opt(h, mi, 0))
            .expect("valid test datetime")
            .and_utc()
    }

    fn hhmm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).expect("valid test time")
    }

    fn record_with_success(last_success: Option<DateTime<Utc>>) -> JobRunRecord {
        JobRunRecord {
            last_run: last_success.unwrap_or_else(|| utc(2026, 8, 1, 0, 0)),
            status: JobRunStatus::Succeeded,
            exit_code: Some(0),
            duration_ms: Some(1),
            last_success,
        }
    }

    fn records(entries: &[(&str, Option<DateTime<Utc>>)]) -> BTreeMap<String, JobRunRecord> {
        entries
            .iter()
            .map(|(name, success)| (name.to_string(), record_with_success(*success)))
            .collect()
    }

    fn at_utc(h: u32, m: u32) -> JobSchedule {
        JobSchedule::At {
            time: hhmm(h, m),
            weekdays_only: false,
            timezone: Some(chrono_tz::UTC),
        }
    }

    fn after(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn no_after_is_always_ready() {
        let decision = evaluate_gate(&[], &at_utc(7, 30), &BTreeMap::new(), utc(2026, 8, 5, 8, 0));
        assert_eq!(decision, GateDecision::Ready);
    }

    #[test]
    fn ready_when_every_prereq_succeeded_today() {
        let now = utc(2026, 8, 5, 8, 0);
        let recs = records(&[
            ("calendar-sync", Some(utc(2026, 8, 5, 6, 0))),
            ("email-sync", Some(utc(2026, 8, 5, 7, 59))),
        ]);
        let decision = evaluate_gate(
            &after(&["calendar-sync", "email-sync"]),
            &at_utc(7, 30),
            &recs,
            now,
        );
        assert_eq!(decision, GateDecision::Ready);
    }

    #[test]
    fn blocked_lists_only_the_unmet_prereqs() {
        let now = utc(2026, 8, 5, 8, 0);
        let recs = records(&[
            ("calendar-sync", Some(utc(2026, 8, 4, 6, 0))), // yesterday
            ("email-sync", Some(utc(2026, 8, 5, 6, 0))),    // today
        ]);
        let decision = evaluate_gate(
            &after(&["calendar-sync", "email-sync", "never-ran"]),
            &at_utc(7, 30),
            &recs,
            now,
        );
        assert_eq!(
            decision,
            GateDecision::Blocked {
                waiting_on: after(&["calendar-sync", "never-ran"]),
            }
        );
    }

    #[test]
    fn a_failed_run_after_a_success_today_still_counts() {
        // calendar-sync succeeded at 06:00 today, then failed at 07:00: the
        // recorded last_success survives, so the gate is open.
        let now = utc(2026, 8, 5, 8, 0);
        let mut recs = records(&[("calendar-sync", Some(utc(2026, 8, 5, 6, 0)))]);
        if let Some(record) = recs.get_mut("calendar-sync") {
            record.status = JobRunStatus::Failed;
            record.last_run = utc(2026, 8, 5, 7, 0);
        }
        let decision = evaluate_gate(&after(&["calendar-sync"]), &at_utc(7, 30), &recs, now);
        assert_eq!(decision, GateDecision::Ready);
    }

    #[test]
    fn stale_fire_from_a_previous_day_is_missed() {
        // Fire was yesterday 07:30; it is now 06:00 the next day and the
        // prereq still has no success today: the run is forfeited.
        let now = utc(2026, 8, 6, 6, 0);
        let recs = records(&[("calendar-sync", Some(utc(2026, 8, 4, 6, 0)))]);
        let decision = evaluate_gate(&after(&["calendar-sync"]), &at_utc(7, 30), &recs, now);
        assert_eq!(decision, GateDecision::Missed);
    }

    #[test]
    fn same_day_fire_with_unmet_prereqs_blocks_not_misses() {
        let now = utc(2026, 8, 5, 8, 0); // fire was 07:30 today
        let decision = evaluate_gate(
            &after(&["calendar-sync"]),
            &at_utc(7, 30),
            &BTreeMap::new(),
            now,
        );
        assert!(matches!(decision, GateDecision::Blocked { .. }));
    }

    #[test]
    fn every_jobs_block_but_never_miss() {
        let schedule = JobSchedule::Every(Duration::from_secs(900));
        let decision = evaluate_gate(
            &after(&["calendar-sync"]),
            &schedule,
            &BTreeMap::new(),
            utc(2026, 8, 6, 6, 0),
        );
        assert_eq!(
            decision,
            GateDecision::Blocked {
                waiting_on: after(&["calendar-sync"]),
            }
        );
    }

    #[test]
    fn today_is_evaluated_in_the_jobs_timezone() {
        // 02:00 UTC on Aug 5 is still Aug 4 in Vancouver (UTC-7): a success
        // at that instant does NOT count as "today, Aug 5" for a
        // Vancouver-scheduled job later that day.
        let schedule = JobSchedule::At {
            time: hhmm(7, 30),
            weekdays_only: false,
            timezone: Some(chrono_tz::America::Vancouver),
        };
        let recs = records(&[("calendar-sync", Some(utc(2026, 8, 5, 2, 0)))]);
        // 19:00 UTC = 12:00 Vancouver on Aug 5; fire was 07:30 Vancouver today.
        let now = utc(2026, 8, 5, 19, 0);
        let decision = evaluate_gate(&after(&["calendar-sync"]), &schedule, &recs, now);
        assert!(
            matches!(decision, GateDecision::Blocked { .. }),
            "{decision:?}"
        );

        // But a success at 15:00 UTC (= 08:00 Vancouver, Aug 5) counts.
        let recs = records(&[("calendar-sync", Some(utc(2026, 8, 5, 15, 0)))]);
        let decision = evaluate_gate(&after(&["calendar-sync"]), &schedule, &recs, now);
        assert_eq!(decision, GateDecision::Ready);
    }

    #[test]
    fn unmet_prereqs_treats_missing_records_as_unmet() {
        let unmet = unmet_prereqs(
            &after(&["a", "b"]),
            &records(&[("a", Some(utc(2026, 8, 5, 6, 0)))]),
            Some(chrono_tz::UTC),
            utc(2026, 8, 5, 8, 0),
        );
        assert_eq!(unmet, after(&["b"]));
    }
}
