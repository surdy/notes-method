use crate::ConfigError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    io,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonLockfile {
    pub pid: u32,
    pub port: u16,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub binary_path: PathBuf,
}

impl DaemonLockfile {
    pub fn path() -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            dirs::data_dir().map(|dir| dir.join("Notesmith").join("daemon.lock"))
        } else if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            Some(
                PathBuf::from(runtime_dir)
                    .join("notesmith")
                    .join("daemon.lock"),
            )
        } else {
            Some(PathBuf::from(format!(
                "/tmp/notesmith-{}/daemon.lock",
                current_uid()
            )))
        }
    }

    pub fn write(&self) -> Result<(), ConfigError> {
        let path = Self::path().ok_or(ConfigError::NoDataDir)?;
        self.write_to_path(&path)
    }

    pub fn read() -> Result<Option<Self>, ConfigError> {
        let Some(path) = Self::path() else {
            return Ok(None);
        };

        Self::read_from_path(&path)
    }

    pub fn remove() -> Result<(), ConfigError> {
        let Some(path) = Self::path() else {
            return Ok(());
        };

        Self::remove_at_path(&path)
    }

    pub fn is_stale(&self) -> bool {
        if self.pid == 0 || self.pid > i32::MAX as u32 {
            return true;
        }

        #[cfg(unix)]
        {
            // SAFETY: `kill(pid, 0)` checks whether the process exists without sending a signal.
            let result = unsafe { libc::kill(self.pid as i32, 0) };
            if result == 0 {
                return false;
            }

            matches!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH))
        }

        #[cfg(not(unix))]
        {
            false
        }
    }

    pub fn read_active() -> Result<Option<Self>, ConfigError> {
        let Some(path) = Self::path() else {
            return Ok(None);
        };

        Self::read_active_from_path(&path)
    }

    fn write_to_path(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            })?;
        }

        let json =
            serde_json::to_string_pretty(self).map_err(|error| ConfigError::SerializeError {
                message: error.to_string(),
            })?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|source| ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            })?;
        file.write_all(format!("{json}\n").as_bytes())
            .map_err(|source| ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            })
    }

    fn read_from_path(path: &Path) -> Result<Option<Self>, ConfigError> {
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadError {
            path: path.to_path_buf(),
            source,
        })?;

        serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| ConfigError::ParseError {
                path: path.to_path_buf(),
                message: error.to_string(),
            })
    }

    fn remove_at_path(path: &Path) -> Result<(), ConfigError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ConfigError::WriteError {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    fn read_active_from_path(path: &Path) -> Result<Option<Self>, ConfigError> {
        let Some(lockfile) = Self::read_from_path(path)? else {
            return Ok(None);
        };

        if lockfile.is_stale() {
            Self::remove_at_path(path)?;
            return Ok(None);
        }

        Ok(Some(lockfile))
    }
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: `geteuid` reads the effective user id for the current process.
    unsafe { libc::geteuid() as u32 }
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::DaemonLockfile;
    use crate::ConfigError;
    use chrono::Utc;
    use std::{io, path::PathBuf};
    use tempfile::TempDir;

    fn sample_lockfile(pid: u32) -> DaemonLockfile {
        DaemonLockfile {
            pid,
            port: 27183,
            version: "0.1.0".to_string(),
            started_at: Utc::now(),
            binary_path: PathBuf::from("/usr/local/bin/notesmith"),
        }
    }

    #[test]
    fn write_and_read_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("daemon.lock");
        let expected = sample_lockfile(std::process::id());

        expected.write_to_path(&path).unwrap();
        let actual = DaemonLockfile::read_from_path(&path).unwrap().unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn stale_detection_returns_true_for_missing_pid() {
        let lockfile = sample_lockfile(u32::MAX);
        assert!(lockfile.is_stale());
    }

    #[test]
    fn read_active_returns_none_for_stale_lockfile() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("daemon.lock");
        sample_lockfile(u32::MAX).write_to_path(&path).unwrap();

        let actual = DaemonLockfile::read_active_from_path(&path).unwrap();

        assert!(actual.is_none());
        assert!(!path.exists());
    }

    #[test]
    fn read_active_returns_some_for_current_pid() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("daemon.lock");
        let expected = sample_lockfile(std::process::id());
        expected.write_to_path(&path).unwrap();

        let actual = DaemonLockfile::read_active_from_path(&path).unwrap();

        assert_eq!(actual, Some(expected));
        assert!(path.exists());
    }

    #[test]
    fn remove_cleans_up_lockfile() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("daemon.lock");
        sample_lockfile(std::process::id())
            .write_to_path(&path)
            .unwrap();

        DaemonLockfile::remove_at_path(&path).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn write_does_not_overwrite_existing_lockfile() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("daemon.lock");
        sample_lockfile(std::process::id())
            .write_to_path(&path)
            .unwrap();

        let error = sample_lockfile(std::process::id())
            .write_to_path(&path)
            .unwrap_err();

        match error {
            ConfigError::WriteError { source, .. } => {
                assert_eq!(source.kind(), io::ErrorKind::AlreadyExists);
            }
            other => panic!("expected write error, got {other:?}"),
        }
    }
}
