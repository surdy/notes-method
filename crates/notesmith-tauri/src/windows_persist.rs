//! Persistence for the set of open vault windows.
//!
//! On every meaningful change (a window opens, moves, resizes, or closes) we
//! atomically rewrite `windows.json` in the app config directory. On launch we
//! replay the file to re-open each vault window at its saved geometry.
//!
//! All filesystem I/O and the pure geometry helpers (`clamp_to_monitor`,
//! `dedupe_latest_per_vault`) live here so they can be unit tested without a
//! running Tauri runtime.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Current schema version. Bump and add a migration when fields change.
///
/// v1 → v2 (ADR 0017): each window records the `server_id` it is bound to.
/// v1 files have no `server_id`; they load and migrate to the default
/// connection at restore.
pub const SCHEMA_VERSION: u32 = 2;

/// On-disk representation of the open-window set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsFile {
    pub version: u32,
    #[serde(default)]
    pub windows: Vec<WindowEntry>,
}

impl Default for WindowsFile {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            windows: Vec::new(),
        }
    }
}

/// One persisted vault window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowEntry {
    pub vault: String,
    /// The server this window is bound to (ADR 0017). Absent in legacy v1 files;
    /// `None` triggers migration to the default connection at restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Rectangle in screen coordinates. Used for monitor-bounds clamping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Return the path to `windows.json` inside the given config directory.
pub fn windows_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join("windows.json")
}

/// Load and parse `windows.json`. Missing, empty, or corrupt files return
/// `Ok(None)` so the caller can fall back to first-launch flow.
pub fn load(path: &Path) -> io::Result<Option<WindowsFile>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    match serde_json::from_slice::<WindowsFile>(&bytes) {
        // Accept any known schema version (v1 legacy files have no per-window
        // `server_id`; serde fills `None`, which restore migrates to default).
        Ok(file) if (1..=SCHEMA_VERSION).contains(&file.version) => Ok(Some(file)),
        _ => Ok(None),
    }
}

/// Atomically write `windows.json` by serialising to a sibling `.tmp` file
/// and renaming. The parent directory must already exist.
pub fn save(path: &Path, file: &WindowsFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = match path.extension() {
        Some(ext) => {
            let mut new_ext = ext.to_os_string();
            new_ext.push(".tmp");
            path.with_extension(new_ext)
        }
        None => path.with_extension("tmp"),
    };
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Collapse multiple entries for the same vault, keeping the last one in
/// document order. The on-disk invariant is "at most one entry per vault".
pub fn dedupe_latest_per_vault(entries: Vec<WindowEntry>) -> Vec<WindowEntry> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<WindowEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(&index) = seen.get(&entry.vault) {
            result[index] = entry;
        } else {
            seen.insert(entry.vault.clone(), result.len());
            result.push(entry);
        }
    }
    result
}

