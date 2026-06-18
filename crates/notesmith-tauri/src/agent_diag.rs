//! On-demand, structured agent-discovery diagnostics (ADR 0013, decisions 3 & 5).
//!
//! The fast availability path ([`crate::agent_bridge::agent_list`]) is a cheap
//! PATH-existence test that never spawns a process, so the picker populates
//! instantly. This module is the **deep, on-demand** counterpart: it walks the
//! declarative registry and, for every launch candidate, records a step-by-step
//! trace explaining *what* was found and *why* — the resolved PATH entries, the
//! directories searched, the absolute program resolved (if any), and — for a
//! found candidate with a configured probe — a **bounded** version-probe of the
//! CLI (exit code + a capped stdout snippet, with a hard timeout).
//!
//! ## Verdict semantics
//!
//! Each agent gets a single `verdict`, derived from its first launch candidate
//! whose `program` resolves on the PATH:
//! - `"available"` — a candidate resolved and either has no probe or its probe
//!   succeeded (exit code `0`, no timeout). The binary is present and launchable.
//! - `"probe_failed"` — a candidate resolved but its probe timed out or exited
//!   non-zero. The binary exists (so it is still launchable), but the probe could
//!   not confirm it; this is informational, not a hard failure.
//! - `"package_missing"` — the launcher resolved on the PATH, but the agent runs
//!   its adapter through a package runner (e.g. `npx <pkg>`) and that package is
//!   not installed locally. The launcher alone cannot run the agent (issue #241).
//! - `"not_found"` — no candidate's `program` resolved on the PATH.
//!
//! ## Resilience (ADR 0009)
//!
//! Note content is not involved here, but the same no-panic discipline applies
//! to process and filesystem data: spawning, waiting, and decoding output never
//! `unwrap`/`expect`. Probe time is bounded by [`PROBE_TIMEOUT`] and probe output
//! by [`PROBE_SNIPPET_CAP`]; a hung or chatty CLI can neither block nor flood.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use notesmith_agent::{AgentDescriptor, builtin_registry};

/// Hard wall-clock bound on a single version probe before it is abandoned.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Maximum number of stdout bytes retained from a probe (the rest is dropped so
/// a chatty CLI can never flood the report).
const PROBE_SNIPPET_CAP: usize = 500;

/// The full diagnostics trace returned to the Settings "Run diagnostics" UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    /// The PATH entries (in order) used for resolution and spawning.
    pub resolved_path: Vec<String>,
    /// One entry per registry agent, in picker order.
    pub agents: Vec<AgentDiagnostic>,
}

/// Per-agent discovery trace: the candidates examined plus the final verdict.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiagnostic {
    /// Stable agent id (e.g. `"copilot"`).
    pub id: String,
    /// Human-readable name shown in the picker.
    pub display_name: String,
    /// `"available"` | `"not_found"` | `"probe_failed"` (see module docs).
    pub verdict: String,
    /// One entry per launch candidate, in registry order.
    pub candidates: Vec<CandidateDiagnostic>,
    /// Actionable setup guidance from the registry.
    pub setup_hint: String,
    /// Documentation URL for installing / configuring the agent.
    pub docs_url: String,
    /// The agent CLI version parsed from the version probe, normalized to
    /// `"major.minor.patch"`, or `None` when no version could be detected
    /// (issue #192).
    pub detected_version: Option<String>,
    /// A warning surfaced when the detected version is strictly below the
    /// registry's `min_version` floor, or `None` when up to date / unknown /
    /// no floor is declared (issue #192).
    pub version_warning: Option<String>,
}

/// Per-candidate trace: where it was looked for, what resolved, and any probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDiagnostic {
    /// The program looked up on PATH (or an absolute path).
    pub program: String,
    /// Base launch arguments for this candidate.
    pub args: Vec<String>,
    /// Absolute path resolved on the PATH, if the program was found.
    pub resolved_program: Option<String>,
    /// Whether the program resolved on the PATH.
    pub found_on_path: bool,
    /// The directories checked (only populated for bare program names).
    pub searched_dirs: Vec<String>,
    /// The version probe, present only when the program was found and the
    /// candidate declares `probe_args`.
    pub probe: Option<ProbeResult>,
}

