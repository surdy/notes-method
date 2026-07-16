use crate::parse_note;
use notesmith_core::{
    Note, NotesmithError, VaultEngine, VaultName, VaultPath, WriteResult, traits::Result,
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::{DirEntry, WalkDir};

pub struct NativeVaultEngine;

impl VaultEngine for NativeVaultEngine {
    fn scan(&self, root: &Path) -> Result<Vec<Note>> {
        let vault_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .map(VaultName::new)
            .unwrap_or_else(|| VaultName::new("vault"));

        let mut notes = WalkDir::new(root)
            .into_iter()
            .filter_entry(|entry| !should_skip_entry(entry))
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry),
                Err(err) => {
                    tracing::warn!("skipping unreadable entry: {err}");
                    None
                }
            })
            .filter(|entry| is_markdown_file(entry.path()))
            .filter_map(|entry| {
                // Per ADR 0009, a single unreadable/malformed note (e.g. a
                // non-UTF-8 file) must not abort the whole vault scan. Skip it
                // with a structured warning and keep indexing the rest.
                match load_note(root, &vault_name, entry.path()) {
                    Ok(note) => Some(note),
                    Err(err) => {
                        tracing::warn!(
                            note = %entry.path().display(),
                            stage = "read",
                            reason = %err,
                            "skipping note that failed to load during scan"
                        );
                        None
                    }
                }
            })
            .collect::<Vec<_>>();

        notes.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        Ok(notes)
    }

    fn read(&self, root: &Path, path: &VaultPath) -> Result<String> {
        read_note_file(&root.join(path.as_str()), path)
    }

    fn write(
        &self,
        root: &Path,
        path: &VaultPath,
        expected_hash: Option<&str>,
        content: &str,
    ) -> Result<WriteResult> {
        let destination = root.join(path.as_str());
        if let Some(expected_hash) = expected_hash {
            match fs::read_to_string(&destination) {
                Ok(existing) => {
                    let actual = blake3::hash(existing.as_bytes()).to_hex().to_string();
                    if actual != expected_hash {
                        return Ok(WriteResult::Conflict {
                            expected: expected_hash.to_string(),
                            actual,
                        });
                    }
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    return Ok(WriteResult::Conflict {
                        expected: expected_hash.to_string(),
                        actual: String::new(),
                    });
                }
                Err(err) => return Err(err.into()),
            }
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = sibling_temp_path(&destination);
        fs::write(&temp_path, content)?;
        if let Err(err) = fs::rename(&temp_path, &destination) {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }

        Ok(WriteResult::Written {
            hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
        })
    }

    fn delete(&self, root: &Path, path: &VaultPath) -> Result<()> {
        let absolute = root.join(path.as_str());
        fs::remove_file(&absolute).map_err(|err| match err.kind() {
            ErrorKind::NotFound => NotesmithError::NoteNotFound { path: path.clone() },
            _ => err.into(),
        })?;
        Ok(())
    }

    fn move_path(&self, root: &Path, from: &VaultPath, to: &VaultPath) -> Result<()> {
        let from_abs = root.join(from.as_str());
        let to_abs = root.join(to.as_str());
        if let Some(parent) = to_abs.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from_abs, to_abs).map_err(|err| match err.kind() {
            ErrorKind::NotFound => NotesmithError::NoteNotFound { path: from.clone() },
            _ => err.into(),
        })?;
        Ok(())
    }
}

fn load_note(root: &Path, vault_name: &VaultName, absolute_path: &Path) -> Result<Note> {
    let path = relative_vault_path(root, absolute_path)?;
    let content = read_note_file(absolute_path, &path)?;
    Ok(parse_note(vault_name, &path, &content))
}

fn read_note_file(path: &Path, vault_path: &VaultPath) -> Result<String> {
    fs::read_to_string(path).map_err(|err| match err.kind() {
        ErrorKind::NotFound => NotesmithError::NoteNotFound {
            path: vault_path.clone(),
        },
        _ => err.into(),
    })
}

fn relative_vault_path(root: &Path, absolute_path: &Path) -> Result<VaultPath> {
    absolute_path
        .strip_prefix(root)
        .map(|relative| VaultPath::new(relative.to_string_lossy().replace('\\', "/")))
        .map_err(|err| NotesmithError::Other(err.to_string()))
}

fn should_skip_entry(entry: &DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry.depth() > 0
        && matches!(
            entry.file_name().to_str(),
            Some(".notesmith") | Some(".obsidian")
        )
}

