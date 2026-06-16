//! notesmith-prompts: static custom-prompt storage and merge logic.
//!
//! A *custom prompt* is a named, **static** saved instruction string. There are
//! two sources:
//!
//! * **Defaults** — built-in prompts seeded into the daemon config dir
//!   (`<config>/notesmith/prompts/*.md`) on first run. Users may edit them.
//! * **Vault overrides** — markdown files in a vault's `_prompts/` folder.
//!
//! The two sets are merged by `name`; a vault entry overrides a config default
//! with the same name. The merged list is served over HTTP to the chat UI,
//! which sends the prompt body verbatim to the user's agent.
//!
//! # File format
//!
//! Each prompt is a single markdown file with YAML frontmatter:
//!
//! ```text
//! ---
//! name: summarize
//! description: Concise summary of the current note.
//! ---
//! Provide a concise summary of the current note.
//! ```
//!
//! * `name` — the prompt's stable identifier (falls back to the file stem).
//! * `description` — a short human-readable label (optional).
//! * The markdown **body** is the prompt text, currently sent verbatim.
//!
//! ## Forward compatibility: variables
//!
//! Variable substitution (`{{selection}}`, `{{title}}`, …) is intentionally
//! **not** implemented in this slice. The format reserves a frontmatter field
//! named `variables` for a future list of declared placeholders; because
//! frontmatter is parsed into a generic map, files that already declare
//! `variables` are accepted and the extra field is ignored today. Adding
//! `{{variable}}` interpolation later is therefore a non-breaking change: the
//! body is stored as-is and a future renderer can expand placeholders without
//! altering existing files or the response shape.
//!
//! # Resilience (ADR 0009)
//!
//! Prompt `.md` files are untrusted input. A malformed file (bad YAML, missing
//! frontmatter, non-UTF-8 bytes) never aborts loading: it is logged at `WARN`
//! and skipped. A missing prompts directory yields an empty list, never an
//! error.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Reserved frontmatter field for future `{{variable}}` declarations. Present so
/// the format is forward-compatible; parsed-and-ignored today.
pub const RESERVED_VARIABLES_FIELD: &str = "variables";

/// Vault folder (relative to the vault root) holding user prompt overrides.
pub const VAULT_PROMPTS_DIRNAME: &str = "_prompts";

/// Where a prompt came from. Serialized lowercase to match the HTTP contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptSource {
    /// A built-in default seeded in the daemon config dir.
    Default,
    /// A user-provided override from the vault `_prompts/` folder.
    Vault,
}

/// A single static custom prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    /// Stable identifier, unique within the merged set.
    pub name: String,
    /// Short human-readable description (may be empty).
    pub description: String,
    /// The prompt text, sent verbatim to the agent.
    pub body: String,
    /// Whether this entry is a config default or a vault override.
    pub source: PromptSource,
}

/// The built-in default prompt set: `(name, description, body)`.
///
/// Mirrors the default slash-command set (issue #194). Each body is a concise,
/// self-contained static instruction.
pub const DEFAULT_PROMPTS: &[(&str, &str, &str)] = &[
    (
        "summarize",
        "Concise summary of the current note.",
        "Provide a concise summary of the current note, capturing the key points in a few sentences.",
    ),
    (
        "rewrite",
        "Rewrite for clarity and flow.",
        "Rewrite the current note to improve clarity, flow, and concision while preserving its meaning.",
    ),
    (
        "outline",
        "Structured outline of the note.",
        "Produce a structured outline of the current note using nested bullet points.",
    ),
    (
        "fix",
        "Fix spelling and grammar.",
        "Fix spelling, grammar, and punctuation in the current note without changing its meaning.",
    ),
    (
        "tags",
        "Suggest relevant tags.",
        "Suggest a concise set of relevant tags for the current note.",
    ),
    (
        "links",
        "Suggest wikilinks to related notes.",
        "Identify concepts in the current note that could link to other notes and suggest wikilinks.",
    ),
    (
        "daily",
        "Draft today's daily note.",
        "Draft a daily note with today's priorities, scheduled tasks, and a short reflection prompt.",
    ),
    (
        "new",
        "Draft a new note from an idea.",
        "Help me draft a new note from the topic or idea I provide.",
    ),
    (
        "ask",
        "Answer a question using the vault.",
        "Answer my question using the current note and the rest of the vault as context.",
    ),
];