/// The bounded result of spawning a candidate's version probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// The command line that was run (e.g. `"/opt/homebrew/bin/gemini --version"`).
    pub command: String,
    /// Process exit code, or `None` if it timed out or was signal-terminated.
    pub exit_code: Option<i32>,
    /// First [`PROBE_SNIPPET_CAP`] bytes of stdout, lossily decoded and trimmed.
    pub stdout_snippet: String,
    /// Whether the probe exceeded [`PROBE_TIMEOUT`] and was abandoned.
    pub timed_out: bool,
}

/// The current process PATH split into ordered directory entries.
fn current_path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default()
}

/// Resolve `program` against `path_dirs`, returning the absolute path found (if
/// any) and the directories actually searched.
///
/// A `program` containing `/` is treated as an explicit (absolute or relative)
/// path and checked directly, with no directory search. A bare program name is
/// looked up in each dir in order; `searched_dirs` lists the dirs checked up to
/// and including the match (or all of them when nothing is found).
fn resolve_on_path(program: &str, path_dirs: &[PathBuf]) -> (Option<PathBuf>, Vec<PathBuf>) {
    if program.contains('/') {
        let candidate = PathBuf::from(program);
        return if candidate.exists() {
            (Some(candidate), Vec::new())
        } else {
            (None, Vec::new())
        };
    }
    let mut searched = Vec::new();
    for dir in path_dirs {
        searched.push(dir.clone());
        let candidate = dir.join(program);
        if candidate.exists() {
            return (Some(candidate), searched);
        }
    }
    (None, searched)
}

/// Decode and bound a probe's stdout: at most `cap` bytes, lossily decoded and
/// trimmed. Never panics on non-UTF-8 input.
fn stdout_snippet(bytes: &[u8], cap: usize) -> String {
    let end = bytes.len().min(cap);
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

/// Spawn `program <probe_args>` with a hard timeout and bounded stdout capture.
///
/// Uses the threaded-channel `recv_timeout` pattern (mirroring
/// `agent_path::query_login_shell_path`) so a hung CLI cannot block the caller.
/// Never panics: spawn/wait failures degrade to a timed-out/empty result.
fn run_probe(program: &str, probe_args: &[&str], timeout: Duration) -> ProbeResult {
    let command = if probe_args.is_empty() {
        program.to_string()
    } else {
        format!("{} {}", program, probe_args.join(" "))
    };

    let program_owned = program.to_string();
    let args_owned: Vec<String> = probe_args.iter().map(|arg| arg.to_string()).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let output = std::process::Command::new(&program_owned)
            .args(&args_owned)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();
        let _ = tx.send(output);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => ProbeResult {
            command,
            exit_code: output.status.code(),
            stdout_snippet: stdout_snippet(&output.stdout, PROBE_SNIPPET_CAP),
            timed_out: false,
        },
        // Spawn failed (e.g. ENOENT after a TOCTOU race) — record as a failed,
        // non-timed-out probe with no output.
        Ok(Err(_)) => ProbeResult {
            command,
            exit_code: None,
            stdout_snippet: String::new(),
            timed_out: false,
        },
        // The probe did not respond within the timeout; the worker thread is
        // detached and will exit on its own.
        Err(_) => ProbeResult {
            command,
            exit_code: None,
            stdout_snippet: String::new(),
            timed_out: true,
        },
    }
}

/// A parsed semantic version: `(major, minor, patch)`.
type Version = (u64, u64, u64);

