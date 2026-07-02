//! Pure git2 operations: status, commit, pull, push, log.

use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::Serialize;

/// File extensions we auto-stage for commits.
const STAGEABLE_EXTENSIONS: &[&str] = &[
    "md", "yaml", "yml", "toml", "json", "png", "jpg", "jpeg", "gif", "svg", "pdf",
];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    pub changed: Vec<String>,
    pub staged: Vec<String>,
    pub untracked: Vec<String>,
    pub clean: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullResult {
    pub updated: bool,
    pub new_head: Option<String>,
    pub conflict: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushResult {
    pub pushed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitLogEntry {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

/// Rich per-commit history entry for the git-history UI. Field names are
/// camelCase to match the frontend `GitLogEntry` contract (`git-island/types.ts`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryEntry {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub author_email: String,
    pub timestamp_secs: i64,
    pub subject: String,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

/// A single line within a commit diff. Mirrors the frontend `DiffLine`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    Hunk,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}

/// One file's changes within a commit diff. Mirrors the frontend `DiffFile`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub path: String,
    pub status: DiffFileStatus,
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<DiffLine>,
}

/// A commit's full file-level diff. Mirrors the frontend `CommitDiff`.
#[derive(Debug, Clone, Serialize)]
pub struct CommitDiff {
    pub sha: String,
    pub files: Vec<DiffFile>,
}

/// Result of a commit: the new SHA plus the relative paths that were committed.
#[derive(Debug, Clone, Serialize)]
pub struct CommitOutcome {
    pub sha: String,
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns `true` if `path` is inside a git repository (has a `.git` directory).
pub fn is_git_repo(path: &Path) -> bool {
    git2::Repository::open(path).is_ok()
}

/// Returns the working-tree status of the repository at `path`.
pub fn status(path: &Path) -> Result<GitStatus> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .include_ignored(false)
        .recurse_untracked_dirs(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get git status")?;

    let mut changed = Vec::new();
    let mut staged = Vec::new();
    let mut untracked = Vec::new();

    for entry in statuses.iter() {
        let path_str = entry.path().unwrap_or("").to_string();
        let s = entry.status();
        if s.intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        ) {
            staged.push(path_str.clone());
        }
        if s.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        ) {
            changed.push(path_str.clone());
        }
        if s.intersects(git2::Status::WT_NEW) {
            untracked.push(path_str);
        }
    }

    let clean = changed.is_empty() && staged.is_empty() && untracked.is_empty();
    Ok(GitStatus {
        changed,
        staged,
        untracked,
        clean,
    })
}

/// Stage files matching [`STAGEABLE_EXTENSIONS`], commit if there are changes,
/// and return the new commit SHA (or `None` if nothing to commit).
///
/// Thin wrapper over [`commit_all`] with an explicit message; retained for
/// callers that only need the SHA.
pub fn auto_commit(path: &Path, message: &str) -> Result<Option<String>> {
    Ok(commit_all(path, Some(message))?.map(|outcome| outcome.sha))
}

/// Stage all changed, stageable working-tree files and commit them.
///
/// When `message` is `None`, a message is generated from the staged file list
/// (see [`generate_commit_message`]). Returns `None` when there is nothing to
/// commit.
pub fn commit_all(path: &Path, message: Option<&str>) -> Result<Option<CommitOutcome>> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;

    let mut index = repo.index().context("failed to get repo index")?;
    let staged = stage_changes(&repo, &mut index)?;

    if staged.is_empty() {
        return Ok(None);
    }

    let message = match message {
        Some(msg) => msg.to_string(),
        None => generate_commit_message(&staged),
    };

    index.write()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let sig = repo
        .signature()
        .or_else(|_| git2::Signature::now("notesmith", "notesmith@localhost"))
        .context("failed to create signature")?;

    let parent_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit().context("HEAD is not a commit")?),
        Err(e)
            if e.code() == git2::ErrorCode::UnbornBranch
                || e.code() == git2::ErrorCode::NotFound =>
        {
            None
        }
        Err(e) => return Err(e.into()),
    };

    let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;

    Ok(Some(CommitOutcome {
        sha: oid.to_string(),
        files: staged,
    }))
}

