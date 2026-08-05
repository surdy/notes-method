//! Job subprocess execution (issues #280, #282).
//!
//! Mirrors the `notesmith-hooks` subprocess conventions (interpreter dispatch
//! by extension, own process group, kill-on-timeout) with the job-specific
//! contract: cwd = vault root, no JSON on stdin, and the connector env
//! (`NOTESMITH_API_BASE`, `NOTESMITH_VAULT`, `NOTESMITH_STATE_DIR`). Output
//! is captured bounded for logging.
//!
//! Two entry points share one subprocess core: [`run_command_job`] executes a
//! vault-relative connector script, and [`run_agent_job`] shells out to the
//! colocated `notesmith` CLI (`notesmith ai prompt <name>`) — per the ADR
//! 0019/0022/0023 precedent, heavy agent work runs OUTSIDE the daemon
//! process, so the daemon never hosts an in-process ACP session.

use std::path::Path;

use notesmith_config::JobAgentConfig;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Cap on captured stdout/stderr, per stream. Job output is for logs and
/// failure diagnostics, not data transfer; connectors write results through
/// the REST API or files.
const MAX_CAPTURED_OUTPUT: usize = 64 * 1024;

/// Outcome of one subprocess run (however it ended).
#[derive(Debug)]
pub struct JobRunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
}

impl JobRunOutcome {
    pub fn succeeded(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JobRunError {
    #[error("job command not found: {path}")]
    CommandNotFound { path: String },
    #[error("job execution failed: {source}")]
    ExecutionFailed { source: std::io::Error },
}

/// Environment for one job run.
#[derive(Debug, Clone)]
pub struct JobEnv {
    /// Daemon base URL, e.g. `http://127.0.0.1:27183`.
    pub api_base: String,
    pub vault_name: String,
    /// Per-job connector-state dir (created before the run).
    pub state_dir: std::path::PathBuf,
}

/// Run a job command with cwd = `vault_root`, the connector env, and a hard
/// wall-clock `timeout` (the whole process group is killed on expiry).
pub async fn run_command_job(
    vault_root: &Path,
    command_rel: &str,
    timeout: Duration,
    env: &JobEnv,
) -> Result<JobRunOutcome, JobRunError> {
    let command_path = vault_root.join(command_rel);
    if !command_path.exists() {
        return Err(JobRunError::CommandNotFound {
            path: command_path.display().to_string(),
        });
    }

    run_prepared(build_command(&command_path, vault_root, env), timeout).await
}

/// Run an agent-kind job (issue #282): shell out to the colocated `notesmith`
/// CLI's headless `ai prompt` subcommand with the job's prompt name, under
/// the same cwd/env/timeout contract as command jobs. `notesmith_bin` is the
/// daemon's own binary in production and a stub in tests — no LLM runs here.
pub async fn run_agent_job(
    notesmith_bin: &Path,
    vault_root: &Path,
    agent: &JobAgentConfig,
    timeout: Duration,
    env: &JobEnv,
) -> Result<JobRunOutcome, JobRunError> {
    let mut command = Command::new(notesmith_bin);
    command
        .arg("--vault")
        .arg(&env.vault_name)
        .arg("ai")
        .arg("prompt")
        .arg(&agent.prompt);
    if agent.allow_writes {
        command.arg("--allow-writes");
    }
    apply_job_contract(&mut command, vault_root, env);
    run_prepared(command, timeout).await
}

/// Spawn a fully-built job command and see it through: bounded output
/// capture, wall-clock timeout with process-group kill, duration accounting.
async fn run_prepared(
    mut command: Command,
    timeout: Duration,
) -> Result<JobRunOutcome, JobRunError> {
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|source| JobRunError::ExecutionFailed { source })?;

    let stdout_task = read_bounded(child.stdout.take());
    let stderr_task = read_bounded(child.stderr.take());

    let (exit_code, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(wait_result) => {
            let status = wait_result.map_err(|source| JobRunError::ExecutionFailed { source })?;
            (status.code(), false)
        }
        Err(_) => {
            kill_child_processes(&mut child).await;
            (None, true)
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();

    Ok(JobRunOutcome {
        exit_code,
        timed_out,
        duration: started.elapsed(),
        stdout,
        stderr,
    })
}

/// Read a child stream to completion, keeping at most [`MAX_CAPTURED_OUTPUT`]
/// bytes. Runs as a task so slow/large output never deadlocks the wait.
fn read_bounded<R>(stream: Option<R>) -> tokio::task::JoinHandle<String>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(mut stream) = stream else {
            return String::new();
        };
        let mut captured = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = MAX_CAPTURED_OUTPUT.saturating_sub(captured.len());
                    captured.extend_from_slice(&buffer[..n.min(room)]);
                    // Keep draining past the cap so the child never blocks on
                    // a full pipe.
                }
            }
        }
        String::from_utf8_lossy(&captured).into_owned()
    })
}