/// Parse the first `x.y.z` (or `x.y`, with patch defaulting to `0`) version
/// found anywhere in a probe snippet. Returns `None` when no two-or-more
/// component numeric version is present.
///
/// Tolerant by design (ADR 0009): it scans for the first match rather than
/// assuming a fixed layout, so `"gemini 1.2.3"`, `"v0.0.330 (abc)"`, and
/// `"copilot version 1.10"` all parse, while prose with no version yields
/// `None`. Numeric overflow saturates rather than panicking.
fn parse_version(snippet: &str) -> Option<Version> {
    let chars: Vec<char> = snippet.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            if let Some(version) = parse_version_at(&chars, i) {
                return Some(version);
            }
            // Skip the rest of this digit run before scanning further.
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Attempt to parse a `major.minor[.patch]` version starting at `start`.
/// Requires at least two dot-separated numeric components.
fn parse_version_at(chars: &[char], start: usize) -> Option<Version> {
    let mut idx = start;
    let mut nums: Vec<u64> = Vec::new();
    loop {
        let begin = idx;
        let mut value: u64 = 0;
        while idx < chars.len() && chars[idx].is_ascii_digit() {
            let digit = (chars[idx] as u8 - b'0') as u64;
            value = value.saturating_mul(10).saturating_add(digit);
            idx += 1;
        }
        if idx == begin {
            break;
        }
        nums.push(value);
        if nums.len() == 3 {
            break;
        }
        // Continue only across a single dot immediately followed by a digit.
        if idx + 1 < chars.len() && chars[idx] == '.' && chars[idx + 1].is_ascii_digit() {
            idx += 1;
        } else {
            break;
        }
    }
    match nums.len() {
        2 => Some((nums[0], nums[1], 0)),
        3 => Some((nums[0], nums[1], nums[2])),
        _ => None,
    }
}

/// Whether `detected` is strictly older than `min` (lexicographic on
/// major/minor/patch).
fn is_outdated(detected: Version, min: Version) -> bool {
    detected < min
}

/// Resolve the `(detected_version, version_warning)` pair for an agent from its
/// candidate probes and the registry `min_version` floor. The detected version
/// comes from the first candidate that resolved on the PATH and produced probe
/// output; the warning is set only when that version is below the floor.
fn detect_version(
    candidates: &[CandidateDiagnostic],
    min_version: Option<&str>,
) -> (Option<String>, Option<String>) {
    let detected = candidates
        .iter()
        .find(|candidate| candidate.found_on_path)
        .and_then(|candidate| candidate.probe.as_ref())
        .and_then(|probe| parse_version(&probe.stdout_snippet));
    let detected_string = detected.map(|(a, b, c)| format!("{a}.{b}.{c}"));

    let warning = match (detected, min_version.and_then(parse_version)) {
        (Some(detected), Some(min)) if is_outdated(detected, min) => Some(format!(
            "Detected version {}.{}.{} is older than the supported minimum {}.{}.{}. \
             Update the agent CLI.",
            detected.0, detected.1, detected.2, min.0, min.1, min.2
        )),
        _ => None,
    };
    (detected_string, warning)
}

/// Derive an agent verdict from its candidate traces (see module docs).
///
/// `availability_package` is the npm package the resolved launcher runs through
/// (e.g. the Claude adapter behind `npx`), or `None` for agents launched
/// directly from a binary. When set and unresolved locally, the launcher being
/// on the PATH is not enough — the verdict is `"package_missing"` (issue #241).
fn verdict_for(
    candidates: &[CandidateDiagnostic],
    availability_package: Option<&str>,
) -> &'static str {
    match candidates.iter().find(|candidate| candidate.found_on_path) {
        None => "not_found",
        Some(candidate) => {
            if let Some(pkg) = availability_package
                && !crate::agent_bridge::npm_package_available(pkg)
            {
                return "package_missing";
            }
            match &candidate.probe {
                Some(probe) if probe.timed_out || probe.exit_code != Some(0) => "probe_failed",
                _ => "available",
            }
        }
    }
}

