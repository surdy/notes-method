//! `notesmith job` — daemon-backed job commands (issue #280, ADR 0025).
//!
//! Jobs are declared in the vault's `[[jobs]]` config and executed by the
//! daemon's job runner; these commands go through the daemon like other
//! daemon-backed commands (list for debuggability, run for manual triggers —
//! the connector-development workflow).

use std::path::Path;

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use serde_json::Value;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum JobCommand {
    /// List configured jobs with schedule, validity, and last-run status
    List,
    /// Manually trigger a job by name (bypasses its schedule)
    Run {
        /// Job name from the vault's `[[jobs]]` config
        name: String,
    },
}

impl JobCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        crate::daemon_client::ensure_daemon(global_config).await?;
        let detected = detect_vault(cwd, explicit_vault, global_config)?;

        match self {
            JobCommand::List => list_jobs(global_config, &detected.name, format).await,
            JobCommand::Run { name } => run_job(global_config, &detected.name, name, format).await,
        }
    }
}

fn jobs_url(global_config: &GlobalConfig, vault: &str) -> anyhow::Result<reqwest::Url> {
    let mut url = crate::daemon_client::daemon_url(global_config)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
        .push("api")
        .push("v")
        .push(vault)
        .push("jobs");
    Ok(url)
}

fn connect_error(global_config: &GlobalConfig, error: reqwest::Error) -> anyhow::Error {
    if error.is_connect() {
        anyhow::anyhow!(
            "could not reach the Notesmith daemon at {}",
            global_config.daemon.bind
        )
    } else {
        anyhow::anyhow!("job request failed: {error}")
    }
}

async fn list_jobs(
    global_config: &GlobalConfig,
    vault: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let url = jobs_url(global_config, vault)?;
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| connect_error(global_config, error))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("job list failed with {status}: {body}");
    }

    let body: Value = response.json().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&body)?),
        OutputFormat::Text => {
            let jobs = body["jobs"].as_array().cloned().unwrap_or_default();
            if jobs.is_empty() {
                println!("No jobs configured for {vault} (add [[jobs]] to vault.toml)");
                return Ok(());
            }
            for job in &jobs {
                println!("{}", format_job_line(job));
            }
        }
    }
    Ok(())
}

/// Render one job list entry as a single text line. Pure and unit-testable.
fn format_job_line(job: &Value) -> String {
    let name = job["name"].as_str().unwrap_or("<unnamed>");
    let schedule = if let Some(every) = job["every"].as_str() {
        format!("every {every}")
    } else if let Some(at) = job["at"].as_str() {
        let mut schedule = format!("at {at}");
        if job["weekdays_only"].as_bool().unwrap_or(false) {
            schedule.push_str(" weekdays");
        }
        if let Some(tz) = job["timezone"].as_str() {
            schedule.push_str(&format!(" ({tz})"));
        }
        schedule
    } else {
        "unscheduled".to_string()
    };

    let state = if !job["enabled"].as_bool().unwrap_or(true) {
        "disabled".to_string()
    } else if let Some(reason) = job["config_error"].as_str() {
        format!("invalid: {reason}")
    } else if job["running"].as_bool().unwrap_or(false) {
        "running".to_string()
    } else {
        "enabled".to_string()
    };

    let last = match job["last_run"].as_object() {
        Some(last_run) => format!(
            "last: {} {}",
            last_run
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("?"),
            last_run.get("at").and_then(Value::as_str).unwrap_or("?")
        ),
        None => "last: never".to_string(),
    };

    format!("{name}  [{schedule}]  {state}  {last}")
}

async fn run_job(
    global_config: &GlobalConfig,
    vault: &str,
    name: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let mut url = jobs_url(global_config, vault)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
        .push(name)
        .push("run");

    let response = reqwest::Client::new()
        .post(url)
        .send()
        .await
        .map_err(|error| connect_error(global_config, error))?;

    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);

    if status == reqwest::StatusCode::ACCEPTED {
        match format {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&body)?),
            OutputFormat::Text => println!(
                "Started job {name} in {vault} — watch `notesmith job list` or job.* events for the outcome"
            ),
        }
        return Ok(());
    }

    let error = body["error"].as_str().unwrap_or("unknown error");
    match status.as_u16() {
        409 => anyhow::bail!("job {name} is already running"),
        404 => anyhow::bail!("{error}"),
        _ => anyhow::bail!("job run failed with {status}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_job_line_every_job_never_run() {
        let line = format_job_line(&json!({
            "name": "calendar-sync",
            "enabled": true,
            "every": "15m",
            "weekdays_only": false,
            "valid": true,
            "running": false
        }));
        assert_eq!(line, "calendar-sync  [every 15m]  enabled  last: never");
    }

    #[test]
    fn format_job_line_at_job_with_timezone_and_last_run() {
        let line = format_job_line(&json!({
            "name": "email-digest",
            "enabled": true,
            "at": "07:30",
            "weekdays_only": true,
            "timezone": "America/Vancouver",
            "valid": true,
            "running": false,
            "last_run": { "status": "succeeded", "at": "2026-08-05T07:30:02+00:00" }
        }));
        assert_eq!(
            line,
            "email-digest  [at 07:30 weekdays (America/Vancouver)]  enabled  last: succeeded 2026-08-05T07:30:02+00:00"
        );
    }

    #[test]
    fn format_job_line_flags_disabled_invalid_and_running() {
        let disabled = format_job_line(&json!({
            "name": "x", "enabled": false, "every": "5m"
        }));
        assert!(disabled.contains("disabled"));

        let invalid = format_job_line(&json!({
            "name": "x", "enabled": true, "config_error": "job is missing a `command`"
        }));
        assert!(invalid.contains("invalid: job is missing a `command`"));
        assert!(invalid.contains("[unscheduled]"));

        let running = format_job_line(&json!({
            "name": "x", "enabled": true, "every": "5m", "running": true
        }));
        assert!(running.contains("running"));
    }
}