/// Resolve the daemon's default-prompts directory:
/// `<config>/notesmith/prompts`, a sibling of `config.toml`.
///
/// Honours `XDG_CONFIG_HOME`, falling back to the platform config dir, matching
/// [`notesmith_config::GlobalConfig::default_path`].
pub fn default_prompts_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .map(|dir| dir.join("notesmith").join("prompts"))
}

/// The `_prompts/` directory for a given vault root.
pub fn vault_prompts_dir(vault_root: &Path) -> PathBuf {
    vault_root.join(VAULT_PROMPTS_DIRNAME)
}

/// Serialize a built-in default into prompt-file markdown.
fn default_prompt_file_contents(name: &str, description: &str, body: &str) -> String {
    // `serde_yaml` quotes/escapes the values safely so descriptions with
    // colons or other YAML-significant characters round-trip correctly.
    let mut fm = serde_yaml::Mapping::new();
    fm.insert("name".into(), name.into());
    fm.insert("description".into(), description.into());
    let yaml = serde_yaml::to_string(&fm).unwrap_or_else(|_| {
        // Unreachable for string maps, but never panic on file-bound data.
        format!("name: {name}\ndescription: {description}\n")
    });
    format!("---\n{}---\n{}\n", yaml, body.trim_end())
}

/// Seed the built-in default prompts into `dir`, creating it if needed.
///
/// Idempotent: a file that already exists is left untouched, so user edits to
/// the defaults survive restarts. Returns the number of files newly written.
/// Best-effort — individual write failures are logged and skipped rather than
/// aborting the whole pass.
pub fn seed_default_prompts(dir: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let mut written = 0;
    for (name, description, body) in DEFAULT_PROMPTS {
        let path = dir.join(format!("{name}.md"));
        if path.exists() {
            continue;
        }
        let contents = default_prompt_file_contents(name, description, body);
        match std::fs::write(&path, contents) {
            Ok(()) => written += 1,
            Err(error) => {
                tracing::warn!(
                    note = %path.display(),
                    stage = "seed_prompt",
                    reason = %error,
                    "failed to write default prompt; skipping",
                );
            }
        }
    }
    Ok(written)
}

/// Parse one prompt file's raw contents. Returns `None` (after logging) for
/// untrusted input we cannot turn into a usable prompt.
fn parse_prompt(path: &Path, contents: &str, source: PromptSource) -> Option<Prompt> {
    let (frontmatter, body) = notesmith_vault::extract_frontmatter(contents);
    let Some(frontmatter) = frontmatter else {
        tracing::warn!(
            note = %path.display(),
            stage = "parse_prompt",
            reason = "missing frontmatter",
            "skipping prompt file",
        );
        return None;
    };

    // ADR 0009: never `?`-propagate a YAML parse of note-derived bytes. Fall
    // back to skipping the file on malformed frontmatter.
    let frontmatter: notesmith_core::Frontmatter = match serde_yaml::from_str(&frontmatter) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!(
                note = %path.display(),
                stage = "parse_prompt",
                reason = %error,
                "malformed prompt frontmatter; skipping",
            );
            return None;
        }
    };

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let name = frontmatter
        .get_string("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| stem.to_string());
    if name.is_empty() {
        tracing::warn!(
            note = %path.display(),
            stage = "parse_prompt",
            reason = "empty name",
            "skipping prompt file",
        );
        return None;
    }

    let description = frontmatter
        .get_string("description")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Some(Prompt {
        name,
        description,
        body: body.trim().to_string(),
        source,
    })
}