/// Build the full diagnostic trace for one registry agent.
fn diagnose_agent(descriptor: &AgentDescriptor, path_dirs: &[PathBuf]) -> AgentDiagnostic {
    let candidates: Vec<CandidateDiagnostic> = descriptor
        .candidates
        .iter()
        .map(|candidate| {
            let (resolved, searched) = resolve_on_path(candidate.program, path_dirs);
            let probe = match (resolved.as_ref(), candidate.probe_args) {
                (Some(program), Some(probe_args)) => Some(run_probe(
                    &program.to_string_lossy(),
                    probe_args,
                    PROBE_TIMEOUT,
                )),
                _ => None,
            };
            CandidateDiagnostic {
                program: candidate.program.to_string(),
                args: candidate.args.iter().map(|arg| arg.to_string()).collect(),
                found_on_path: resolved.is_some(),
                resolved_program: resolved.map(|path| path.to_string_lossy().into_owned()),
                searched_dirs: searched
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                probe,
            }
        })
        .collect();

    let verdict = verdict_for(&candidates, descriptor.availability_package()).to_string();
    let (detected_version, version_warning) = detect_version(&candidates, descriptor.min_version);
    AgentDiagnostic {
        id: descriptor.id.to_string(),
        display_name: descriptor.display_name.to_string(),
        verdict,
        candidates,
        setup_hint: descriptor.setup_hint.to_string(),
        docs_url: descriptor.docs_url.to_string(),
        detected_version,
        version_warning,
    }
}

/// Build the on-demand diagnostics report: the resolved PATH plus a per-agent
/// discovery trace for every registry agent.
pub fn build_diagnostics() -> DiagnosticsReport {
    let path_dirs = current_path_dirs();
    let resolved_path = path_dirs
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let agents = builtin_registry()
        .iter()
        .map(|descriptor| diagnose_agent(descriptor, &path_dirs))
        .collect();
    DiagnosticsReport {
        resolved_path,
        agents,
    }
}

