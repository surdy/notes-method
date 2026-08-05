//! Daily note scheduler: ensures daily notes exist and runs on a configurable schedule.

use std::path::Path;

use chrono::{Local, NaiveDate, NaiveTime};
use notesmith_config::VaultConfig;
use notesmith_core::{PeriodKind, VaultEngine};
use tokio::task::JoinHandle;

use crate::server::SharedAppState;

// Periodic-note path resolution and creation moved to `notesmith_ops::periodic`
// (issue #279) so ops/MCP, HTTP routes, and the scheduler share one path
// resolver. Re-exported here to keep this module's public API stable.
pub use notesmith_ops::periodic::{
    EnsurePeriodicResult, ResolvedPeriodicNote, ensure_periodic_note, resolve_periodic_note,
};

pub struct DailyScheduler {
    _tasks: Vec<JoinHandle<()>>,
}

/// Ensure a daily note exists for the given date, using the vault's effective
/// `[periodic.daily]` config (folder, template, and filename pattern — issue
/// #279: the scheduler must resolve the same path as ops/MCP and the REST
/// route). Returns `Ok(Some(path))` if created, `Ok(None)` if already exists.
pub fn ensure_daily_note(
    vault_root: &Path,
    vault_config: &VaultConfig,
    date: NaiveDate,
    template_engine: &notesmith_templates::TemplateEngine,
    engine: &dyn VaultEngine,
) -> anyhow::Result<Option<String>> {
    Ok(ensure_periodic_note(
        vault_root,
        &vault_config.effective_daily_periodic(),
        PeriodKind::Daily,
        date,
        template_engine,
        engine,
    )?
    .created_path)
}

/// Run catch-up: create daily notes for any missing days in the last 30 days.
pub fn catch_up_daily_notes(
    vault_root: &Path,
    vault_config: &VaultConfig,
    template_engine: &notesmith_templates::TemplateEngine,
    engine: &dyn VaultEngine,
) -> anyhow::Result<Vec<String>> {
    let today = Local::now().date_naive();
    let mut created = Vec::new();

    for days_ago in (0..30).rev() {
        let date = today - chrono::Duration::days(days_ago);
        if let Some(path) =
            ensure_daily_note(vault_root, vault_config, date, template_engine, engine)?
        {
            created.push(path);
        }
    }

    Ok(created)
}

