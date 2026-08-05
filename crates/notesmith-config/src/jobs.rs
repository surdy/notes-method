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
//! A job is either `command`-kind (connector subprocess) or `agent`-kind
//! (headless `notesmith ai prompt` run with a named prompt, issue #282);
//! exactly one of `command`/`agent` must be set, which the runner's validator
//! enforces. `after` names other jobs that must have succeeded today before
//! this one runs (same-day ordering, also #282).

use serde::{Deserialize, Deserializer, Serialize};

/// Agent-kind job settings: `agent = { prompt = "daily-note", allow_writes = true }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobAgentConfig {
    /// Prompt name; the runner renders `.notesmith/prompts/<prompt>.md` (with
    /// its `context_queries`) and drives the headless agent with the result.
    pub prompt: String,
    /// Allow the headless agent to write to the vault. Defaults to false
    /// (read-only run) — mirroring `notesmith ai --allow-writes`.
    #[serde(default)]
    pub allow_writes: bool,
}

/// The `agent` field of a `[[jobs]]` entry, parsed leniently: a well-formed
/// table becomes [`JobAgentConfig`]; any other shape is preserved raw so the
/// entry still round-trips through the config API and the validator can
/// surface a `config_error` instead of silently dropping the job (ADR 0009).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum JobAgentField {
    Valid(JobAgentConfig),
    Malformed(serde_json::Value),
}

impl JobAgentField {
    /// The typed agent settings, when the field parsed cleanly.
    pub fn config(&self) -> Option<&JobAgentConfig> {
        match self {
            JobAgentField::Valid(config) => Some(config),
            JobAgentField::Malformed(_) => None,
        }
    }
}

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
    /// Agent-kind job settings (mutually exclusive with `command`). Parsed
    /// leniently — see [`JobAgentField`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<JobAgentField>,
    /// Same-day ordering: names of jobs that must have a successful run today
    /// before this one fires. Manual triggers bypass this gate.
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
    fn agent_and_after_fields_parse_typed_and_round_trip() {
        let job: JobConfig = serde_json::from_value(json!({
            "name": "daily-briefing",
            "at": "07:30",
            "after": ["calendar-sync"],
            "agent": { "prompt": "daily-note", "allow_writes": true }
        }))
        .unwrap();

        assert_eq!(job.after, vec!["calendar-sync".to_string()]);
        let agent = job.agent.as_ref().unwrap().config().unwrap();
        assert_eq!(agent.prompt, "daily-note");
        assert!(agent.allow_writes);

        let back = serde_json::to_value(&job).unwrap();
        assert_eq!(back["agent"]["allow_writes"], json!(true));
        assert_eq!(back["after"], json!(["calendar-sync"]));
    }

    #[test]
    fn agent_allow_writes_defaults_to_false() {
        let job: JobConfig = serde_json::from_value(json!({
            "name": "daily-briefing",
            "agent": { "prompt": "daily-note" }
        }))
        .unwrap();
        let agent = job.agent.as_ref().unwrap().config().unwrap();
        assert_eq!(agent.prompt, "daily-note");
        assert!(!agent.allow_writes);
    }

    #[test]
    fn malformed_agent_field_is_preserved_not_dropped() {
        // Wrong shape (a bare string, a table missing `prompt`, a mistyped
        // flag): the entry must survive with the raw value preserved so the
        // validator can report it — never drop the whole job or crash.
        for bad in [
            json!("daily-note"),
            json!({ "allow_writes": true }),
            json!({ "prompt": 42 }),
            json!({ "prompt": "x", "allow_writes": "yes" }),
        ] {
            let job: JobConfig = serde_json::from_value(json!({
                "name": "daily-briefing",
                "agent": bad.clone()
            }))
            .unwrap_or_else(|e| panic!("agent shape {bad} must not fail the entry: {e}"));
            let agent = job.agent.as_ref().unwrap();
            assert_eq!(agent.config(), None, "shape {bad} must be Malformed");
            assert_eq!(agent, &JobAgentField::Malformed(bad.clone()));

            // Round-trips verbatim.
            let back = serde_json::to_value(&job).unwrap();
            assert_eq!(back["agent"], bad);
        }
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