/// Stage every changed, stageable working-tree file into `index` and return the
/// staged relative paths (deletions included).
fn stage_changes(repo: &git2::Repository, index: &mut git2::Index) -> Result<Vec<String>> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .include_ignored(false)
        .recurse_untracked_dirs(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get status")?;

    let mut staged = Vec::new();
    for entry in statuses.iter() {
        let Some(file_path) = entry.path() else {
            continue;
        };
        let s = entry.status();
        let dominated = s.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_NEW
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        );
        if !dominated {
            continue;
        }
        if !is_stageable(file_path) {
            continue;
        }

        if s.contains(git2::Status::WT_DELETED) {
            index.remove_path(Path::new(file_path)).ok();
        } else {
            index.add_path(Path::new(file_path))?;
        }
        staged.push(file_path.to_string());
    }

    Ok(staged)
}

/// Build a commit message from a changed-file list (list-summary format), e.g.
/// `"Update note-a.md, note-b.md and 3 more"`. Uses file basenames.
pub fn generate_commit_message(files: &[String]) -> String {
    const SHOWN: usize = 2;

    let names: Vec<&str> = files
        .iter()
        .map(|f| {
            Path::new(f)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(f.as_str())
        })
        .collect();

    match names.len() {
        0 => "notesmith: checkpoint".to_string(),
        1 => format!("Update {}", names[0]),
        n if n <= SHOWN => format!("Update {}", names.join(", ")),
        n => format!(
            "Update {} and {} more",
            names[..SHOWN].join(", "),
            n - SHOWN
        ),
    }
}

/// Return the newest modification time among changed, stageable working-tree
/// files, or `None` when the tree has no stageable changes. Used to gate
/// inactivity checkpoints: a commit fires only once the newest change is older
/// than the configured inactivity window.
pub fn newest_change_mtime(path: &Path) -> Result<Option<SystemTime>> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .include_ignored(false)
        .recurse_untracked_dirs(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get status")?;

    let mut newest: Option<SystemTime> = None;
    for entry in statuses.iter() {
        let Some(file_path) = entry.path() else {
            continue;
        };
        let s = entry.status();
        if !s.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_NEW
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        ) {
            continue;
        }
        if !is_stageable(file_path) {
            continue;
        }
        // Deleted files have no mtime; treat the change as "just happened" so a
        // deletion still gates the inactivity window rather than being ignored.
        let mtime = match std::fs::metadata(path.join(file_path)).and_then(|m| m.modified()) {
            Ok(mtime) => mtime,
            Err(_) => SystemTime::now(),
        };
        newest = Some(match newest {
            Some(current) if current >= mtime => current,
            _ => mtime,
        });
    }

    Ok(newest)
}

/// Fetch from remote and attempt a fast-forward merge.
pub fn pull_ff(path: &Path, remote_name: &str) -> Result<PullResult> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;

    // Fetch
    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("remote '{remote_name}' not found"))?;
    remote
        .fetch(&[] as &[&str], None, None)
        .context("fetch failed")?;

    // Determine current branch
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            return Ok(PullResult {
                updated: false,
                new_head: None,
                conflict: false,
            });
        }
        Err(e) => return Err(e.into()),
    };

    let branch_name = head.shorthand().unwrap_or("main").to_string();

    // Find the remote tracking reference
    let remote_ref_name = format!("refs/remotes/{remote_name}/{branch_name}");
    let remote_ref = match repo.find_reference(&remote_ref_name) {
        Ok(r) => r,
        Err(_) => {
            return Ok(PullResult {
                updated: false,
                new_head: None,
                conflict: false,
            });
        }
    };

    let remote_oid = remote_ref.target().context("remote ref has no target")?;

    let local_oid = head.target().context("HEAD has no target")?;

    if local_oid == remote_oid {
        return Ok(PullResult {
            updated: false,
            new_head: None,
            conflict: false,
        });
    }

    // Check if fast-forward is possible
    let (merge_analysis, _) = repo.merge_analysis(&[&repo.find_annotated_commit(remote_oid)?])?;

    if merge_analysis.is_fast_forward() {
        // Update the branch ref directly (not HEAD, which is symbolic)
        repo.reference(
            &format!("refs/heads/{branch_name}"),
            remote_oid,
            true,
            "notesmith: fast-forward pull",
        )?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;

        Ok(PullResult {
            updated: true,
            new_head: Some(remote_oid.to_string()),
            conflict: false,
        })
    } else if merge_analysis.is_up_to_date() {
        Ok(PullResult {
            updated: false,
            new_head: None,
            conflict: false,
        })
    } else {
        // Not fast-forwardable — conflict
        Ok(PullResult {
            updated: false,
            new_head: None,
            conflict: true,
        })
    }
}