/// On-demand structured diagnostics for agent discovery (ADR 0013 decision 5).
#[tauri::command]
pub async fn agent_diagnostics() -> Result<DiagnosticsReport, String> {
    Ok(build_diagnostics())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "notesmith-diag-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_on_path_finds_program_in_provided_dir() {
        let dir = temp_dir("found");
        let bin = dir.join("faux-agent");
        fs::write(&bin, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        make_executable(&bin);

        let (resolved, searched) = resolve_on_path("faux-agent", &[dir.clone()]);
        assert_eq!(resolved.as_deref(), Some(bin.as_path()));
        assert_eq!(searched, vec![dir.clone()]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_on_path_reports_searched_dirs_when_absent() {
        let a = temp_dir("absent-a");
        let b = temp_dir("absent-b");
        let (resolved, searched) = resolve_on_path("nope-agent", &[a.clone(), b.clone()]);
        assert!(resolved.is_none());
        assert_eq!(searched, vec![a.clone(), b.clone()]);
        fs::remove_dir_all(&a).ok();
        fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn resolve_on_path_accepts_existing_absolute_path() {
        let dir = temp_dir("abs");
        let bin = dir.join("agent-bin");
        fs::write(&bin, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        make_executable(&bin);

        let other = temp_dir("abs-other");
        // An explicit path is checked directly, not searched in path_dirs.
        let (resolved, searched) = resolve_on_path(&bin.to_string_lossy(), &[other.clone()]);
        assert_eq!(resolved.as_deref(), Some(bin.as_path()));
        assert!(searched.is_empty());

        let (missing, searched_missing) =
            resolve_on_path("/no/such/agent/binary", &[other.clone()]);
        assert!(missing.is_none());
        assert!(searched_missing.is_empty());

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&other).ok();
    }

    #[test]
    fn stdout_snippet_truncates_to_cap_and_trims() {
        let long = vec![b'x'; 2000];
        let snippet = stdout_snippet(&long, PROBE_SNIPPET_CAP);
        assert_eq!(snippet.len(), PROBE_SNIPPET_CAP);

        let trimmed = stdout_snippet(b"   gemini 1.2.3 \n", PROBE_SNIPPET_CAP);
        assert_eq!(trimmed, "gemini 1.2.3");
    }

    #[test]
    fn stdout_snippet_handles_non_utf8_without_panicking() {
        let snippet = stdout_snippet(&[0xff, 0xfe, 0x41], PROBE_SNIPPET_CAP);
        assert!(snippet.contains('A'));
    }

    #[test]
    fn verdict_available_when_found_and_no_probe() {
        let candidates = vec![CandidateDiagnostic {
            program: "x".into(),
            args: vec![],
            resolved_program: Some("/bin/x".into()),
            found_on_path: true,
            searched_dirs: vec![],
            probe: None,
        }];
        assert_eq!(verdict_for(&candidates, None), "available");
    }

    #[test]
    fn verdict_not_found_when_nothing_resolves() {
        let candidates = vec![CandidateDiagnostic {
            program: "x".into(),
            args: vec![],
            resolved_program: None,
            found_on_path: false,
            searched_dirs: vec!["/bin".into()],
            probe: None,
        }];
        assert_eq!(verdict_for(&candidates, None), "not_found");
    }

    #[test]
    fn verdict_probe_failed_when_probe_times_out_or_errors() {
        let timed_out = vec![CandidateDiagnostic {
            program: "x".into(),
            args: vec![],
            resolved_program: Some("/bin/x".into()),
            found_on_path: true,
            searched_dirs: vec![],
            probe: Some(ProbeResult {
                command: "x --version".into(),
                exit_code: None,
                stdout_snippet: String::new(),
                timed_out: true,
            }),
        }];
        assert_eq!(verdict_for(&timed_out, None), "probe_failed");

        let nonzero = vec![CandidateDiagnostic {
            program: "x".into(),
            args: vec![],
            resolved_program: Some("/bin/x".into()),
            found_on_path: true,
            searched_dirs: vec![],
            probe: Some(ProbeResult {
                command: "x --version".into(),
                exit_code: Some(1),
                stdout_snippet: String::new(),
                timed_out: false,
            }),
        }];
        assert_eq!(verdict_for(&nonzero, None), "probe_failed");
    }

    #[test]
    fn verdict_available_when_probe_succeeds() {
        let candidates = vec![CandidateDiagnostic {
            program: "x".into(),
            args: vec![],
            resolved_program: Some("/bin/x".into()),
            found_on_path: true,
            searched_dirs: vec![],
            probe: Some(ProbeResult {
                command: "x --version".into(),
                exit_code: Some(0),
                stdout_snippet: "x 1.0".into(),
                timed_out: false,
            }),
        }];
        assert_eq!(verdict_for(&candidates, None), "available");
    }

    #[test]
    fn verdict_package_missing_when_launcher_found_but_adapter_absent() {
        // The launcher (e.g. `npx`) resolved, but the adapter package the agent
        // runs through is not installed locally (issue #241). A package name no
        // one would ever install keeps this deterministic and offline.
        let candidates = vec![CandidateDiagnostic {
            program: "npx".into(),
            args: vec!["--yes".into(), "@notesmith/definitely-not-real-pkg".into()],
            resolved_program: Some("/usr/bin/npx".into()),
            found_on_path: true,
            searched_dirs: vec![],
            probe: Some(ProbeResult {
                command: "npx --version".into(),
                exit_code: Some(0),
                stdout_snippet: "10.0.0".into(),
                timed_out: false,
            }),
        }];
        assert_eq!(
            verdict_for(&candidates, Some("@notesmith/definitely-not-real-pkg")),
            "package_missing"
        );
    }

    #[test]
    fn verdict_not_found_ignores_the_package_gate() {
        // No launcher on PATH wins regardless of the package gate.
        let candidates = vec![CandidateDiagnostic {
            program: "npx".into(),
            args: vec![],
            resolved_program: None,
            found_on_path: false,
            searched_dirs: vec!["/bin".into()],
            probe: None,
        }];
        assert_eq!(
            verdict_for(&candidates, Some("@notesmith/definitely-not-real-pkg")),
            "not_found"
        );
    }

    #[test]
    fn build_diagnostics_covers_every_registry_agent() {
        let report = build_diagnostics();
        assert_eq!(report.agents.len(), builtin_registry().len());
        assert!(!report.resolved_path.is_empty());

        let ids: Vec<&str> = report.agents.iter().map(|a| a.id.as_str()).collect();
        let expected: Vec<&str> = builtin_registry().iter().map(|d| d.id).collect();
        assert_eq!(ids, expected);

        for agent in &report.agents {
            assert!(matches!(
                agent.verdict.as_str(),
                "available" | "not_found" | "probe_failed" | "package_missing"
            ));
            assert!(!agent.candidates.is_empty());
        }
    }

    #[test]
    fn parse_version_handles_common_cli_formats() {
        assert_eq!(parse_version("gemini 1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("copilot version 1.10"), Some((1, 10, 0)));
        assert_eq!(parse_version("v0.0.330 (abc1234)"), Some((0, 0, 330)));
        assert_eq!(parse_version("codex-acp 2.0.0-beta.1"), Some((2, 0, 0)));
        // Trailing components beyond patch are ignored.
        assert_eq!(parse_version("4.5.6.7"), Some((4, 5, 6)));
    }

    #[test]
    fn parse_version_rejects_garbage_and_single_numbers() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("no version here"), None);
        // A lone integer is not a version (needs at least major.minor).
        assert_eq!(parse_version("version 5"), None);
        // The first valid version after noise still parses.
        assert_eq!(parse_version("build 7; release 1.4.2"), Some((1, 4, 2)));
    }

    #[test]
    fn parse_version_tolerates_replacement_chars_without_panicking() {
        // Lossy-decoded non-UTF-8 probe output may contain replacement chars;
        // parsing must not panic and should still find an embedded version.
        let lossy = "agent \u{FFFD}\u{FFFD} 3.1";
        assert_eq!(parse_version(lossy), Some((3, 1, 0)));
    }

    #[test]
    fn is_outdated_compares_components() {
        assert!(is_outdated((1, 2, 3), (1, 2, 4)));
        assert!(is_outdated((1, 2, 3), (1, 3, 0)));
        assert!(is_outdated((0, 9, 9), (1, 0, 0)));
        assert!(!is_outdated((1, 2, 3), (1, 2, 3)));
        assert!(!is_outdated((2, 0, 0), (1, 9, 9)));
    }

    fn found_candidate_with_snippet(snippet: &str) -> CandidateDiagnostic {
        CandidateDiagnostic {
            program: "agent".into(),
            args: vec![],
            resolved_program: Some("/bin/agent".into()),
            found_on_path: true,
            searched_dirs: vec![],
            probe: Some(ProbeResult {
                command: "agent --version".into(),
                exit_code: Some(0),
                stdout_snippet: snippet.into(),
                timed_out: false,
            }),
        }
    }

    #[test]
    fn detect_version_reports_detected_string() {
        let candidates = vec![found_candidate_with_snippet("agent 1.4.2")];
        let (detected, warning) = detect_version(&candidates, None);
        assert_eq!(detected.as_deref(), Some("1.4.2"));
        assert!(warning.is_none());
    }

    #[test]
    fn detect_version_warns_when_below_minimum() {
        let candidates = vec![found_candidate_with_snippet("agent 1.0.0")];
        let (detected, warning) = detect_version(&candidates, Some("1.5.0"));
        assert_eq!(detected.as_deref(), Some("1.0.0"));
        let warning = warning.expect("expected an outdated warning");
        assert!(warning.contains("1.0.0"));
        assert!(warning.contains("1.5.0"));
    }

    #[test]
    fn detect_version_no_warning_when_up_to_date_or_unparsable() {
        let up_to_date = vec![found_candidate_with_snippet("agent 2.0.0")];
        assert!(detect_version(&up_to_date, Some("1.5.0")).1.is_none());

        let no_version = vec![found_candidate_with_snippet("agent (dev build)")];
        let (detected, warning) = detect_version(&no_version, Some("1.5.0"));
        assert!(detected.is_none());
        assert!(warning.is_none());
    }
}
