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
            .map(|entry| load_note(root, &vault_name, entry.path()))
            .collect::<Result<Vec<_>>>()?;

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
    let parsed = parse_note(&content, vault_name, &path);

    Ok(Note {
        vault: vault_name.clone(),
        path,
        frontmatter: parsed.frontmatter,
        raw_frontmatter: parsed.raw_frontmatter,
        body: parsed.body,
        tasks: parsed.tasks,
        links: parsed.links,
        inline_fields: parsed.inline_fields,
        blocks: parsed.blocks,
        hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
    })
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