/// Load every `*.md` prompt in `dir`, tagging each with `source`.
///
/// Resilient (ADR 0009): a missing directory yields an empty list; a single
/// malformed or unreadable file is logged and skipped. Results are sorted by
/// `name` for a stable response.
pub fn load_prompts_from_dir(dir: &Path, source: PromptSource) -> Vec<Prompt> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    note = %dir.display(),
                    stage = "load_prompts",
                    reason = %error,
                    "could not read prompts directory; treating as empty",
                );
            }
            return Vec::new();
        }
    };

    let mut prompts = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(stage = "load_prompts", reason = %error, "skipping unreadable dir entry");
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                tracing::warn!(
                    note = %path.display(),
                    stage = "load_prompts",
                    reason = %error,
                    "could not read prompt file; skipping",
                );
                continue;
            }
        };
        if let Some(prompt) = parse_prompt(&path, &contents, source) {
            prompts.push(prompt);
        }
    }
    prompts.sort_by(|a, b| a.name.cmp(&b.name));
    prompts
}

/// Merge config `defaults` with vault `overrides`, where a vault entry wins on a
/// `name` collision. The result is sorted by `name`.
pub fn merge_prompts(defaults: Vec<Prompt>, overrides: Vec<Prompt>) -> Vec<Prompt> {
    use std::collections::BTreeMap;
    let mut merged: BTreeMap<String, Prompt> = BTreeMap::new();
    for prompt in defaults {
        merged.insert(prompt.name.clone(), prompt);
    }
    for prompt in overrides {
        merged.insert(prompt.name.clone(), prompt);
    }
    merged.into_values().collect()
}

