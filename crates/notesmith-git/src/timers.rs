//! Timer management for auto-commit, auto-pull, and auto-push.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use notesmith_config::GitConfig;

/// Configuration needed to start git timers for a single vault.
#[derive(Debug, Clone)]
pub struct GitTimerConfig {
    pub vault_name: String,
    pub vault_root: PathBuf,
    pub config: GitConfig,
}

/// Parse a human-friendly duration string like "5m", "30s", "1h".
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (digits, suffix) = s.split_at(s.len() - 1);
    let value: u64 = digits.parse().ok()?;

    match suffix {
        "s" => Some(Duration::from_secs(value)),
        "m" => Some(Duration::from_secs(value * 60)),
        "h" => Some(Duration::from_secs(value * 3600)),
        _ => None,
    }
}

/// Spawn timer tasks for each vault with git enabled.
///
/// Returns the join handles so callers can manage the task lifetimes.
pub async fn start_git_timers(configs: Vec<GitTimerConfig>) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    for cfg in configs {
        if !cfg.config.enabled {
            continue;
        }

        if !crate::ops::is_git_repo(&cfg.vault_root) {
            tracing::warn!(
                vault = %cfg.vault_name,
                "git enabled but vault root is not a git repository; skipping timers"
            );
            continue;
        }

        // Auto-commit timer
        if let Some(ref interval_str) = cfg.config.auto_commit_every {
            if let Some(interval) = parse_duration(interval_str) {
                let vault_name = cfg.vault_name.clone();
                let vault_root = cfg.vault_root.clone();
                let interval_display = interval_str.clone();
                let message = cfg.config.commit_message.clone();

                handles.push(tokio::spawn(async move {
                    tracing::info!(vault = %vault_name, every = %interval_display, "starting auto-commit timer");
                    loop {
                        tokio::time::sleep(interval).await;
                        match crate::ops::commit_all(&vault_root, message.as_deref()) {
                            Ok(Some(outcome)) => {
                                tracing::info!(vault = %vault_name, sha = %outcome.sha, files = outcome.files.len(), "auto-committed");
                            }
                            Ok(None) => {
                                tracing::debug!(vault = %vault_name, "auto-commit: nothing to commit");
                            }
                            Err(e) => {
                                tracing::warn!(vault = %vault_name, error = %e, "auto-commit failed");
                            }
                        }
                    }
                }));
            }
        }

        // Inactivity-checkpoint timer (headless). Commits once the newest
        // working-tree change has been stable for the configured window. The
        // desktop editor drives its own inactivity checkpoint (which also
        // flushes unsaved buffers to disk first); this daemon timer is the
        // fallback for when the app isn't open/focused.
        if let Some(interval_str) = cfg.config.commit_on_inactivity.clone() {
            if let Some(window) = parse_duration(&interval_str) {
                let vault_name = cfg.vault_name.clone();
                let vault_root = cfg.vault_root.clone();
                let interval_display = interval_str;
                let message = cfg.config.commit_message.clone();
                // Poll several times within the window, bounded to a sane range.
                let tick = window
                    .min(Duration::from_secs(30))
                    .max(Duration::from_secs(1));

                handles.push(tokio::spawn(async move {
                    tracing::info!(vault = %vault_name, after = %interval_display, "starting inactivity-commit timer");
                    loop {
                        tokio::time::sleep(tick).await;
                        match crate::ops::newest_change_mtime(&vault_root) {
                            Ok(Some(mtime)) => {
                                let age = SystemTime::now()
                                    .duration_since(mtime)
                                    .unwrap_or_default();
                                if age < window {
                                    continue;
                                }
                                match crate::ops::commit_all(&vault_root, message.as_deref()) {
                                    Ok(Some(outcome)) => {
                                        tracing::info!(vault = %vault_name, sha = %outcome.sha, files = outcome.files.len(), "inactivity checkpoint committed");
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        tracing::warn!(vault = %vault_name, error = %e, "inactivity commit failed");
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                tracing::warn!(vault = %vault_name, error = %e, "inactivity status check failed");
                            }
                        }
                    }
                }));
            }
        }

        // Auto-pull timer
        if let Some(interval_str) = cfg.config.auto_pull_every.clone() {
            if let Some(interval) = parse_duration(&interval_str) {
                let vault_name = cfg.vault_name.clone();
                let vault_root = cfg.vault_root.clone();
                let interval_display = interval_str;

                handles.push(tokio::spawn(async move {
                    tracing::info!(vault = %vault_name, every = %interval_display, "starting auto-pull timer");
                    loop {
                        tokio::time::sleep(interval).await;
                        match crate::ops::pull_ff(&vault_root, "origin") {
                            Ok(result) => {
                                if result.conflict {
                                    tracing::warn!(vault = %vault_name, "auto-pull: conflict detected, skipping");
                                } else if result.updated {
                                    tracing::info!(vault = %vault_name, head = ?result.new_head, "auto-pull: updated");
                                } else {
                                    tracing::debug!(vault = %vault_name, "auto-pull: already up to date");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(vault = %vault_name, error = %e, "auto-pull failed");
                            }
                        }
                    }
                }));
            }
        }

        // Auto-push timer (always pulls first)
        if let Some(interval_str) = cfg.config.auto_push_every.clone() {
            if let Some(interval) = parse_duration(&interval_str) {
                let vault_name = cfg.vault_name.clone();
                let vault_root = cfg.vault_root.clone();
                let interval_display = interval_str;

                handles.push(tokio::spawn(async move {
                    tracing::info!(vault = %vault_name, every = %interval_display, "starting auto-push timer");
                    loop {
                        tokio::time::sleep(interval).await;

                        // Pull first to minimize conflicts
                        match crate::ops::pull_ff(&vault_root, "origin") {
                            Ok(result) if result.conflict => {
                                tracing::warn!(vault = %vault_name, "auto-push: pull conflict, skipping push");
                                continue;
                            }
                            Err(e) => {
                                tracing::warn!(vault = %vault_name, error = %e, "auto-push: pull failed, skipping push");
                                continue;
                            }
                            _ => {}
                        }

                        match crate::ops::push(&vault_root, "origin") {
                            Ok(result) => {
                                if result.pushed {
                                    tracing::info!(vault = %vault_name, "auto-push: pushed successfully");
                                } else if let Some(ref err) = result.error {
                                    tracing::warn!(vault = %vault_name, error = %err, "auto-push failed");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(vault = %vault_name, error = %e, "auto-push failed");
                            }
                        }
                    }
                }));
            }
        }
    }

    handles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("5m"), Some(Duration::from_secs(300)));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn parse_duration_invalid_suffix() {
        assert_eq!(parse_duration("5x"), None);
    }

    #[test]
    fn parse_duration_empty() {
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn parse_duration_no_number() {
        assert_eq!(parse_duration("m"), None);
    }

    #[test]
    fn parse_duration_trims_whitespace() {
        assert_eq!(parse_duration(" 10s "), Some(Duration::from_secs(10)));
    }

    #[tokio::test]
    async fn start_git_timers_skips_disabled_vaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = GitTimerConfig {
            vault_name: "test".into(),
            vault_root: dir.path().to_path_buf(),
            config: GitConfig {
                enabled: false,
                auto_commit_every: Some("5m".into()),
                commit_on_inactivity: None,
                auto_pull_every: None,
                auto_push_every: None,
                commit_message: None,
            },
        };

        let handles = start_git_timers(vec![config]).await;
        assert!(
            handles.is_empty(),
            "should not spawn timers for disabled vaults"
        );
    }

    #[tokio::test]
    async fn start_git_timers_skips_non_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        // Not a git repo — should skip
        let config = GitTimerConfig {
            vault_name: "test".into(),
            vault_root: dir.path().to_path_buf(),
            config: GitConfig {
                enabled: true,
                auto_commit_every: Some("5m".into()),
                commit_on_inactivity: None,
                auto_pull_every: None,
                auto_push_every: None,
                commit_message: None,
            },
        };

        let handles = start_git_timers(vec![config]).await;
        assert!(
            handles.is_empty(),
            "should not spawn timers for non-git dirs"
        );
    }

    #[tokio::test]
    async fn start_git_timers_spawns_for_enabled_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();

        let config = GitTimerConfig {
            vault_name: "test".into(),
            vault_root: dir.path().to_path_buf(),
            config: GitConfig {
                enabled: true,
                auto_commit_every: Some("5m".into()),
                commit_on_inactivity: None,
                auto_pull_every: Some("10m".into()),
                auto_push_every: Some("15m".into()),
                commit_message: None,
            },
        };

        let handles = start_git_timers(vec![config]).await;
        assert_eq!(
            handles.len(),
            3,
            "should spawn commit, pull, and push timers"
        );

        // Clean up tasks
        for h in handles {
            h.abort();
        }
    }

    #[tokio::test]
    async fn start_git_timers_spawns_only_configured_timers() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();

        let config = GitTimerConfig {
            vault_name: "test".into(),
            vault_root: dir.path().to_path_buf(),
            config: GitConfig {
                enabled: true,
                auto_commit_every: Some("5m".into()),
                commit_on_inactivity: None,
                auto_pull_every: None,
                auto_push_every: None,
                commit_message: None,
            },
        };

        let handles = start_git_timers(vec![config]).await;
        assert_eq!(handles.len(), 1, "should only spawn commit timer");

        for h in handles {
            h.abort();
        }
    }

    #[tokio::test]
    async fn start_git_timers_spawns_inactivity_timer() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();

        let config = GitTimerConfig {
            vault_name: "test".into(),
            vault_root: dir.path().to_path_buf(),
            config: GitConfig {
                enabled: true,
                auto_commit_every: None,
                commit_on_inactivity: Some("120s".into()),
                auto_pull_every: None,
                auto_push_every: None,
                commit_message: None,
            },
        };

        let handles = start_git_timers(vec![config]).await;
        assert_eq!(handles.len(), 1, "should spawn only the inactivity timer");

        for h in handles {
            h.abort();
        }
    }
}
