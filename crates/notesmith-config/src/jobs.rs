//! Per-vault `[[jobs]]` configuration (ADR 0025 Decision 2).
//!
//! A job is a scheduled unit of work executed by the daemon's generic job
//! runner. This module only models the *config*; scheduling semantics live in
//! the daemon (`notesmith-http::jobs`).
//!
//! Parsing is deliberately lenient per entry: one malformed `[[jobs]]` table
//! must never fail the whole `vault.toml` parse (which would take out capture,
//! daily notes, git timers, … per ADR 0009). Bad entries are skipped with a
//! WARN; the rest of the config loads normally. Schedule *semantics* (e.g.
//! `every` vs `at` mutual exclusion) are validated by the runner, not here, so
//! an invalid schedule still round-trips through the config API and can be
//! surfaced/fixed by the user.
//!
//! Only `command`-kind jobs are executed today. The `agent` and `after` fields
//! are reserved for #282 (agent-kind jobs, same-day ordering): they parse and
//! round-trip so a future daemon can pick them up, but the runner ignores them.

use serde::{Deserialize, Deserializer, Serialize};

/// One `[[jobs]]` entry in `vault.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JobConfig {
    /// Unique (per vault) job name; the key for manual triggers, state, and
    /// `job.*` events.
    #[serde(default)]
    pub name: String,
    /// Whether the runner schedules this job. Re-read each tick, so toggling
    /// takes effect without a daemon restart.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Interval schedule (e.g. `"15m"`, `"1h"`). Mutually exclusive with `at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,
    /// Time-of-day schedule (`"HH:MM"`), with catch-up-on-wake. Mutually
    /// exclusive with `every`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// For `at` jobs: only fire Monday–Friday.
    #[serde(default)]
    pub weekdays_only: bool,
    /// Optional IANA timezone name (e.g. `"America/Vancouver"`) for `at`
    /// schedules. Defaults to daemon-local time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Vault-relative path of the executable to run (command-kind jobs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Subprocess wall-clock budget (e.g. `"120s"`); the process is killed on
    /// expiry. Defaults to 120s in the runner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Reserved for #282: agent-kind jobs (`agent = { prompt = "…" }`).
    /// Parsed and round-tripped but not executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<serde_json::Value>,
    /// Reserved for #282: same-day ordering. Parsed and round-tripped but not
    /// honored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

/// Lenient deserializer for the `[[jobs]]` array: each entry is first read as
/// a free-form value, then converted to [`JobConfig`]. Entries that fail
/// conversion (wrong types, etc.) are logged with a WARN and dropped instead
/// of failing the whole `vault.toml` parse (ADR 0009).
pub(crate) fn deserialize_jobs_lenient<'de, D>(deserializer: D) -> Result<Vec<JobConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(convert_job_entries(raw))
}

/// Convert loosely-typed job entries into [`JobConfig`]s, skipping (with a
/// WARN) any entry that does not fit the schema. Pure and unit-testable.
pub(crate) fn convert_job_entries(raw: Vec<serde_json::Value>) -> Vec<JobConfig> {
    raw.into_iter()
        .enumerate()
        .filter_map(
            |(index, value)| match serde_json::from_value::<JobConfig>(value) {
                Ok(job) => Some(job),
                Err(error) => {
                    tracing::warn!(
                        entry = index,
                        reason = %error,
                        "skipping malformed [[jobs]] entry in vault.toml"
                    );
                    None
                }
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn convert_keeps_valid_entries_and_drops_malformed_ones() {
        let entries = vec![
            json!({ "name": "calendar-sync", "every": "15m", "command": "sync.py" }),
            json!({ "name": "bad", "enabled": "yes-please" }),
            json!({ "name": "email-digest", "at": "07:30", "weekdays_only": true }),
        ];

        let jobs = convert_job_entries(entries);

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].name, "calendar-sync");
        assert_eq!(jobs[0].every.as_deref(), Some("15m"));
        assert_eq!(jobs[1].name, "email-digest");
        assert!(jobs[1].weekdays_only);
    }

    #[test]
    fn job_defaults_are_enabled_with_no_schedule() {
        let job: JobConfig = serde_json::from_value(json!({ "name": "x" })).unwrap();
        assert!(job.enabled);
        assert!(job.every.is_none());
        assert!(job.at.is_none());
        assert!(!job.weekdays_only);
        assert!(job.timezone.is_none());
        assert!(job.command.is_none());
        assert!(job.timeout.is_none());
        assert!(job.agent.is_none());
        assert!(job.after.is_empty());
    }

    #[test]
    fn reserved_agent_and_after_fields_round_trip() {
        let job: JobConfig = serde_json::from_value(json!({
            "name": "daily-briefing",
            "at": "07:30",
            "after": ["calendar-sync"],
            "agent": { "prompt": "daily-note", "allow_writes": true }
        }))
        .unwrap();

        assert_eq!(job.after, vec!["calendar-sync".to_string()]);
        let agent = job.agent.as_ref().unwrap();
        assert_eq!(agent["prompt"], json!("daily-note"));

        let back = serde_json::to_value(&job).unwrap();
        assert_eq!(back["agent"]["allow_writes"], json!(true));
        assert_eq!(back["after"], json!(["calendar-sync"]));
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let job: JobConfig = serde_json::from_value(json!({
            "name": "future",
            "every": "5m",
            "brand_new_option": 42
        }))
        .unwrap();
        assert_eq!(job.name, "future");
    }
}