/// Load the merged prompt set for a vault: config-dir defaults overridden by the
/// vault's `_prompts/` entries. Both sources are resilient to missing dirs and
/// malformed files.
pub fn load_merged_prompts(defaults_dir: &Path, vault_root: &Path) -> Vec<Prompt> {
    let defaults = load_prompts_from_dir(defaults_dir, PromptSource::Default);
    let overrides = load_prompts_from_dir(&vault_prompts_dir(vault_root), PromptSource::Vault);
    merge_prompts(defaults, overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, contents: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn seeds_all_defaults_and_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("prompts");

        let first = seed_default_prompts(&dir).unwrap();
        assert_eq!(first, DEFAULT_PROMPTS.len());
        for (name, _, _) in DEFAULT_PROMPTS {
            assert!(dir.join(format!("{name}.md")).exists(), "missing {name}");
        }

        // Second run writes nothing (survives restarts; preserves user edits).
        let second = seed_default_prompts(&dir).unwrap();
        assert_eq!(second, 0);
    }

    #[test]
    fn seeded_defaults_are_loadable_and_well_formed() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("prompts");
        seed_default_prompts(&dir).unwrap();

        let loaded = load_prompts_from_dir(&dir, PromptSource::Default);
        assert_eq!(loaded.len(), DEFAULT_PROMPTS.len());
        let summarize = loaded.iter().find(|p| p.name == "summarize").unwrap();
        assert_eq!(summarize.source, PromptSource::Default);
        assert!(!summarize.description.is_empty());
        assert!(summarize.body.contains("concise summary"));
    }

    #[test]
    fn parses_frontmatter_name_description_and_body() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "greet.md",
            "---\nname: greet\ndescription: Say hi\n---\nWrite a friendly greeting.",
        );
        let prompts = load_prompts_from_dir(dir, PromptSource::Vault);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "greet");
        assert_eq!(prompts[0].description, "Say hi");
        assert_eq!(prompts[0].body, "Write a friendly greeting.");
        assert_eq!(prompts[0].source, PromptSource::Vault);
    }

    #[test]
    fn name_falls_back_to_file_stem() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "from-stem.md",
            "---\ndescription: No name field\n---\nBody.",
        );
        let prompts = load_prompts_from_dir(dir, PromptSource::Vault);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "from-stem");
    }

    #[test]
    fn reserved_variables_field_is_accepted_and_ignored() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "future.md",
            "---\nname: future\ndescription: forward compatible\nvariables:\n  - selection\n  - title\n---\nUse {{selection}} from {{title}}.",
        );
        let prompts = load_prompts_from_dir(dir, PromptSource::Vault);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "future");
        // Body is preserved verbatim so a future renderer can expand it.
        assert_eq!(prompts[0].body, "Use {{selection}} from {{title}}.");
    }

    #[test]
    fn malformed_files_are_skipped_not_fatal() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        // Broken YAML frontmatter.
        write(
            dir,
            "bad-yaml.md",
            "---\nname: : ][\n  bad: indent\n---\nBody",
        );
        // No frontmatter at all.
        write(dir, "no-fm.md", "# Just a heading\nBody only");
        // A good one alongside the bad ones.
        write(
            dir,
            "ok.md",
            "---\nname: ok\ndescription: fine\n---\nGood body.",
        );

        let prompts = load_prompts_from_dir(dir, PromptSource::Vault);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "ok");
    }

    #[test]
    fn missing_directory_yields_empty_list() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(load_prompts_from_dir(&missing, PromptSource::Default).is_empty());
    }

    #[test]
    fn vault_entry_overrides_default_by_name() {
        let defaults = vec![
            Prompt {
                name: "summarize".into(),
                description: "default".into(),
                body: "default summary".into(),
                source: PromptSource::Default,
            },
            Prompt {
                name: "fix".into(),
                description: "default fix".into(),
                body: "default fix body".into(),
                source: PromptSource::Default,
            },
        ];
        let overrides = vec![
            Prompt {
                name: "summarize".into(),
                description: "custom".into(),
                body: "custom summary".into(),
                source: PromptSource::Vault,
            },
            Prompt {
                name: "extra".into(),
                description: "new".into(),
                body: "extra body".into(),
                source: PromptSource::Vault,
            },
        ];

        let merged = merge_prompts(defaults, overrides);
        assert_eq!(merged.len(), 3);

        let summarize = merged.iter().find(|p| p.name == "summarize").unwrap();
        assert_eq!(summarize.source, PromptSource::Vault);
        assert_eq!(summarize.body, "custom summary");

        let fix = merged.iter().find(|p| p.name == "fix").unwrap();
        assert_eq!(fix.source, PromptSource::Default);

        assert!(merged.iter().any(|p| p.name == "extra"));
        // Sorted by name.
        let names: Vec<_> = merged.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["extra", "fix", "summarize"]);
    }

    #[test]
    fn load_merged_prompts_combines_defaults_and_vault() {
        let tmp = TempDir::new().unwrap();
        let defaults_dir = tmp.path().join("config-prompts");
        seed_default_prompts(&defaults_dir).unwrap();

        let vault_root = tmp.path().join("vault");
        let vault_prompts = vault_prompts_dir(&vault_root);
        write(
            &vault_prompts,
            "summarize.md",
            "---\nname: summarize\ndescription: vault override\n---\nVault summary.",
        );
        write(
            &vault_prompts,
            "custom.md",
            "---\nname: custom\ndescription: only here\n---\nCustom body.",
        );

        let merged = load_merged_prompts(&defaults_dir, &vault_root);
        // All defaults plus the one new vault-only prompt.
        assert_eq!(merged.len(), DEFAULT_PROMPTS.len() + 1);

        let summarize = merged.iter().find(|p| p.name == "summarize").unwrap();
        assert_eq!(summarize.source, PromptSource::Vault);
        assert_eq!(summarize.body, "Vault summary.");

        let custom = merged.iter().find(|p| p.name == "custom").unwrap();
        assert_eq!(custom.source, PromptSource::Vault);
    }
}