/// Kill the child's whole process group (the command may have spawned its own
/// children), mirroring `notesmith-hooks`.
async fn kill_child_processes(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{id}")])
            .status();
    }

    let _ = child.start_kill();
    let _ = child.wait().await;
}

/// Build the job command: interpreter dispatch by extension (like hooks), own
/// process group, cwd = vault root, stdin closed (jobs receive no JSON
/// payload — their inputs are env + the vault), stdout/stderr piped for
/// bounded capture.
fn build_command(command_path: &Path, vault_root: &Path, env: &JobEnv) -> Command {
    let ext = command_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    let mut command = match ext {
        "py" => {
            let mut command = Command::new("python3");
            command.arg(command_path);
            command
        }
        "sh" => {
            let mut command = Command::new("sh");
            command.arg(command_path);
            command
        }
        "js" => {
            let mut command = Command::new("node");
            command.arg(command_path);
            command
        }
        _ => Command::new(command_path),
    };

    apply_job_contract(&mut command, vault_root, env);
    command
}

/// The execution contract shared by every job kind: own process group,
/// cwd = vault root, the connector env, stdin closed (jobs receive no JSON
/// payload — their inputs are env + the vault), stdout/stderr piped for
/// bounded capture.
fn apply_job_contract(command: &mut Command, vault_root: &Path, env: &JobEnv) {
    #[cfg(unix)]
    command.process_group(0);

    command
        .current_dir(vault_root)
        // Never let an inherited NOTESMITH_URL point a connector's CLI calls
        // at a remote daemon; NOTESMITH_API_BASE is the job contract.
        .env_remove("NOTESMITH_URL")
        .env("NOTESMITH_API_BASE", &env.api_base)
        .env("NOTESMITH_VAULT", &env.vault_name)
        .env("NOTESMITH_STATE_DIR", &env.state_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_executable(path: &Path, content: &str) {
        std::fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).unwrap();
        }
    }

    fn env_for(dir: &TempDir) -> JobEnv {
        JobEnv {
            api_base: "http://127.0.0.1:27183".to_string(),
            vault_name: "work".to_string(),
            state_dir: dir.path().join("state"),
        }
    }

    #[tokio::test]
    async fn runs_with_vault_cwd_and_connector_env() {
        let vault = TempDir::new().unwrap();
        write_executable(
            &vault.path().join("job.sh"),
            "#!/bin/sh\npwd\necho \"$NOTESMITH_API_BASE|$NOTESMITH_VAULT|$NOTESMITH_STATE_DIR\"\n",
        );

        let env = env_for(&vault);
        let outcome = run_command_job(vault.path(), "job.sh", Duration::from_secs(10), &env)
            .await
            .unwrap();

        assert!(outcome.succeeded());
        assert_eq!(outcome.exit_code, Some(0));
        let canonical_vault = vault.path().canonicalize().unwrap();
        let mut lines = outcome.stdout.lines();
        let cwd = std::path::PathBuf::from(lines.next().unwrap());
        assert_eq!(cwd.canonicalize().unwrap(), canonical_vault);
        assert_eq!(
            lines.next().unwrap(),
            format!("http://127.0.0.1:27183|work|{}", env.state_dir.display())
        );
    }

    #[tokio::test]
    async fn nonzero_exit_is_captured_not_an_error() {
        let vault = TempDir::new().unwrap();
        write_executable(
            &vault.path().join("fail.sh"),
            "#!/bin/sh\necho boom >&2\nexit 3\n",
        );

        let outcome = run_command_job(
            vault.path(),
            "fail.sh",
            Duration::from_secs(10),
            &env_for(&vault),
        )
        .await
        .unwrap();

        assert!(!outcome.succeeded());
        assert_eq!(outcome.exit_code, Some(3));
        assert!(outcome.stderr.contains("boom"));
    }

    #[tokio::test]
    async fn timeout_kills_the_subprocess() {
        let vault = TempDir::new().unwrap();
        write_executable(&vault.path().join("sleep.sh"), "#!/bin/sh\nsleep 60\n");

        let started = Instant::now();
        let outcome = run_command_job(
            vault.path(),
            "sleep.sh",
            Duration::from_millis(300),
            &env_for(&vault),
        )
        .await
        .unwrap();

        assert!(outcome.timed_out);
        assert!(!outcome.succeeded());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn agent_job_invokes_the_cli_with_prompt_and_write_flag() {
        let vault = TempDir::new().unwrap();
        let bin_dir = TempDir::new().unwrap();
        let stub = bin_dir.path().join("notesmith-stub");
        write_executable(
            &stub,
            "#!/bin/sh\necho \"$@\"\necho \"$NOTESMITH_VAULT\"\npwd\n",
        );

        let env = env_for(&vault);
        let agent = JobAgentConfig {
            prompt: "daily-note".to_string(),
            allow_writes: true,
        };
        let outcome = run_agent_job(&stub, vault.path(), &agent, Duration::from_secs(10), &env)
            .await
            .unwrap();

        assert!(outcome.succeeded(), "stderr: {}", outcome.stderr);
        let mut lines = outcome.stdout.lines();
        assert_eq!(
            lines.next().unwrap(),
            "--vault work ai prompt daily-note --allow-writes"
        );
        assert_eq!(lines.next().unwrap(), "work");
        let cwd = std::path::PathBuf::from(lines.next().unwrap());
        assert_eq!(
            cwd.canonicalize().unwrap(),
            vault.path().canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn agent_job_defaults_to_read_only_and_reports_failure() {
        let vault = TempDir::new().unwrap();
        let bin_dir = TempDir::new().unwrap();
        let stub = bin_dir.path().join("notesmith-stub");
        write_executable(
            &stub,
            "#!/bin/sh\necho \"$@\"\necho agent boom >&2\nexit 5\n",
        );

        let agent = JobAgentConfig {
            prompt: "daily-note".to_string(),
            allow_writes: false,
        };
        let outcome = run_agent_job(
            &stub,
            vault.path(),
            &agent,
            Duration::from_secs(10),
            &env_for(&vault),
        )
        .await
        .unwrap();

        assert!(!outcome.succeeded());
        assert_eq!(outcome.exit_code, Some(5));
        assert!(!outcome.stdout.contains("--allow-writes"));
        assert!(outcome.stderr.contains("agent boom"));
    }

    #[tokio::test]
    async fn agent_job_with_missing_binary_is_a_clear_error() {
        let vault = TempDir::new().unwrap();
        let agent = JobAgentConfig {
            prompt: "daily-note".to_string(),
            allow_writes: false,
        };
        let error = run_agent_job(
            Path::new("/nonexistent/notesmith"),
            vault.path(),
            &agent,
            Duration::from_secs(1),
            &env_for(&vault),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, JobRunError::ExecutionFailed { .. }));
    }

    #[tokio::test]
    async fn missing_command_is_a_clear_error() {
        let vault = TempDir::new().unwrap();
        let error = run_command_job(
            vault.path(),
            "nope.sh",
            Duration::from_secs(1),
            &env_for(&vault),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, JobRunError::CommandNotFound { .. }));
    }

    #[tokio::test]
    async fn captured_output_is_bounded() {
        let vault = TempDir::new().unwrap();
        // ~1MB of output; capture must stop at the cap without hanging.
        write_executable(
            &vault.path().join("noisy.sh"),
            "#!/bin/sh\ni=0\nwhile [ $i -lt 16384 ]; do echo 'sixty-four bytes of filler text to overflow the capture cap!!'; i=$((i+1)); done\n",
        );

        let outcome = run_command_job(
            vault.path(),
            "noisy.sh",
            Duration::from_secs(30),
            &env_for(&vault),
        )
        .await
        .unwrap();

        assert!(outcome.succeeded());
        assert!(outcome.stdout.len() <= MAX_CAPTURED_OUTPUT);
    }
}