/// Push the current branch to the named remote.
pub fn push(path: &Path, remote_name: &str) -> Result<PushResult> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;

    let head = match repo.head() {
        Ok(h) => h,
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            return Ok(PushResult {
                pushed: false,
                error: Some("no commits to push".into()),
            });
        }
        Err(e) => return Err(e.into()),
    };

    let branch_name = head.shorthand().unwrap_or("main").to_string();
    let refspec = format!("refs/heads/{branch_name}:refs/heads/{branch_name}");

    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("remote '{remote_name}' not found"))?;

    let mut push_error: Option<String> = None;
    {
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.push_update_reference(|_refname, status| {
            if let Some(msg) = status {
                push_error = Some(msg.to_string());
            }
            Ok(())
        });
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        match remote.push(&[&refspec], Some(&mut push_opts)) {
            Ok(()) => {}
            Err(e) => {
                return Ok(PushResult {
                    pushed: false,
                    error: Some(e.message().to_string()),
                });
            }
        }
    }

    if let Some(err) = push_error {
        return Ok(PushResult {
            pushed: false,
            error: Some(err),
        });
    }

    Ok(PushResult {
        pushed: true,
        error: None,
    })
}

/// Return the most recent `limit` commits from the repository log.
pub fn log(path: &Path, limit: usize) -> Result<Vec<GitLogEntry>> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;

    let head = match repo.head() {
        Ok(h) => h,
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    let head_oid = head.target().context("HEAD has no target")?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_oid)?;
    revwalk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;

    let mut entries = Vec::new();
    for oid in revwalk.take(limit) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let timestamp = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        entries.push(GitLogEntry {
            sha: oid.to_string(),
            message: commit.message().unwrap_or("").trim().to_string(),
            author: commit.author().name().unwrap_or("").to_string(),
            timestamp,
        });
    }

    Ok(entries)
}

/// Rich commit history with per-commit diff stats, for the git-history UI.
/// Newest first, up to `limit` commits. Returns an empty list for a repo with
/// no commits yet.
pub fn history(path: &Path, limit: usize) -> Result<Vec<GitHistoryEntry>> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;

    let head = match repo.head() {
        Ok(h) => h,
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    let head_oid = head.target().context("HEAD has no target")?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_oid)?;
    revwalk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;

    let mut entries = Vec::new();
    for oid in revwalk.take(limit) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => Some(parent.tree()?),
            Err(_) => None,
        };

        let diff =
            repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_options()))?;
        let stats = diff.stats()?;

        let message = commit.message().unwrap_or("");
        let subject = message.lines().next().unwrap_or("").trim().to_string();
        let sha = oid.to_string();
        let short_sha = sha.chars().take(7).collect();

        entries.push(GitHistoryEntry {
            sha,
            short_sha,
            author: commit.author().name().unwrap_or("").to_string(),
            author_email: commit.author().email().unwrap_or("").to_string(),
            timestamp_secs: commit.time().seconds(),
            subject,
            files_changed: stats.files_changed(),
            insertions: stats.insertions(),
            deletions: stats.deletions(),
        });
    }

    Ok(entries)
}

