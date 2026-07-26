//! notesmith-kit: installable vault kits — the blessed configurations, shipped.
//!
//! A **kit** is a set of files (`.notesmith/` config, templates, dashboards)
//! plus a folder skeleton that turns an empty directory into a working vault.
//! The Work Notes kit's bytes are the same ones the `golden-vault/` fixture
//! runs its tests against (enforced by `tests/kit_matches_golden_vault.rs`), so
//! what gets installed is what has been proven to work.
//!
//! Applying is **non-destructive**: an existing file is reported as skipped and
//! left alone. Nothing here needs a running daemon.

use std::path::{Path, PathBuf};

mod work_notes;

/// Placeholder substituted with the vault's name when a kit file is written.
const VAULT_NAME_PLACEHOLDER: &str = "{{ vault_name }}";

#[derive(Debug, thiserror::Error)]
pub enum KitError {
    #[error("unknown kit '{id}' (available: {available})")]
    UnknownKit { id: String, available: String },
    #[error("vault root is not a directory: {path}")]
    NotADirectory { path: PathBuf },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What an [`Kit::apply`] did, so callers can report honestly rather than
/// claiming to have written files that were already there.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    /// Vault-relative paths written (or, under `dry_run`, that would be).
    pub written: Vec<String>,
    /// Vault-relative paths left untouched because they already exist.
    pub skipped: Vec<String>,
    /// Vault-relative folders created.
    pub created_dirs: Vec<String>,
    /// Whether this was a preview.
    pub dry_run: bool,
}

impl ApplyReport {
    pub fn is_noop(&self) -> bool {
        self.written.is_empty() && self.created_dirs.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ApplyOptions {
    /// Value substituted for `{{ vault_name }}` in kit files.
    pub vault_name: String,
    /// Overwrite files that already exist.
    pub force: bool,
    /// Report what would happen without touching the filesystem.
    pub dry_run: bool,
}

impl ApplyOptions {
    pub fn for_vault(vault_name: impl Into<String>) -> Self {
        Self {
            vault_name: vault_name.into(),
            force: false,
            dry_run: false,
        }
    }

    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

/// A file a kit installs: vault-relative path, and its contents.
pub type KitFile = (&'static str, &'static str);

#[derive(Debug, Clone, Copy)]
pub struct Kit {
    id: &'static str,
    description: &'static str,
    files: &'static [KitFile],
    folders: &'static [&'static str],
}

const KITS: &[Kit] = &[Kit {
    id: work_notes::ID,
    description: work_notes::DESCRIPTION,
    files: work_notes::FILES,
    folders: work_notes::FOLDERS,
}];

impl Kit {
    /// Every built-in kit.
    pub fn all() -> &'static [Kit] {
        KITS
    }

    /// Look up a kit by id.
    pub fn builtin(id: &str) -> Option<Kit> {
        KITS.iter().copied().find(|kit| kit.id == id)
    }

    /// Look up a kit by id, or fail with the list of valid ids.
    pub fn require(id: &str) -> Result<Kit, KitError> {
        Self::builtin(id).ok_or_else(|| KitError::UnknownKit {
            id: id.to_string(),
            available: KITS.iter().map(|kit| kit.id).collect::<Vec<_>>().join(", "),
        })
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    pub fn description(&self) -> &'static str {
        self.description
    }

    /// The files this kit installs, vault-relative.
    pub fn files(&self) -> &'static [KitFile] {
        self.files
    }

    /// The folder skeleton this kit creates.
    pub fn folders(&self) -> &'static [&'static str] {
        self.folders
    }

    /// Write the kit into `vault_root`.
    ///
    /// Existing files are skipped unless [`ApplyOptions::force`] is set, so this
    /// is safe to run against a populated vault. Parent directories are created
    /// as needed.
    pub fn apply(
        &self,
        vault_root: &Path,
        options: &ApplyOptions,
    ) -> Result<ApplyReport, KitError> {
        if vault_root.exists() && !vault_root.is_dir() {
            return Err(KitError::NotADirectory {
                path: vault_root.to_path_buf(),
            });
        }

        let mut report = ApplyReport {
            dry_run: options.dry_run,
            ..Default::default()
        };

        for folder in self.folders {
            let path = vault_root.join(folder);
            if path.is_dir() {
                continue;
            }
            report.created_dirs.push((*folder).to_string());
            if !options.dry_run {
                std::fs::create_dir_all(&path).map_err(|source| KitError::Write {
                    path: path.clone(),
                    source,
                })?;
            }
        }

        for (relative, contents) in self.files {
            let path = vault_root.join(relative);
            if path.exists() && !options.force {
                report.skipped.push((*relative).to_string());
                continue;
            }

            report.written.push((*relative).to_string());
            if options.dry_run {
                continue;
            }

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|source| KitError::Write {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let rendered = contents.replace(VAULT_NAME_PLACEHOLDER, &options.vault_name);
            std::fs::write(&path, rendered).map_err(|source| KitError::Write {
                path: path.clone(),
                source,
            })?;
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_lists_available_kits_when_the_id_is_unknown() {
        let error = Kit::require("nope").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("unknown kit 'nope'"), "{message}");
        assert!(message.contains("work-notes"), "{message}");
    }

    #[test]
    fn kit_paths_are_relative_and_well_formed() {
        for kit in Kit::all() {
            for (relative, contents) in kit.files() {
                assert!(
                    !relative.starts_with('/') && !relative.contains(".."),
                    "{relative} must be a vault-relative path"
                );
                assert!(!contents.is_empty(), "{relative} is empty");
            }
            for folder in kit.folders() {
                assert!(!folder.starts_with('/') && !folder.contains(".."));
            }
        }
    }

    #[test]
    fn applying_to_a_file_path_is_an_error_not_a_panic() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-a-dir");
        std::fs::write(&file, "x").unwrap();

        let error = Kit::builtin("work-notes")
            .unwrap()
            .apply(&file, &ApplyOptions::for_vault("x"))
            .unwrap_err();

        assert!(matches!(error, KitError::NotADirectory { .. }));
    }

    #[test]
    fn the_vault_name_placeholder_only_appears_where_it_is_meant_to() {
        let occurrences: Vec<&str> = Kit::builtin("work-notes")
            .unwrap()
            .files()
            .iter()
            .filter(|(_, contents)| contents.contains(VAULT_NAME_PLACEHOLDER))
            .map(|(path, _)| *path)
            .collect();

        assert_eq!(
            occurrences,
            vec![".notesmith/vault.toml"],
            "an unsubstituted placeholder would ship as literal text"
        );
    }
}
