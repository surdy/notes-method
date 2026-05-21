//! Vault-wide wikilink/embed target rewriting.
//!
//! Pure helpers for rewriting `[[target]]` and `![[target]]` references when a
//! note is renamed. Skips fenced code blocks, inline code spans, and
//! frontmatter so we never accidentally edit example links.
//!
//! See issue #98.

use crate::frontmatter::extract_frontmatter;
use crate::parser::find_code_block_ranges;
use std::ops::Range;
use std::path::{Path, PathBuf};
use tracing::warn;
use walkdir::WalkDir;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WikilinkRewriteResult {
    pub files_scanned: usize,
    pub files_modified: usize,
    pub references_rewritten: usize,
}

/// Rewrites `[[old]]` / `![[old]]` references to `new` inside a single note's
/// body (frontmatter is excluded by the caller).
///
/// Matching rules:
/// - If `old_target` contains `/`, it matches the full vault-relative target
///   string exactly (case-insensitive).
/// - Otherwise it matches the basename of the wikilink target, where the
///   basename is the substring after the last `/`. This mirrors Obsidian's
///   resolution: `[[Foo]]` and `[[folder/Foo]]` both refer to a note whose
///   basename is `Foo`, so renaming `Foo` should rewrite both.
///
/// Anchors (`#heading`), block refs (`^id`), and display text (`|alias`) on
/// the link are preserved verbatim.
pub fn rewrite_body(body: &str, old_target: &str, new_target: &str) -> (String, usize) {
    let excluded = find_code_block_ranges(body);
    let mut output = String::with_capacity(body.len());
    let mut count = 0;
    let mut cursor = 0;
    let bytes = body.as_bytes();

    while let Some(start_rel) = body[cursor..].find("[[") {
        let start = cursor + start_rel;
        // Copy everything before the link.
        output.push_str(&body[cursor..start]);

        let Some(end_rel) = body[start + 2..].find("]]") else {
            // Unterminated link — copy rest and stop.
            output.push_str(&body[start..]);
            cursor = body.len();
            break;
        };
        let inner_start = start + 2;
        let inner_end = inner_start + end_rel;
        let end = inner_end + 2;

        let inner = &body[inner_start..inner_end];
        // Reject nested brackets (e.g. `[[a [[b]] c]]`) — too ambiguous; skip.
        if inner.contains("[[") {
            output.push_str(&body[start..end]);
            cursor = end;
            continue;
        }

        if overlaps(start, end, &excluded) {
            output.push_str(&body[start..end]);
            cursor = end;
            continue;
        }

        // Determine if this is an embed (preceded by `!`).
        let is_embed = start > 0 && bytes[start - 1] == b'!';

        // Split target from anchor / block / display.
        let (target_part, suffix) = split_target(inner);
        if target_matches(target_part, old_target) {
            let rewritten_target = rewrite_target(target_part, old_target, new_target);
            // Reconstruct the link with original prefix preserved.
            output.push_str("[[");
            output.push_str(&rewritten_target);
            output.push_str(suffix);
            output.push_str("]]");
            count += 1;
            let _ = is_embed; // `!` was already copied via the `body[cursor..start]` slice.
        } else {
            output.push_str(&body[start..end]);
        }
        cursor = end;
    }
    output.push_str(&body[cursor..]);
    (output, count)
}

/// Rewrites links across an entire note's content (frontmatter + body).
/// Frontmatter is left untouched.
pub fn rewrite_content(content: &str, old_target: &str, new_target: &str) -> (String, usize) {
    let (frontmatter, body) = extract_frontmatter(content);
    let (new_body, count) = rewrite_body(body, old_target, new_target);
    let mut out = String::with_capacity(content.len());
    if let Some(fm) = frontmatter {
        out.push_str("---\n");
        out.push_str(&fm);
        if !fm.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("---\n");
    }
    out.push_str(&new_body);
    (out, count)
}