/// The full file-level diff of a single commit against its first parent
/// (or the empty tree for a root commit). `rev` may be a full or abbreviated
/// SHA, or any revision git can resolve.
pub fn commit_diff(path: &Path, rev: &str) -> Result<CommitDiff> {
    let repo = git2::Repository::open(path).context("failed to open git repo")?;
    let object = repo
        .revparse_single(rev)
        .with_context(|| format!("unknown revision: {rev}"))?;
    let commit = object
        .peel_to_commit()
        .with_context(|| format!("revision is not a commit: {rev}"))?;

    let tree = commit.tree()?;
    let parent_tree = match commit.parent(0) {
        Ok(parent) => Some(parent.tree()?),
        Err(_) => None,
    };

    let diff =
        repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_options()))?;

    let mut files = Vec::new();
    let delta_count = diff.deltas().len();
    for idx in 0..delta_count {
        let delta = match diff.get_delta(idx) {
            Some(d) => d,
            None => continue,
        };
        let status = map_delta_status(delta.status());
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Binary files (or unreadable) have no textual patch.
        let patch = git2::Patch::from_diff(&diff, idx)?;
        let (added, removed, lines) = match patch {
            Some(mut patch) => build_diff_lines(&mut patch)?,
            None => (0, 0, Vec::new()),
        };

        files.push(DiffFile {
            path: file_path,
            status,
            added,
            removed,
            lines,
        });
    }

    Ok(CommitDiff {
        sha: commit.id().to_string(),
        files,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Diff options shared by history stats and commit-diff rendering.
fn diff_options() -> git2::DiffOptions {
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3).ignore_whitespace(false);
    opts
}

fn map_delta_status(status: git2::Delta) -> DiffFileStatus {
    match status {
        git2::Delta::Added | git2::Delta::Copied | git2::Delta::Untracked => DiffFileStatus::Added,
        git2::Delta::Deleted => DiffFileStatus::Deleted,
        git2::Delta::Renamed => DiffFileStatus::Renamed,
        _ => DiffFileStatus::Modified,
    }
}

/// Walk a file patch into hunk-header + line rows and count additions/deletions.
fn build_diff_lines(patch: &mut git2::Patch) -> Result<(usize, usize, Vec<DiffLine>)> {
    let mut lines = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    for h in 0..patch.num_hunks() {
        let (hunk, _line_count) = patch.hunk(h)?;
        let header = String::from_utf8_lossy(hunk.header())
            .trim_end()
            .to_string();
        lines.push(DiffLine {
            kind: DiffLineKind::Hunk,
            old_line: None,
            new_line: None,
            text: header,
        });

        let num_lines = patch.num_lines_in_hunk(h)?;
        for l in 0..num_lines {
            let line = patch.line_in_hunk(h, l)?;
            let kind = match line.origin() {
                '+' => {
                    added += 1;
                    DiffLineKind::Added
                }
                '-' => {
                    removed += 1;
                    DiffLineKind::Removed
                }
                _ => DiffLineKind::Context,
            };
            let text = String::from_utf8_lossy(line.content())
                .trim_end_matches(['\n', '\r'])
                .to_string();
            lines.push(DiffLine {
                kind,
                old_line: line.old_lineno(),
                new_line: line.new_lineno(),
                text,
            });
        }
    }

    Ok((added, removed, lines))
}

fn is_stageable(file_path: &str) -> bool {
    let path = Path::new(file_path);
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| STAGEABLE_EXTENSIONS.contains(&ext))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// Create a temporary git repo and return its path.
    fn init_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let repo = git2::Repository::init(&path).unwrap();

        // Configure user so commits work
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();

        (dir, path)
    }

    /// Create an initial commit so the repo has HEAD.
    fn make_initial_commit(path: &Path) {
        let repo = git2::Repository::open(path).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let mut index = repo.index().unwrap();

        // Write an initial file so we have something to commit
        fs::write(path.join("README.md"), "# Test").unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();

        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();
    }

    #[test]
    fn is_git_repo_true_for_git_dir() {
        let (dir, path) = init_test_repo();
        assert!(is_git_repo(&path));
        drop(dir);
    }

    #[test]
    fn is_git_repo_false_for_non_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_git_repo(dir.path()));
    }

    #[test]
    fn status_clean_on_empty_repo() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        let s = status(&path).unwrap();
        assert!(s.clean);
        assert!(s.changed.is_empty());
        assert!(s.staged.is_empty());
        assert!(s.untracked.is_empty());
    }

    #[test]
    fn status_detects_untracked_md_file() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("new-note.md"), "hello").unwrap();
        let s = status(&path).unwrap();
        assert!(!s.clean);
        assert!(s.untracked.contains(&"new-note.md".to_string()));
    }

    #[test]
    fn status_detects_modified_file() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        // Modify the tracked README.md
        fs::write(path.join("README.md"), "# Modified").unwrap();
        let s = status(&path).unwrap();
        assert!(!s.clean);
        assert!(s.changed.contains(&"README.md".to_string()));
    }

    #[test]
    fn auto_commit_stages_and_commits_md_file() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("note.md"), "# My note").unwrap();

        let sha = auto_commit(&path, "test commit").unwrap();
        assert!(sha.is_some(), "expected a commit SHA");

        // Verify the commit exists
        let entries = log(&path, 1).unwrap();
        assert_eq!(entries[0].message, "test commit");
    }

    #[test]
    fn auto_commit_ignores_non_stageable_extensions() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("script.sh"), "#!/bin/bash").unwrap();

        let sha = auto_commit(&path, "should be empty").unwrap();
        assert!(sha.is_none(), "should not commit non-stageable files");
    }

    #[test]
    fn auto_commit_returns_none_when_clean() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        let sha = auto_commit(&path, "nothing").unwrap();
        assert!(sha.is_none());
    }

    #[test]
    fn auto_commit_stages_yaml_and_json() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("config.yaml"), "key: value").unwrap();
        fs::write(path.join("data.json"), "{}").unwrap();

        let sha = auto_commit(&path, "config files").unwrap();
        assert!(sha.is_some());
    }

    #[test]
    fn auto_commit_on_empty_repo_creates_initial_commit() {
        let (_dir, path) = init_test_repo();
        fs::write(path.join("first.md"), "# First").unwrap();

        let sha = auto_commit(&path, "initial").unwrap();
        assert!(sha.is_some());

        let entries = log(&path, 1).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "initial");
    }

    #[test]
    fn log_returns_commits_in_reverse_chronological_order() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        fs::write(path.join("a.md"), "a").unwrap();
        auto_commit(&path, "second").unwrap();

        fs::write(path.join("b.md"), "b").unwrap();
        auto_commit(&path, "third").unwrap();

        let entries = log(&path, 10).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "third");
        assert_eq!(entries[1].message, "second");
        assert_eq!(entries[2].message, "initial commit");
    }

    #[test]
    fn log_respects_limit() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        fs::write(path.join("a.md"), "a").unwrap();
        auto_commit(&path, "second").unwrap();

        let entries = log(&path, 1).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn log_empty_repo_returns_empty() {
        let (_dir, path) = init_test_repo();
        let entries = log(&path, 10).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn history_reports_per_commit_stats() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        fs::write(path.join("a.md"), "line1\nline2\n").unwrap();
        auto_commit(&path, "add a.md").unwrap();

        let entries = history(&path, 10).unwrap();
        assert_eq!(entries.len(), 2);

        let newest = &entries[0];
        assert_eq!(newest.subject, "add a.md");
        assert_eq!(newest.short_sha.len(), 7);
        assert!(newest.sha.starts_with(&newest.short_sha));
        assert_eq!(newest.author, "test");
        assert_eq!(newest.author_email, "test@test.com");
        assert!(newest.timestamp_secs > 0);
        assert_eq!(newest.files_changed, 1);
        assert_eq!(newest.insertions, 2);
        assert_eq!(newest.deletions, 0);
    }

    #[test]
    fn history_empty_repo_returns_empty() {
        let (_dir, path) = init_test_repo();
        assert!(history(&path, 10).unwrap().is_empty());
    }

    #[test]
    fn commit_diff_renders_added_and_removed_lines() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        // Modify the tracked README.md: change one line.
        fs::write(path.join("README.md"), "# Test\nnew line\n").unwrap();
        auto_commit(&path, "edit readme").unwrap();

        let head = log(&path, 1).unwrap();
        let diff = commit_diff(&path, &head[0].sha).unwrap();
        assert_eq!(diff.sha, head[0].sha);

        let file = diff
            .files
            .iter()
            .find(|f| f.path == "README.md")
            .expect("README.md in diff");
        assert!(matches!(file.status, DiffFileStatus::Modified));
        assert!(file.added >= 1);
        // The diff must contain at least one hunk header and one added line.
        assert!(
            file.lines
                .iter()
                .any(|l| matches!(l.kind, DiffLineKind::Hunk))
        );
        assert!(
            file.lines
                .iter()
                .any(|l| matches!(l.kind, DiffLineKind::Added) && l.text.contains("new line"))
        );
    }

    #[test]
    fn commit_diff_root_commit_shows_added_file() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        let head = log(&path, 1).unwrap();
        let diff = commit_diff(&path, &head[0].sha).unwrap();
        let file = diff
            .files
            .iter()
            .find(|f| f.path == "README.md")
            .expect("README.md in root diff");
        assert!(matches!(file.status, DiffFileStatus::Added));
    }

    #[test]
    fn commit_diff_unknown_revision_errors() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        assert!(commit_diff(&path, "deadbeefdeadbeef").is_err());
    }

    #[test]
    fn is_stageable_accepts_known_extensions() {
        assert!(is_stageable("notes/foo.md"));
        assert!(is_stageable("config.yaml"));
        assert!(is_stageable("config.yml"));
        assert!(is_stageable("config.toml"));
        assert!(is_stageable("data.json"));
        assert!(is_stageable("image.png"));
        assert!(is_stageable("image.jpg"));
        assert!(is_stageable("doc.pdf"));
        assert!(is_stageable("icon.svg"));
    }

    #[test]
    fn is_stageable_rejects_unknown_extensions() {
        assert!(!is_stageable("script.sh"));
        assert!(!is_stageable("binary.exe"));
        assert!(!is_stageable("Makefile"));
        assert!(!is_stageable("data.csv"));
    }

    #[test]
    fn generate_commit_message_zero_files() {
        assert_eq!(generate_commit_message(&[]), "notesmith: checkpoint");
    }

    #[test]
    fn generate_commit_message_single_file_uses_basename() {
        let files = vec!["notes/sub/idea.md".to_string()];
        assert_eq!(generate_commit_message(&files), "Update idea.md");
    }

    #[test]
    fn generate_commit_message_two_files() {
        let files = vec!["a.md".to_string(), "dir/b.md".to_string()];
        assert_eq!(generate_commit_message(&files), "Update a.md, b.md");
    }

    #[test]
    fn generate_commit_message_many_files_summarizes() {
        let files = vec![
            "note-a.md".to_string(),
            "note-b.md".to_string(),
            "c.md".to_string(),
            "d.md".to_string(),
            "e.md".to_string(),
        ];
        assert_eq!(
            generate_commit_message(&files),
            "Update note-a.md, note-b.md and 3 more"
        );
    }

    #[test]
    fn commit_all_generates_message_when_none() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("alpha.md"), "a").unwrap();
        fs::write(path.join("beta.md"), "b").unwrap();

        let outcome = commit_all(&path, None).unwrap().expect("expected commit");
        assert_eq!(outcome.files.len(), 2);
        let entries = log(&path, 1).unwrap();
        assert!(
            entries[0].message.starts_with("Update "),
            "generated message, got: {}",
            entries[0].message
        );
    }

    #[test]
    fn commit_all_uses_explicit_message() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("n.md"), "x").unwrap();

        let outcome = commit_all(&path, Some("explicit msg"))
            .unwrap()
            .expect("expected commit");
        assert_eq!(outcome.files, vec!["n.md".to_string()]);
        let entries = log(&path, 1).unwrap();
        assert_eq!(entries[0].message, "explicit msg");
    }

    #[test]
    fn commit_all_returns_none_when_clean() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        assert!(commit_all(&path, None).unwrap().is_none());
    }

    #[test]
    fn newest_change_mtime_none_when_clean() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        assert!(newest_change_mtime(&path).unwrap().is_none());
    }

    #[test]
    fn newest_change_mtime_some_when_dirty() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("dirty.md"), "changed").unwrap();
        assert!(newest_change_mtime(&path).unwrap().is_some());
    }

    #[test]
    fn newest_change_mtime_ignores_non_stageable() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);
        fs::write(path.join("ignore.sh"), "x").unwrap();
        assert!(newest_change_mtime(&path).unwrap().is_none());
    }

    #[test]
    fn pull_ff_on_repo_without_remote() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        let result = pull_ff(&path, "origin");
        assert!(result.is_err()); // no remote configured
    }

    #[test]
    fn push_on_repo_without_remote() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        let result = push(&path, "origin");
        assert!(result.is_err()); // no remote configured
    }

    #[test]
    fn pull_ff_with_local_bare_remote() {
        // Set up a bare "remote" repo
        let bare_dir = tempfile::tempdir().unwrap();
        let bare_path = bare_dir.path().to_path_buf();
        git2::Repository::init_bare(&bare_path).unwrap();

        // Clone it to create a working repo with an origin
        let work_dir = tempfile::tempdir().unwrap();
        let work_path = work_dir.path().join("repo");
        let repo = git2::Repository::clone(bare_path.to_str().unwrap(), &work_path).unwrap();

        // Configure user
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();

        // Make an initial commit
        fs::write(work_path.join("README.md"), "# hello").unwrap();
        auto_commit(&work_path, "initial").unwrap();

        // Push to bare
        push(&work_path, "origin").unwrap();

        // Clone again to simulate a second user
        let work2_dir = tempfile::tempdir().unwrap();
        let work2_path = work2_dir.path().join("repo");
        let repo2 = git2::Repository::clone(bare_path.to_str().unwrap(), &work2_path).unwrap();
        let mut config2 = repo2.config().unwrap();
        config2.set_str("user.name", "test2").unwrap();
        config2.set_str("user.email", "test2@test.com").unwrap();

        // Second user adds a commit and pushes
        fs::write(work2_path.join("note.md"), "# a note").unwrap();
        auto_commit(&work2_path, "from user2").unwrap();
        push(&work2_path, "origin").unwrap();

        // First user pulls — should fast-forward
        let result = pull_ff(&work_path, "origin").unwrap();
        assert!(result.updated);
        assert!(!result.conflict);
        assert!(result.new_head.is_some());

        // Verify the file arrived
        assert!(work_path.join("note.md").exists());
    }

    #[test]
    fn push_to_bare_remote_succeeds() {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare_path = bare_dir.path().to_path_buf();
        git2::Repository::init_bare(&bare_path).unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let work_path = work_dir.path().join("repo");
        let repo = git2::Repository::clone(bare_path.to_str().unwrap(), &work_path).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();

        fs::write(work_path.join("note.md"), "# note").unwrap();
        auto_commit(&work_path, "first commit").unwrap();

        let result = push(&work_path, "origin").unwrap();
        assert!(result.pushed);
        assert!(result.error.is_none());
    }

    #[test]
    fn push_empty_repo_returns_error() {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare_path = bare_dir.path().to_path_buf();
        git2::Repository::init_bare(&bare_path).unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let work_path = work_dir.path().join("repo");
        git2::Repository::clone(bare_path.to_str().unwrap(), &work_path).unwrap();

        let result = push(&work_path, "origin").unwrap();
        assert!(!result.pushed);
    }

    #[test]
    fn auto_commit_handles_deleted_file() {
        let (_dir, path) = init_test_repo();
        make_initial_commit(&path);

        // Add and commit a file
        fs::write(path.join("to-delete.md"), "will be deleted").unwrap();
        auto_commit(&path, "add file").unwrap();

        // Delete the file
        fs::remove_file(path.join("to-delete.md")).unwrap();
        let sha = auto_commit(&path, "delete file").unwrap();
        assert!(sha.is_some());

        let entries = log(&path, 1).unwrap();
        assert_eq!(entries[0].message, "delete file");
    }
}
