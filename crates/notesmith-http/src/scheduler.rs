//! Daily note scheduler: ensures daily notes exist and runs on a configurable schedule.

use std::collections::HashMap;
use std::path::Path;

use chrono::{Local, NaiveDate, NaiveTime};
use tokio::task::JoinHandle;

use crate::server::SharedAppState;

pub struct DailyScheduler {
    _tasks: Vec<JoinHandle<()>>,
}

/// Ensure a daily note exists for the given date.
/// Returns `Ok(Some(path))` if created, `Ok(None)` if already exists.
pub fn ensure_daily_note(
    vault_root: &Path,
    daily_folder: &str,
    daily_template: &str,
    date: NaiveDate,
    template_engine: &notesmith_templates::TemplateEngine,
    engine: &dyn notesmith_core::VaultEngine,
) -> anyhow::Result<Option<String>> {
    let date_str = date.format("%Y-%m-%d").to_string();
    let expected_path = format!("{daily_folder}/{date_str}.md");
    let vault_path = notesmith_core::VaultPath::new(expected_path);

    // Check if note already exists
    match engine.read(vault_root, &vault_path) {
        Ok(_) => return Ok(None),
        Err(notesmith_core::NotesmithError::NoteNotFound { .. }) => {}
        Err(e) => return Err(e.into()),
    }

    // Create via template, overriding "today" with the target date
    let mut prompts = HashMap::new();
    prompts.insert("today".to_string(), date_str);

    let rendered = template_engine.instantiate(daily_template, &prompts, engine)?;
    Ok(Some(rendered.path))
}

/// Run catch-up: create daily notes for any missing days in the last 30 days.
pub fn catch_up_daily_notes(
    vault_root: &Path,
    daily_folder: &str,
    daily_template: &str,
    template_engine: &notesmith_templates::TemplateEngine,
    engine: &dyn notesmith_core::VaultEngine,
) -> anyhow::Result<Vec<String>> {
    let today = Local::now().date_naive();
    let mut created = Vec::new();

    for days_ago in (0..30).rev() {
        let date = today - chrono::Duration::days(days_ago);
        if let Some(path) = ensure_daily_note(
            vault_root,
            daily_folder,
            daily_template,
            date,
            template_engine,
            engine,
        )? {
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
                    let daily_config = vault.vault_config.load();
                    if let Ok(Some(path)) = ensure_daily_note(
                        &vault.root,
                        &daily_config.daily.folder,
                        &daily_config.daily.template,
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
        match catch_up_daily_notes(
            &vault.root,
            &config.daily.folder,
            &config.daily.template,
            &vault.template_engine,
            &vault.engine,
        ) {
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

/// Compute how long to sleep before the next trigger at `time_str` (HH:MM).
pub fn compute_delay_until(time_str: &str, _timezone: Option<&str>) -> std::time::Duration {
    let target_time = NaiveTime::parse_from_str(time_str, "%H:%M")
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(6, 30, 0).unwrap());

    let now = Local::now();
    let today_target = now.date_naive().and_time(target_time);

    let target_datetime = if today_target > now.naive_local() {
        today_target
    } else {
        (now.date_naive() + chrono::Duration::days(1)).and_time(target_time)
    };

    let duration = target_datetime - now.naive_local();
    duration
        .to_std()
        .unwrap_or(std::time::Duration::from_secs(3600))
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

        let templates_src = golden_vault().join("Assets").join("templates");
        let templates_dst = root.join("Assets").join("templates");
        std::fs::create_dir_all(&templates_dst).unwrap();
        for entry in std::fs::read_dir(&templates_src).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), templates_dst.join(entry.file_name())).unwrap();
        }

        (temp_dir, root)
    }

    #[test]
    fn ensure_daily_note_creates_when_missing() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();

        let result = ensure_daily_note(
            &root,
            "Inbox/Daily",
            "daily-note",
            date,
            &template_engine,
            &engine,
        )
        .unwrap();

        assert_eq!(result, Some("Inbox/Daily/2025-06-15.md".to_string()));

        let content = std::fs::read_to_string(root.join("Inbox/Daily/2025-06-15.md")).unwrap();
        assert!(content.contains("# 2025-06-15"));
        assert!(content.contains("date: 2025-06-15"));
    }

    #[test]
    fn ensure_daily_note_idempotent() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();

        let first = ensure_daily_note(
            &root,
            "Inbox/Daily",
            "daily-note",
            date,
            &template_engine,
            &engine,
        )
        .unwrap();
        assert!(first.is_some());

        let second = ensure_daily_note(
            &root,
            "Inbox/Daily",
            "daily-note",
            date,
            &template_engine,
            &engine,
        )
        .unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn ensure_daily_note_uses_correct_date_not_today() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);
        let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();

        ensure_daily_note(
            &root,
            "Inbox/Daily",
            "daily-note",
            date,
            &template_engine,
            &engine,
        )
        .unwrap();

        let content = std::fs::read_to_string(root.join("Inbox/Daily/2024-01-01.md")).unwrap();
        assert!(
            content.contains("# 2024-01-01"),
            "expected template to use overridden date, got:\n{content}"
        );
        assert!(content.contains("date: 2024-01-01"));
    }

    #[test]
    fn catch_up_creates_missing_days() {
        let (_tmp, root) = setup_temp_vault();
        let engine = NativeVaultEngine;
        let template_engine = notesmith_templates::TemplateEngine::new(root.clone(), None);

        let created = catch_up_daily_notes(
            &root,
            "Inbox/Daily",
            "daily-note",
            &template_engine,
            &engine,
        )
        .unwrap();

        // Should have created today + up to 29 days back
        assert!(!created.is_empty());
        // Today's note should exist
        let today = Local::now().format("%Y-%m-%d").to_string();
        let today_path = root.join(format!("Inbox/Daily/{today}.md"));
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
    fn compute_delay_invalid_time_uses_default() {
        let delay = compute_delay_until("not-a-time", None);
        // Should use default 06:30 and produce a valid delay
        assert!(delay.as_secs() > 0);
        assert!(delay.as_secs() <= 86400);
    }
}
