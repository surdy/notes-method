use std::path::{Path, PathBuf};

use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub struct LogGuard {
    _file_guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Returns the platform-appropriate log directory.
/// - macOS: ~/Library/Logs/Notesmith/
/// - Linux: $XDG_STATE_HOME/notesmith/ or ~/.local/state/notesmith/
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_log_dir_override().lock().unwrap().clone() {
        return Some(path);
    }

    if cfg!(target_os = "macos") {
        dirs::home_dir().map(|home| home.join("Library/Logs/Notesmith"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".local/state")))
            .map(|dir| dir.join("notesmith"))
    }
}

/// Returns the log file path (e.g., ~/Library/Logs/Notesmith/daemon.log).
pub fn log_file_path() -> Option<PathBuf> {
    log_dir().map(|dir| dir.join("daemon.log"))
}

pub fn init_logging() -> Option<LogGuard> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let console_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_target(true);

    let log_directory = match log_dir() {
        Some(path) => path,
        None => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(console_layer)
                .try_init()
                .ok()?;
            return None;
        }
    };

    std::fs::create_dir_all(&log_directory).ok()?;
    cleanup_old_logs(&log_directory, 7);

    let file_appender = rolling::daily(&log_directory, "daemon.log");
    let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .with_writer(non_blocking_file)
        .with_ansi(false)
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .try_init()
        .ok()?;

    Some(LogGuard {
        _file_guard: file_guard,
    })
}

pub fn cleanup_old_logs(log_directory: &Path, retention_days: u64) {
    let cutoff = chrono::Utc::now().date_naive() - chrono::Duration::days(retention_days as i64);

    let Ok(entries) = std::fs::read_dir(log_directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(date) = rotated_log_date(file_name) else {
            continue;
        };

        if date < cutoff {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn rotated_log_date(file_name: &str) -> Option<chrono::NaiveDate> {
    file_name
        .strip_prefix("daemon.log.")
        .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

pub fn current_log_path() -> Option<PathBuf> {
    let log_directory = log_dir()?;
    let prefix = log_file_path()?;

    let latest_rotated = std::fs::read_dir(&log_directory)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?;
            let date = rotated_log_date(file_name)?;
            Some((date, path))
        })
        .max_by_key(|(date, _)| *date)
        .map(|(_, path)| path);

    latest_rotated.or_else(|| prefix.exists().then_some(prefix))
}

#[cfg(test)]
pub(crate) fn test_log_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(crate) fn set_test_log_dir_override(path: Option<PathBuf>) {
    *test_log_dir_override().lock().unwrap() = path;
}

#[cfg(test)]
fn test_log_dir_override() -> &'static std::sync::Mutex<Option<PathBuf>> {
    use std::sync::{Mutex, OnceLock};

    static OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    OVERRIDE.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn log_dir_uses_expected_platform_suffix() {
        let _guard = test_log_lock().lock().unwrap();
        let log_dir = log_dir().expect("log dir should resolve");
        if cfg!(target_os = "macos") {
            assert!(log_dir.ends_with("Library/Logs/Notesmith"));
        } else {
            assert!(log_dir.ends_with("notesmith"));
        }
    }

    #[test]
    fn log_file_path_points_to_daemon_log() {
        let _guard = test_log_lock().lock().unwrap();
        let log_path = log_file_path().expect("log file path should resolve");
        assert!(log_path.ends_with("daemon.log"));
    }

    #[test]
    fn cleanup_old_logs_removes_only_expired_rotated_files() {
        let _guard = test_log_lock().lock().unwrap();
        let log_dir = test_log_dir();
        fs::create_dir_all(&log_dir).unwrap();

        let today = Utc::now().date_naive();
        let expired = log_dir.join(format!(
            "daemon.log.{}",
            (today - Duration::days(8)).format("%Y-%m-%d")
        ));
        let retained = log_dir.join(format!(
            "daemon.log.{}",
            (today - Duration::days(2)).format("%Y-%m-%d")
        ));
        let active = log_dir.join("daemon.log");

        fs::write(&expired, "expired\n").unwrap();
        fs::write(&retained, "retained\n").unwrap();
        fs::write(&active, "active\n").unwrap();

        cleanup_old_logs(&log_dir, 7);

        assert!(!expired.exists(), "expired rotated log should be removed");
        assert!(retained.exists(), "recent rotated log should be retained");
        assert!(active.exists(), "active log file should be retained");

        fs::remove_dir_all(&log_dir).unwrap();
    }

    fn test_log_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-artifacts")
            .join(format!("logging-{unique}-{}", std::process::id()))
    }
}
