//! Resolve the real `PATH` for detecting and spawning external agent CLIs.
//!
//! macOS GUI launches (Finder/Dock) inherit a minimal `launchd` `PATH`
//! (`/usr/bin:/bin:…`) that excludes Homebrew, nvm, asdf, volta, bun, and
//! `~/.cargo/bin`. A bundled `.app` therefore cannot find `copilot`, `npx`, or
//! `codex-acp` even when they are installed and on the user's shell `PATH`
//! (ADR 0013). This module computes an augmented `PATH` by merging the current
//! `PATH`, the user's login-shell `PATH` (best-effort), and a curated set of
//! common tool directories, then applies it to the process so detection
//! (`binary_on_path`) and agent spawning never disagree.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long to wait for the login shell to report its `PATH` before giving up
/// and falling back to the curated directory set.
const SHELL_QUERY_TIMEOUT: Duration = Duration::from_millis(2500);

/// Home-relative directories that commonly hold user-installed CLIs but are
/// missing from a macOS GUI `launchd` `PATH`.
const HOME_RELATIVE_DIRS: &[&str] = &[
    ".cargo/bin",
    ".local/bin",
    ".bun/bin",
    ".deno/bin",
    ".volta/bin",
    ".npm-global/bin",
    ".asdf/shims",
    ".asdf/bin",
];

/// Absolute directories that commonly hold user-installed CLIs.
const ABSOLUTE_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/bin",
    "/bin",
];

/// The curated directories to graft onto `PATH`, given the user's `home` dir.
fn curated_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = ABSOLUTE_DIRS.iter().map(PathBuf::from).collect();
    for rel in HOME_RELATIVE_DIRS {
        dirs.push(home.join(rel));
    }
    dirs
}

/// Merge several `PATH`-like strings plus extra directories into one `PATH`
/// value, preserving first-seen order and dropping duplicate and empty
/// segments. Pure and order-deterministic so detection is reproducible.
pub fn merge_paths(sources: &[&str], extra: &[PathBuf]) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for source in sources {
        for segment in source.split(':') {
            if !segment.is_empty() && seen.insert(segment.to_string()) {
                out.push(segment.to_string());
            }
        }
    }
    for dir in extra {
        let segment = dir.to_string_lossy().to_string();
        if !segment.is_empty() && seen.insert(segment.clone()) {
            out.push(segment);
        }
    }
    out.join(":")
}

/// The user's home directory, from `HOME` (Unix) with a `~` fallback that is a
/// no-op join target. Never panics.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Ask the user's login shell for its `PATH`, sourcing login + interactive
/// config (where Homebrew `shellenv`, nvm, asdf, etc. extend `PATH`).
///
/// Best-effort: returns `None` if `SHELL` is unset, the shell cannot run the
/// probe, it exits non-zero, it produces no output, or it does not respond
/// within `timeout`. A non-POSIX shell that rejects the flags simply yields
/// `None`, and the caller falls back to the curated set.
pub fn query_login_shell_path(timeout: Duration) -> Option<String> {
    let shell = std::env::var_os("SHELL")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let output = std::process::Command::new(&shell)
            .args(["-lic", "printf %s \"$PATH\""])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();
        let _ = tx.send(output);
    });

    let output = rx.recv_timeout(timeout).ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

/// Compute the augmented `PATH`: login-shell `PATH` first (the user's intended
/// ordering), then the inherited process `PATH`, then the curated directories
/// as a safety net. Always non-empty (the curated set is unconditional).
pub fn resolve_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let shell = query_login_shell_path(SHELL_QUERY_TIMEOUT).unwrap_or_default();
    let extra = curated_dirs(&home_dir());
    merge_paths(&[shell.as_str(), current.as_str()], &extra)
}

/// Resolve and apply the augmented `PATH` to this process. Call once, early in
/// `main`, before any thread spawns an agent — child processes then inherit the
/// resolved `PATH` and `binary_on_path` (which reads the process `PATH`) agrees
/// with what is actually launchable.
pub fn apply_resolved_path() {
    let resolved = resolve_path();
    // SAFETY: invoked once at startup, before agent-spawning threads exist, so
    // there is no concurrent reader/writer of the environment.
    unsafe {
        std::env::set_var("PATH", resolved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_first_seen_order_and_dedupes() {
        let merged = merge_paths(&["/a:/b:/a", "/b:/c"], &[]);
        assert_eq!(merged, "/a:/b:/c");
    }

    #[test]
    fn merge_drops_empty_segments() {
        let merged = merge_paths(&["", "/a::/b:", ""], &[]);
        assert_eq!(merged, "/a:/b");
    }

    #[test]
    fn merge_appends_extra_dirs_deduped_against_sources() {
        let merged = merge_paths(
            &["/usr/bin"],
            &[
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
            ],
        );
        assert_eq!(merged, "/usr/bin:/opt/homebrew/bin");
    }

    #[test]
    fn merge_orders_sources_before_extra() {
        let merged = merge_paths(
            &["/shell/bin", "/proc/bin"],
            &[PathBuf::from("/curated/bin")],
        );
        assert_eq!(merged, "/shell/bin:/proc/bin:/curated/bin");
    }

    #[test]
    fn curated_dirs_include_homebrew_and_home_tools() {
        let dirs = curated_dirs(Path::new("/home/u"));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.cargo/bin")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.local/bin")));
    }

    #[test]
    fn resolve_path_always_includes_curated_homebrew() {
        // The curated set is unconditional, so this holds even when the login
        // shell probe fails or the inherited PATH is minimal.
        let resolved = resolve_path();
        assert!(!resolved.is_empty());
        assert!(
            resolved.split(':').any(|p| p == "/opt/homebrew/bin"),
            "resolved PATH missing curated Homebrew dir: {resolved}"
        );
    }
}