fn is_markdown_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn sibling_temp_path(destination: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("note.md");
    destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            ".{filename}.notesmith-{}-{timestamp}.tmp",
            std::process::id()
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_core::{NotesmithError, VaultEngine, VaultPath, WriteResult};
    use std::{fs, path::Path};
    use tempfile::TempDir;
    use walkdir::WalkDir;

    fn write_file(root: &TempDir, relative: &str, content: &str) {
        let path = root.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn find_entry(root: &Path, name: &str) -> DirEntry {
        WalkDir::new(root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .find(|entry| entry.file_name().to_str() == Some(name))
            .unwrap()
    }

    #[test]
    fn scan_skips_unreadable_note_and_keeps_the_rest() {
        // Per ADR 0009, a single unreadable/malformed note must not abort the
        // whole vault scan (which historically propagated up and killed daemon
        // startup). A non-UTF-8 `.md` file makes `read_to_string` fail; scan
        // must skip+warn it and still return the well-formed notes.
        let root = TempDir::new().unwrap();
        write_file(&root, "good.md", "# Good\n");
        let bad = root.path().join("bad.md");
        fs::write(&bad, [0xff, 0xfe, 0x00, 0x9f, 0x92, 0xa9]).unwrap();

        let engine = NativeVaultEngine;
        let notes = engine.scan(root.path()).unwrap();
        let paths: Vec<_> = notes.iter().map(|note| note.path.as_str()).collect();

        assert_eq!(paths, vec!["good.md"]);
    }

    #[test]
    fn scan_finds_markdown_files() {
        let root = TempDir::new().unwrap();
        write_file(&root, "alpha.md", "# Alpha\n");
        write_file(&root, "nested/beta.MD", "# Beta\n");
        write_file(&root, "nested/ignore.txt", "ignore\n");

        let engine = NativeVaultEngine;
        let notes = engine.scan(root.path()).unwrap();
        let paths: Vec<_> = notes.iter().map(|note| note.path.as_str()).collect();

        assert_eq!(paths, vec!["alpha.md", "nested/beta.MD"]);
    }

    #[test]
    fn scan_skips_notesmith_and_obsidian_dirs() {
        let root = TempDir::new().unwrap();
        write_file(&root, "visible.md", "# Visible\n");
        write_file(&root, ".notesmith/internal.md", "# Internal\n");
        write_file(&root, ".obsidian/workspace.md", "# Workspace\n");

        let engine = NativeVaultEngine;
        let notes = engine.scan(root.path()).unwrap();
        let paths: Vec<_> = notes.iter().map(|note| note.path.as_str()).collect();

        assert_eq!(paths, vec!["visible.md"]);
    }

    #[test]
    fn read_returns_content() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("note.md");
        write_file(&root, path.as_str(), "# Hello\n");

        let engine = NativeVaultEngine;
        let content = engine.read(root.path(), &path).unwrap();

        assert_eq!(content, "# Hello\n");
    }

    #[test]
    fn read_returns_not_found_for_missing() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("missing.md");

        let engine = NativeVaultEngine;
        let err = engine.read(root.path(), &path).unwrap_err();

        assert!(matches!(err, NotesmithError::NoteNotFound { path: missing } if missing == path));
    }

    #[test]
    fn write_creates_file() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("created.md");
        let content = "# Created\n";

        let engine = NativeVaultEngine;
        let result = engine.write(root.path(), &path, None, content).unwrap();

        assert!(
            matches!(result, WriteResult::Written { hash } if hash == blake3::hash(content.as_bytes()).to_hex().to_string())
        );
        assert_eq!(
            fs::read_to_string(root.path().join(path.as_str())).unwrap(),
            content
        );
    }

    #[test]
    fn write_creates_parent_dirs() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("sub/dir/note.md");

        let engine = NativeVaultEngine;
        engine.write(root.path(), &path, None, "nested\n").unwrap();

        assert_eq!(
            fs::read_to_string(root.path().join(path.as_str())).unwrap(),
            "nested\n"
        );
    }

    #[test]
    fn write_conflict_detection_hash_mismatch() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("conflict.md");
        let existing = "old content\n";
        let expected = blake3::hash(b"different content\n").to_hex().to_string();
        let actual = blake3::hash(existing.as_bytes()).to_hex().to_string();
        write_file(&root, path.as_str(), existing);

        let engine = NativeVaultEngine;
        let result = engine
            .write(root.path(), &path, Some(&expected), "new content\n")
            .unwrap();

        assert_eq!(result, WriteResult::Conflict { expected, actual });
        assert_eq!(
            fs::read_to_string(root.path().join(path.as_str())).unwrap(),
            existing
        );
    }

    #[test]
    fn write_conflict_detection_hash_match() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("conflict.md");
        let existing = "old content\n";
        let updated = "new content\n";
        let expected = blake3::hash(existing.as_bytes()).to_hex().to_string();
        write_file(&root, path.as_str(), existing);

        let engine = NativeVaultEngine;
        let result = engine
            .write(root.path(), &path, Some(&expected), updated)
            .unwrap();

        assert_eq!(
            result,
            WriteResult::Written {
                hash: blake3::hash(updated.as_bytes()).to_hex().to_string(),
            }
        );
        assert_eq!(
            fs::read_to_string(root.path().join(path.as_str())).unwrap(),
            updated
        );
    }

    #[test]
    fn write_conflict_detection_file_not_found() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("missing.md");
        let expected = "expected-hash".to_string();

        let engine = NativeVaultEngine;
        let result = engine
            .write(root.path(), &path, Some(&expected), "content\n")
            .unwrap();

        assert_eq!(
            result,
            WriteResult::Conflict {
                expected,
                actual: String::new(),
            }
        );
        assert!(!root.path().join(path.as_str()).exists());
    }

    #[test]
    fn delete_removes_file() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("delete-me.md");
        write_file(&root, path.as_str(), "bye\n");

        let engine = NativeVaultEngine;
        engine.delete(root.path(), &path).unwrap();

        assert!(!root.path().join(path.as_str()).exists());
    }

    #[test]
    fn delete_returns_not_found() {
        let root = TempDir::new().unwrap();
        let path = VaultPath::new("missing.md");

        let engine = NativeVaultEngine;
        let err = engine.delete(root.path(), &path).unwrap_err();

        assert!(matches!(err, NotesmithError::NoteNotFound { path: missing } if missing == path));
    }

    #[test]
    fn move_path_renames_file() {
        let root = TempDir::new().unwrap();
        let from = VaultPath::new("from.md");
        let to = VaultPath::new("to.md");
        write_file(&root, from.as_str(), "moved\n");

        let engine = NativeVaultEngine;
        engine.move_path(root.path(), &from, &to).unwrap();

        assert!(!root.path().join(from.as_str()).exists());
        assert_eq!(
            fs::read_to_string(root.path().join(to.as_str())).unwrap(),
            "moved\n"
        );
    }

    #[test]
    fn move_path_creates_parent_dirs() {
        let root = TempDir::new().unwrap();
        let from = VaultPath::new("from.md");
        let to = VaultPath::new("nested/dir/to.md");
        write_file(&root, from.as_str(), "moved\n");

        let engine = NativeVaultEngine;
        engine.move_path(root.path(), &from, &to).unwrap();

        assert!(!root.path().join(from.as_str()).exists());
        assert_eq!(
            fs::read_to_string(root.path().join(to.as_str())).unwrap(),
            "moved\n"
        );
    }

    #[test]
    fn should_skip_entry_skips_notesmith_and_obsidian_dirs() {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join(".notesmith")).unwrap();
        fs::create_dir_all(root.path().join(".obsidian")).unwrap();
        fs::create_dir_all(root.path().join("notes")).unwrap();

        let notesmith = find_entry(root.path(), ".notesmith");
        let obsidian = find_entry(root.path(), ".obsidian");
        let notes = find_entry(root.path(), "notes");

        assert!(should_skip_entry(&notesmith));
        assert!(should_skip_entry(&obsidian));
        assert!(!should_skip_entry(&notes));
    }

    #[test]
    fn is_markdown_file_checks_extension_case_insensitively() {
        let root = TempDir::new().unwrap();
        let lower = root.path().join("lower.md");
        let upper = root.path().join("upper.MD");
        let text = root.path().join("note.txt");
        let dir = root.path().join("folder.md");
        fs::write(&lower, "lower\n").unwrap();
        fs::write(&upper, "upper\n").unwrap();
        fs::write(&text, "text\n").unwrap();
        fs::create_dir_all(&dir).unwrap();

        assert!(is_markdown_file(&lower));
        assert!(is_markdown_file(&upper));
        assert!(!is_markdown_file(&text));
        assert!(!is_markdown_file(&dir));
    }
}