/// Walks `vault_root` and rewrites every wikilink/embed that targets
/// `old_target` to `new_target`. Returns aggregate stats.
///
/// Resilience: notes that fail to read or write are logged and skipped — the
/// rewrite continues for other files. This matches ADR 0009 (resilience to
/// malformed content).
pub fn rewrite_wikilinks(
    vault_root: &Path,
    old_target: &str,
    new_target: &str,
) -> std::io::Result<WikilinkRewriteResult> {
    let mut result = WikilinkRewriteResult::default();
    if old_target == new_target {
        return Ok(result);
    }

    for entry in WalkDir::new(vault_root)
        .into_iter()
        .filter_entry(|e| !is_skipped_dir(e.path()))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!(error = %err, "skipping vault entry during wikilink rewrite");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        result.files_scanned += 1;

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(err) => {
                warn!(path = %path.display(), error = %err, "skipping unreadable note during wikilink rewrite");
                continue;
            }
        };

        let (new_content, count) = rewrite_content(&content, old_target, new_target);
        if count > 0 {
            if let Err(err) = atomic_write(path, &new_content) {
                warn!(path = %path.display(), error = %err, "failed to write rewritten note");
                continue;
            }
            result.files_modified += 1;
            result.references_rewritten += count;
        }
    }
    Ok(result)
}

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == ".notesmith" || n == ".obsidian" || n == ".git")
        .unwrap_or(false)
}

fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = PathBuf::from(dir);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("note.md");
    tmp.push(format!(".{file_name}.notesmith-tmp"));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Splits a wikilink interior `target#anchor|display` into target and the
/// suffix (everything from the first `#`, `^`, or `|` onwards, including that
/// delimiter). Whitespace around the target is preserved by trimming only the
/// returned target slice for matching purposes.
fn split_target(inner: &str) -> (&str, &str) {
    let split_at = inner.find(['#', '|', '^']).unwrap_or(inner.len());
    (&inner[..split_at], &inner[split_at..])
}

fn target_matches(target: &str, old: &str) -> bool {
    let target_trimmed = target.trim();
    if old.contains('/') {
        target_trimmed.eq_ignore_ascii_case(old)
    } else {
        let basename = target_trimmed.rsplit('/').next().unwrap_or(target_trimmed);
        basename.eq_ignore_ascii_case(old)
    }
}

fn rewrite_target(target: &str, old: &str, new: &str) -> String {
    let leading_len = target.len() - target.trim_start().len();
    let leading = &target[..leading_len];
    let after_leading = &target[leading_len..];
    let trimmed_len = after_leading.trim_end().len();
    let trailing = &after_leading[trimmed_len..];
    let target_trimmed = &after_leading[..trimmed_len];

    let rewritten = if old.contains('/') {
        new.to_string()
    } else if let Some((dir, _basename)) = target_trimmed.rsplit_once('/') {
        format!("{dir}/{new}")
    } else {
        new.to_string()
    };
    format!("{leading}{rewritten}{trailing}")
}