/// Clamp `rect` so that it intersects one of the given `monitors`. If the
/// rect is fully off-screen (no overlap with any monitor), centre it inside
/// the first monitor. Returns the (possibly adjusted) rectangle.
pub fn clamp_to_monitor(rect: Rect, monitors: &[Rect]) -> Rect {
    if monitors.is_empty() {
        return rect;
    }

    if monitors.iter().any(|m| overlaps(rect, *m)) {
        return rect;
    }

    let primary = monitors[0];
    let w = rect.w.min(primary.w);
    let h = rect.h.min(primary.h);
    let x = primary.x + ((primary.w.saturating_sub(w)) / 2) as i32;
    let y = primary.y + ((primary.h.saturating_sub(h)) / 2) as i32;
    Rect { x, y, w, h }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let a_right = a.x.saturating_add(a.w as i32);
    let a_bottom = a.y.saturating_add(a.h as i32);
    let b_right = b.x.saturating_add(b.w as i32);
    let b_bottom = b.y.saturating_add(b.h as i32);
    a.x < b_right && a_right > b.x && a.y < b_bottom && a_bottom > b.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ns-windows-persist-{nanos}-{counter}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_file() -> WindowsFile {
        WindowsFile {
            version: SCHEMA_VERSION,
            windows: vec![
                WindowEntry {
                    vault: "personal".into(),
                    server_id: Some("local".into()),
                    x: 100,
                    y: 80,
                    w: 1200,
                    h: 800,
                },
                WindowEntry {
                    vault: "work".into(),
                    server_id: Some("home-server".into()),
                    x: 200,
                    y: 120,
                    w: 1100,
                    h: 750,
                },
            ],
        }
    }

    #[test]
    fn round_trip_serialises_and_deserialises() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        let file = sample_file();

        save(&path, &file).unwrap();
        let loaded = load(&path).unwrap().expect("file should exist");

        assert_eq!(loaded, file);
    }

    #[test]
    fn load_returns_none_when_file_missing() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        assert!(load(&path).unwrap().is_none());
    }

    #[test]
    fn load_returns_none_when_file_empty() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        fs::write(&path, b"").unwrap();
        assert!(load(&path).unwrap().is_none());
    }

    #[test]
    fn load_returns_none_when_file_corrupt() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        fs::write(&path, b"{ this is not json").unwrap();
        assert!(load(&path).unwrap().is_none());
    }

    #[test]
    fn load_returns_none_for_unknown_schema_version() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        fs::write(&path, br#"{"version":999,"windows":[]}"#).unwrap();
        assert!(load(&path).unwrap().is_none());
    }

    #[test]
    fn save_writes_through_tmp_file_then_renames() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        save(&path, &sample_file()).unwrap();

        // tmp file must be gone after a successful rename
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "leftover tmp file: {tmp:?}");
        assert!(path.exists());
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tmp_dir().join("nested").join("config");
        let path = windows_file_path(&dir);
        save(&path, &sample_file()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn save_overwrites_existing_file_atomically() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        save(&path, &sample_file()).unwrap();

        let mut updated = sample_file();
        updated.windows[0].x = 999;
        save(&path, &updated).unwrap();

        let loaded = load(&path).unwrap().unwrap();
        assert_eq!(loaded.windows[0].x, 999);
    }

    #[test]
    fn dedupe_keeps_latest_entry_per_vault() {
        let entries = vec![
            WindowEntry {
                vault: "a".into(),
                server_id: Some("local".into()),
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            WindowEntry {
                vault: "b".into(),
                server_id: Some("local".into()),
                x: 1,
                y: 1,
                w: 100,
                h: 100,
            },
            WindowEntry {
                vault: "a".into(),
                server_id: Some("local".into()),
                x: 50,
                y: 50,
                w: 200,
                h: 200,
            },
        ];

        let result = dedupe_latest_per_vault(entries);

        assert_eq!(result.len(), 2);
        let a = result.iter().find(|e| e.vault == "a").unwrap();
        assert_eq!(a.x, 50);
        assert_eq!(a.w, 200);
    }

    #[test]
    fn server_id_round_trips_through_save_load() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        let file = sample_file();
        save(&path, &file).unwrap();
        let loaded = load(&path).unwrap().expect("file should exist");
        assert_eq!(loaded.windows[0].server_id.as_deref(), Some("local"));
        assert_eq!(loaded.windows[1].server_id.as_deref(), Some("home-server"));
    }

    #[test]
    fn legacy_v1_file_without_server_id_loads_with_none() {
        let dir = tmp_dir();
        let path = windows_file_path(&dir);
        // A v1 file as written before per-window connections: no `server_id`.
        fs::write(
            &path,
            br#"{"version":1,"windows":[{"vault":"personal","x":10,"y":20,"w":800,"h":600}]}"#,
        )
        .unwrap();

        let loaded = load(&path).unwrap().expect("legacy file should load");
        assert_eq!(loaded.windows.len(), 1);
        assert_eq!(loaded.windows[0].vault, "personal");
        // No server → restore migrates this to the default connection.
        assert_eq!(loaded.windows[0].server_id, None);
    }

    #[test]
    fn clamp_returns_rect_unchanged_when_it_overlaps_a_monitor() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let rect = Rect {
            x: 100,
            y: 100,
            w: 800,
            h: 600,
        };
        assert_eq!(clamp_to_monitor(rect, &[monitor]), rect);
    }

    #[test]
    fn clamp_recentres_when_rect_is_fully_off_screen() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        // Rect way off to the right
        let rect = Rect {
            x: 5000,
            y: 5000,
            w: 800,
            h: 600,
        };
        let clamped = clamp_to_monitor(rect, &[monitor]);
        assert_eq!(clamped.w, 800);
        assert_eq!(clamped.h, 600);
        // Should be centred: (1920-800)/2 = 560, (1080-600)/2 = 240
        assert_eq!(clamped.x, 560);
        assert_eq!(clamped.y, 240);
    }

    #[test]
    fn clamp_keeps_rect_when_overlapping_secondary_monitor() {
        let primary = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let secondary = Rect {
            x: 1920,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let rect = Rect {
            x: 2000,
            y: 100,
            w: 800,
            h: 600,
        };
        assert_eq!(clamp_to_monitor(rect, &[primary, secondary]), rect);
    }

    #[test]
    fn clamp_shrinks_rect_to_fit_when_recentring_a_huge_window() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 1000,
        };
        let rect = Rect {
            x: 5000,
            y: 5000,
            w: 2000,
            h: 2000,
        };
        let clamped = clamp_to_monitor(rect, &[monitor]);
        assert_eq!(clamped.w, 1000);
        assert_eq!(clamped.h, 1000);
        assert_eq!(clamped.x, 0);
        assert_eq!(clamped.y, 0);
    }

    #[test]
    fn clamp_returns_rect_unchanged_when_no_monitors_known() {
        let rect = Rect {
            x: 10,
            y: 10,
            w: 100,
            h: 100,
        };
        assert_eq!(clamp_to_monitor(rect, &[]), rect);
    }
}