/// Start daily schedulers for all vaults that have `generate_at` configured.
pub async fn start_daily_schedulers(state: SharedAppState) -> Vec<DailyScheduler> {
    let vault_configs: Vec<(String, notesmith_config::DailyConfig)> = {
        let state = state.read().await;
        state
            .vaults
            .iter()
            .map(|(name, vs)| (name.clone(), vs.vault_config.load().daily.clone()))
            .collect()
    };

    let mut schedulers = Vec::new();

    for (vault_name, daily_config) in vault_configs {
        let Some(ref generate_at) = daily_config.generate_at else {
            continue;
        };

        let generate_at = generate_at.clone();
        let timezone = daily_config.timezone.clone();
        let catch_up = daily_config.catch_up;
        let state_clone = state.clone();
        let vault_name_clone = vault_name.clone();

        let task = tokio::spawn(async move {
            if catch_up {
                run_catch_up(&state_clone, &vault_name_clone).await;
            }

            loop {
                let delay = compute_delay_until(&generate_at, timezone.as_deref());
                tokio::time::sleep(delay).await;

                let today = Local::now().date_naive();
                let state = state_clone.read().await;
                if let Some(vault) = state.vaults.get(&vault_name_clone) {
                    let vault_config = vault.vault_config.load();
                    if let Ok(Some(path)) = ensure_daily_note(
                        &vault.root,
                        &vault_config,
                        today,
                        &vault.template_engine,
                        &vault.engine,
                    ) {
                        crate::events::emit(
                            &state.event_tx,
                            &state.event_buffer,
                            crate::events::VaultEvent::new(
                                &vault_name_clone,
                                crate::events::EventType::DailyCreated,
                                &path,
                            ),
                        );
                    }
                }
                drop(state);

                // Prevent firing twice in the same minute
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });

        schedulers.push(DailyScheduler { _tasks: vec![task] });
    }

    schedulers
}

async fn run_catch_up(state: &SharedAppState, vault_name: &str) {
    let state = state.read().await;
    if let Some(vault) = state.vaults.get(vault_name) {
        let config = vault.vault_config.load();
        match catch_up_daily_notes(&vault.root, &config, &vault.template_engine, &vault.engine) {
            Ok(created) => {
                if !created.is_empty() {
                    tracing::info!(
                        "daily catch-up for {vault_name}: created {} notes",
                        created.len()
                    );
                }
            }
            Err(e) => {
                tracing::warn!("daily catch-up failed for {vault_name}: {e}");
            }
        }
    }
}

/// Compute how long to sleep before the next trigger at `time_str` (HH:MM),
/// interpreted in `timezone` (an IANA name) or daemon-local time when absent
/// or unknown. Shares the tz-aware time-of-day math with the job runner
/// (`crate::jobs::schedule`).
pub fn compute_delay_until(time_str: &str, timezone: Option<&str>) -> std::time::Duration {
    let target_time = NaiveTime::parse_from_str(time_str, "%H:%M").unwrap_or_else(|_| {
        tracing::warn!(time = %time_str, "invalid generate_at time; defaulting to 06:30");
        NaiveTime::from_hms_opt(6, 30, 0).expect("06:30 is a valid time")
    });
    let tz = crate::jobs::schedule::resolve_timezone(timezone);
    crate::jobs::schedule::delay_until_time_of_day(target_time, tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_vault::NativeVaultEngine;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn golden_vault() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("golden-vault")
    }

    /// Set up a temp vault with templates copied from golden-vault.
    fn setup_temp_vault() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();

        let templates_src = golden_vault().join(".notesmith").join("templates");
        let templates_dst = root.join(".notesmith").join("templates");
        std::fs::create_dir_all(&templates_dst).unwrap();
        for entry in std::fs::read_dir(&templates_src).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), templates_dst.join(entry.file_name())).unwrap();
        }

        (temp_dir, root)
    }

    /// A vault config whose effective daily settings are the given folder,
    /// template, and filename pattern.
    fn daily_vault_config(folder: &str, template: &str, filename: &str) -> VaultConfig {
        let mut config = VaultConfig::default();
        config.periodic.daily = Some(notesmith_config::PeriodKindConfig {
            folder: folder.to_string(),
            template: Some(template.to_string()),
            filename: filename.to_string(),
            generate_at: None,
            timezone: None,
            catch_up: false,
        });
        config
    }

    #[test]
    fn ensure_daily_note_creates_when_missing() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let config = daily_vault_config("Daily", "daily", "{{ date }}");

        let result = ensure_daily_note(&root, &config, date, &template_engine, &engine).unwrap();

        assert_eq!(result, Some("Daily/2025-06-15.md".to_string()));

        let content = std::fs::read_to_string(root.join("Daily/2025-06-15.md")).unwrap();
        assert!(content.contains("# 2025-06-15"));
        assert!(content.contains("date: 2025-06-15"));
    }

    #[test]
    fn ensure_daily_note_idempotent() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let config = daily_vault_config("Daily", "daily", "{{ date }}");

        let first = ensure_daily_note(&root, &config, date, &template_engine, &engine).unwrap();
        assert!(first.is_some());

        let second = ensure_daily_note(&root, &config, date, &template_engine, &engine).unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn ensure_daily_note_honors_custom_periodic_filename() {
        // Issue #279 follow-up: the scheduler path must resolve the same
        // custom `[periodic.daily] filename` as ops/MCP and the REST route.
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let config = daily_vault_config("Daily", "daily", "Journal {{ date }}");

        let result = ensure_daily_note(&root, &config, date, &template_engine, &engine).unwrap();

        assert_eq!(result, Some("Daily/Journal 2026-03-05.md".to_string()));
        assert!(root.join("Daily/Journal 2026-03-05.md").exists());
    }

    #[test]
    fn ensure_daily_note_falls_back_to_legacy_daily_config() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2026, 3, 6).unwrap();
        let mut config = VaultConfig::default();
        config.periodic.daily = None;
        config.daily.folder = "Daily".to_string();
        config.daily.template = "daily".to_string();
        config.daily.filename = "Log {{ date }}".to_string();

        let result = ensure_daily_note(&root, &config, date, &template_engine, &engine).unwrap();

        assert_eq!(result, Some("Daily/Log 2026-03-06.md".to_string()));
    }

    #[test]
    fn ensure_daily_note_uses_correct_date_not_today() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let config = daily_vault_config("Daily", "daily", "{{ date }}");

        ensure_daily_note(&root, &config, date, &template_engine, &engine).unwrap();

        let content = std::fs::read_to_string(root.join("Daily/2024-01-01.md")).unwrap();
        assert!(
            content.contains("# 2024-01-01"),
            "expected template to use overridden date, got:\n{content}"
        );
        assert!(content.contains("date: 2024-01-01"));
    }

    #[test]
    fn ensure_periodic_note_creates_weekly_note_from_configured_filename() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        std::fs::write(
            root.join(".notesmith/templates/weekly.md"),
            r#"---
notesmith:
  name: weekly
  description: Weekly note
  output_path: "ignored/{{ week }}.md"
---
# {{ period_key }}
{{ period_start }} → {{ period_end }}
"#,
        )
        .unwrap();
        let config = notesmith_config::PeriodicConfig {
            weekly: Some(notesmith_config::PeriodKindConfig {
                folder: "Weekly".to_string(),
                template: Some("weekly".to_string()),
                filename: "Week {{ week }}".to_string(),
                generate_at: None,
                timezone: None,
                catch_up: false,
            }),
            ..Default::default()
        };
        let date = NaiveDate::from_ymd_opt(2026, 5, 23).unwrap();

        let result = ensure_periodic_note(
            &root,
            &config,
            notesmith_core::PeriodKind::Weekly,
            date,
            &template_engine,
            &engine,
        )
        .unwrap();

        assert_eq!(
            result.created_path,
            Some("Weekly/Week 2026-W21.md".to_string())
        );
        let content = std::fs::read_to_string(root.join("Weekly/Week 2026-W21.md")).unwrap();
        assert!(content.contains("# 2026-W21"));
        assert!(content.contains("2026-05-18 → 2026-05-24"));
    }

    #[test]
    fn catch_up_creates_missing_days() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);

        let config = daily_vault_config("Daily", "daily", "{{ date }}");
        let created = catch_up_daily_notes(&root, &config, &template_engine, &engine).unwrap();

        // Should have created today + up to 29 days back
        assert!(!created.is_empty());
        // Today's note should exist
        let today = Local::now().format("%Y-%m-%d").to_string();
        let today_path = root.join(format!("Daily/{today}.md"));
        assert!(today_path.exists());
    }

    #[test]
    fn compute_delay_future_today() {
        // If we set target far in the future today, delay should be positive and < 24h
        let delay = compute_delay_until("23:59", None);
        assert!(delay.as_secs() <= 86400);
        assert!(delay.as_secs() > 0);
    }

    #[test]
    fn compute_delay_past_today_schedules_tomorrow() {
        // 00:00 is always in the past (unless it's exactly midnight)
        let delay = compute_delay_until("00:00", None);
        // Should schedule for tomorrow, so delay > ~0 seconds
        assert!(delay.as_secs() > 0);
        assert!(delay.as_secs() <= 86400);
    }

    #[test]
    fn compute_delay_honors_named_timezone() {
        // Delays to the same wall-clock time in two zones must differ by the
        // zone offset (mod 24h): Tokyo (UTC+9) reaches its noon 9h before UTC.
        let tokyo = compute_delay_until("12:00", Some("Asia/Tokyo"));
        let utc = compute_delay_until("12:00", Some("UTC"));
        let diff_secs = (tokyo.as_secs() as i64 - utc.as_secs() as i64).rem_euclid(86_400);
        assert!((diff_secs - 15 * 3600).abs() <= 2, "diff was {diff_secs}s");
    }

    #[test]
    fn compute_delay_unknown_timezone_falls_back_to_local() {
        let delay = compute_delay_until("23:59", Some("Not/AZone"));
        assert!(delay.as_secs() > 0);
        assert!(delay.as_secs() <= 86_400 + 3600);
    }

    #[test]
    fn compute_delay_invalid_time_uses_default() {
        let delay = compute_delay_until("not-a-time", None);
        // Should use default 06:30 and produce a valid delay
        assert!(delay.as_secs() > 0);
        assert!(delay.as_secs() <= 86400);
    }
}