fn overlaps(start: usize, end: usize, excluded: &[Range<usize>]) -> bool {
    excluded.iter().any(|r| start < r.end && end > r.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(body: &str, old: &str, new: &str) -> (String, usize) {
        rewrite_body(body, old, new)
    }

    #[test]
    fn rewrites_simple_wikilink() {
        let (out, n) = rewrite("see [[Foo]] please", "Foo", "Bar");
        assert_eq!(out, "see [[Bar]] please");
        assert_eq!(n, 1);
    }

    #[test]
    fn preserves_display_text() {
        let (out, n) = rewrite("see [[Foo|the foo]]", "Foo", "Bar");
        assert_eq!(out, "see [[Bar|the foo]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn preserves_heading_anchor() {
        let (out, n) = rewrite("see [[Foo#Section]]", "Foo", "Bar");
        assert_eq!(out, "see [[Bar#Section]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn preserves_block_ref() {
        let (out, n) = rewrite("see [[Foo#^abc123]]", "Foo", "Bar");
        assert_eq!(out, "see [[Bar#^abc123]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrites_embed() {
        let (out, n) = rewrite("![[Foo]]", "Foo", "Bar");
        assert_eq!(out, "![[Bar]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrites_embed_with_anchor() {
        let (out, n) = rewrite("![[Foo#Section|caption]]", "Foo", "Bar");
        assert_eq!(out, "![[Bar#Section|caption]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn no_match_returns_zero() {
        let (out, n) = rewrite("see [[Other]]", "Foo", "Bar");
        assert_eq!(out, "see [[Other]]");
        assert_eq!(n, 0);
    }

    #[test]
    fn rewrites_multiple_matches() {
        let (out, n) = rewrite("[[Foo]] then [[Foo|alias]] and [[Foo#h]]", "Foo", "Bar");
        assert_eq!(out, "[[Bar]] then [[Bar|alias]] and [[Bar#h]]");
        assert_eq!(n, 3);
    }

    #[test]
    fn case_insensitive_match() {
        let (out, n) = rewrite("see [[foo]] and [[FOO]]", "Foo", "Bar");
        assert_eq!(out, "see [[Bar]] and [[Bar]]");
        assert_eq!(n, 2);
    }

    #[test]
    fn skips_fenced_code() {
        let body = "outside [[Foo]]\n```\ninside [[Foo]]\n```\nafter [[Foo]]";
        let (out, n) = rewrite(body, "Foo", "Bar");
        assert_eq!(
            out,
            "outside [[Bar]]\n```\ninside [[Foo]]\n```\nafter [[Bar]]"
        );
        assert_eq!(n, 2);
    }

    #[test]
    fn skips_inline_code() {
        let body = "outside [[Foo]] and `inline [[Foo]] code` and [[Foo]]";
        let (out, n) = rewrite(body, "Foo", "Bar");
        assert_eq!(out, "outside [[Bar]] and `inline [[Foo]] code` and [[Bar]]");
        assert_eq!(n, 2);
    }

    #[test]
    fn skips_frontmatter() {
        let content = "---\ntitle: Foo\nrelated: \"[[Foo]]\"\n---\nbody [[Foo]]";
        let (out, n) = rewrite_content(content, "Foo", "Bar");
        assert_eq!(
            out,
            "---\ntitle: Foo\nrelated: \"[[Foo]]\"\n---\nbody [[Bar]]"
        );
        assert_eq!(n, 1);
    }

    #[test]
    fn basename_match_rewrites_path_prefixed() {
        // Bare old target matches `[[folder/Foo]]` and rewrites only the basename.
        let (out, n) = rewrite("see [[folder/Foo]] and [[Foo]]", "Foo", "Bar");
        assert_eq!(out, "see [[folder/Bar]] and [[Bar]]");
        assert_eq!(n, 2);
    }

    #[test]
    fn path_old_target_only_matches_full_path() {
        // When old contains a slash, `[[Foo]]` must NOT match.
        let (out, n) = rewrite(
            "see [[folder/Foo]] and [[Foo]] and [[other/Foo]]",
            "folder/Foo",
            "folder/Bar",
        );
        assert_eq!(out, "see [[folder/Bar]] and [[Foo]] and [[other/Foo]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn does_not_rewrite_partial_match() {
        let (out, n) = rewrite("see [[Foobar]] and [[Foo]]", "Foo", "Bar");
        assert_eq!(out, "see [[Foobar]] and [[Bar]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn unterminated_link_is_left_alone() {
        let (out, n) = rewrite("dangling [[Foo and more", "Foo", "Bar");
        assert_eq!(out, "dangling [[Foo and more");
        assert_eq!(n, 0);
    }

    #[test]
    fn integration_rewrites_files_in_vault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("a.md"), "see [[Foo]] and [[Foo|x]]").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/b.md"), "embed ![[Foo#h]]").unwrap();
        std::fs::write(root.join("c.md"), "no link here").unwrap();
        std::fs::create_dir_all(root.join(".notesmith")).unwrap();
        std::fs::write(root.join(".notesmith/skip.md"), "[[Foo]]").unwrap();

        let result = rewrite_wikilinks(root, "Foo", "Bar").expect("rewrite");
        assert_eq!(result.files_modified, 2);
        assert_eq!(result.references_rewritten, 3);
        assert!(result.files_scanned >= 3);

        assert_eq!(
            std::fs::read_to_string(root.join("a.md")).unwrap(),
            "see [[Bar]] and [[Bar|x]]"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("sub/b.md")).unwrap(),
            "embed ![[Bar#h]]"
        );
        // Skipped dir untouched.
        assert_eq!(
            std::fs::read_to_string(root.join(".notesmith/skip.md")).unwrap(),
            "[[Foo]]"
        );
    }

    #[test]
    fn integration_skips_unreadable_files_gracefully() {
        // A directory ending in .md still exists on some platforms; we just
        // verify malformed/unreadable content doesn't abort the walk.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("good.md"), "[[Foo]]").unwrap();
        // Write invalid UTF-8 bytes to a .md file — read_to_string will fail
        // and we expect the file to be skipped without aborting.
        std::fs::write(root.join("bad.md"), [0xFFu8, 0xFE, 0xFD]).unwrap();

        let result = rewrite_wikilinks(root, "Foo", "Bar").expect("rewrite");
        assert_eq!(result.files_modified, 1);
        assert_eq!(result.references_rewritten, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("good.md")).unwrap(),
            "[[Bar]]"
        );
    }

    #[test]
    fn no_op_when_old_equals_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.md"), "[[Foo]]").unwrap();
        let result = rewrite_wikilinks(dir.path(), "Foo", "Foo").expect("rewrite");
        assert_eq!(result, WikilinkRewriteResult::default());
    }
}
