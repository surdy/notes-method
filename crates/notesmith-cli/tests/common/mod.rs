//! Shared helpers for the daemon-spawning CLI integration tests.
//!
//! ## The flake these fix
//!
//! Tests picked a port by binding `127.0.0.1:0`, reading the assigned port, and
//! *dropping* the listener before spawning the daemon:
//!
//! ```ignore
//! let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
//! let bind = reserved.local_addr().unwrap();
//! drop(reserved);            // port is free again — and up for grabs
//! ```
//!
//! Between that drop and the daemon binding, anything can take the port —
//! including a sibling test doing the same dance, since cargo runs these in
//! parallel. The daemon then fails to bind and exits, and the test panics with
//! `daemon exited early`. That is the CI flake.
//!
//! ## Why not just bind `:0` and read the port back
//!
//! The daemon does publish the port it actually bound (`write_daemon_lockfile`
//! records `listener.local_addr()`), so `bind = "127.0.0.1:0"` starts fine. But
//! the *client* side resolves the daemon from `config.daemon.bind`
//! (`daemon_client::daemon_url`), so with port 0 every subsequent CLI command
//! looks for `127.0.0.1:0` and never finds it. Teaching the client to prefer the
//! lockfile is a real behaviour change for every command — worth doing, too big
//! to smuggle into a test fix.
//!
//! So the window stays, and [`spawn_daemon_retrying`] closes it the honest way:
//! if the daemon loses the race and exits, pick another port and try again. The
//! retry is observable — it prints — so a genuine startup failure still surfaces
//! rather than being silently papered over.

#![allow(dead_code)] // each integration test binary uses a different subset

use std::{
    path::{Path, PathBuf},
    process::Child,
    time::Duration,
};

use notesmith_config::DaemonLockfile;

/// Debug builds can take seconds to start (vault scan, index build, watchers).
pub const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Port collisions are rare and independent, so a couple of retries converges.
const SPAWN_ATTEMPTS: usize = 4;

/// An ephemeral port, free at the moment it is returned.
///
/// Inherently advisory: the port is unbound before the caller can use it. Pair
/// with [`spawn_daemon_retrying`], which recovers when someone else takes it.
pub fn free_port() -> String {
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);
    bind.to_string()
}

/// Spawn a daemon, retrying on a different port if it loses the bind race.
///
/// `spawn` receives a `host:port` string, is expected to write whatever config
/// the daemon and subsequent CLI calls need, and to return the spawned child.
/// Returns the live child and the bind address it is serving on.
pub async fn spawn_daemon_retrying<F>(mut spawn: F) -> (Child, String)
where
    F: FnMut(&str) -> Child,
{
    let mut last_failure = String::new();

    for attempt in 1..=SPAWN_ATTEMPTS {
        let bind = free_port();
        let mut child = spawn(&bind);

        match wait_until_serving(&mut child, &bind).await {
            Ok(()) => return (child, bind),
            Err(reason) => {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!(
                    "daemon spawn attempt {attempt}/{SPAWN_ATTEMPTS} on {bind} failed: {reason}"
                );
                last_failure = reason;
            }
        }
    }

    panic!("daemon failed to start after {SPAWN_ATTEMPTS} attempts; last failure: {last_failure}");
}

/// Retry a whole attempt on a fresh port until it succeeds.
///
/// For flows where the daemon is started *by the CLI* rather than by the test
/// (e.g. a command that auto-starts one), so there is no child handle to watch —
/// the attempt just reports whether it worked. Returns the attempt's value and
/// the port it succeeded on.
pub fn retrying_on_free_port<T, F>(mut attempt: F) -> (T, String)
where
    F: FnMut(&str) -> Result<T, String>,
{
    let mut last_failure = String::new();

    for index in 1..=SPAWN_ATTEMPTS {
        let bind = free_port();
        match attempt(&bind) {
            Ok(value) => return (value, bind),
            Err(reason) => {
                eprintln!("attempt {index}/{SPAWN_ATTEMPTS} on {bind} failed: {reason}");
                last_failure = reason;
            }
        }
    }

    panic!("no attempt succeeded after {SPAWN_ATTEMPTS} tries; last failure: {last_failure}");
}

/// Wait for the daemon to answer `/ping`, or explain why it never did.
async fn wait_until_serving(child: &mut Child, bind: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + DAEMON_READY_TIMEOUT;

    loop {
        match child.try_wait() {
            // The overwhelmingly likely cause is a lost bind race; retrying on a
            // fresh port sorts it out.
            Ok(Some(status)) => return Err(format!("daemon exited early with {status}")),
            Ok(None) => {}
            Err(error) => return Err(format!("failed to check daemon status: {error}")),
        }

        let last_error = match client.get(format!("http://{bind}/ping")).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => format!("unexpected status {}", response.status()),
            Err(error) => error.to_string(),
        };

        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "did not answer /ping within {DAEMON_READY_TIMEOUT:?}: {last_error}"
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Mirror of `DaemonLockfile::path()`, which reads the *daemon's* environment —
/// the test process has its own, so the path is recomputed from the values the
/// child was spawned with.
pub fn lockfile_path(home: &Path, runtime_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Notesmith")
            .join("daemon.lock")
    } else {
        runtime_dir.join("notesmith").join("daemon.lock")
    }
}

/// The port the daemon actually bound, once it has published it.
pub fn read_daemon_port(lockfile: &Path) -> Option<u16> {
    let contents = std::fs::read_to_string(lockfile).ok()?;
    let lockfile: DaemonLockfile = serde_json::from_str(&contents).ok()?;
    Some(lockfile.port)
}
