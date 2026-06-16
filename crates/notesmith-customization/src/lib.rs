//! Discovery of user-authored **customizations** — custom agents (personas),
//! skills, and instructions — for the chat UI (issue #210, [ADR 0016]).
//!
//! [ADR 0016]: ../../../docs/adr/0016-customization-discovery-and-routing.md
//!
//! # Layout
//!
//! Customizations are markdown files under two scopes, each with three subdirs:
//!
//! | Scope   | Base                                | Subdirs                              |
//! |---------|-------------------------------------|--------------------------------------|
//! | Project | `<vault>/.notesmith/`               | `agents/` · `skills/` · `instructions/` |
//! | Global  | `~/.config/notesmith/` (XDG-aware)  | `agents/` · `skills/` · `instructions/` |
//!
//! Each item is a single `*.md` file with optional YAML frontmatter and a
//! markdown body. The file **stem** is the item `id`; frontmatter `name` and
//! `description` are optional (name falls back to the stem). For an *agent*
//! (persona) the frontmatter may also carry `backend` (a discovered ACP agent id)
//! and `model`; the body is the system/preamble prompt.
//!
//! # Resilience (ADR 0009)
//!
//! All `.md` content is untrusted. A missing directory yields an empty list; a
//! single malformed or unreadable file is logged at `WARN` and skipped. Parsing
//! never `?`-propagates a YAML error above the per-file boundary and never
//! panics on file-derived data.
//!
//! # Precedence
//!
//! Project entries override global entries **by id**, per type (a project file
//! and a global file with the same stem collapse to the project one). When an id
//! exists in only one scope it is shown as-is.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The `.notesmith/` subdir holding customizations within a vault or the global
/// config dir.
pub const AGENTS_SUBDIR: &str = "agents";
/// The skills subdir.
pub const SKILLS_SUBDIR: &str = "skills";
/// The instructions subdir.
pub const INSTRUCTIONS_SUBDIR: &str = "instructions";

/// Where a customization came from. Serialized lowercase for the HTTP contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A vault-local file under `<vault>/.notesmith/`.
    Project,
    /// A user-global file under `~/.config/notesmith/`.
    Global,
}

/// A discovered custom agent (persona): a preamble prompt that runs on top of an
/// ACP agent backend (ADR 0016 decision 1). Not a separate CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomAgent {
    /// Stable identifier (the file stem), unique within the merged set.
    pub id: String,
    /// Human-readable name (falls back to `id`).
    pub name: String,
    /// Short description (may be empty).
    pub description: String,
    /// Optional ACP backend agent id (`copilot`/`claude`/…). `None` = use the
    /// session's currently-selected agent.
    pub backend: Option<String>,
    /// Optional model id to request for this persona.
    pub model: Option<String>,
    /// The persona's system/preamble prompt (the markdown body).
    pub body: String,
    /// Whether this entry is a project or global file.
    pub source: Source,
}

/// A discovered skill: reusable instructions the agent can load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    /// Stable identifier (the file stem).
    pub id: String,
    /// Human-readable name (falls back to `id`).
    pub name: String,
    /// Short description (may be empty).
    pub description: String,
    /// The skill body.
    pub body: String,
    /// Whether this entry is a project or global file.
    pub source: Source,
}

/// A discovered instruction: always-applied guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instruction {
    /// Stable identifier (the file stem).
    pub id: String,
    /// Human-readable name (falls back to `id`).
    pub name: String,
    /// Short description (may be empty).
    pub description: String,
    /// The instruction body.
    pub body: String,
    /// Whether this entry is a project or global file.
    pub source: Source,
}

/// The full set of discovered customizations for a vault.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customizations {
    /// Custom agents (personas), project overriding global by id.
    pub agents: Vec<CustomAgent>,
    /// Skills, project overriding global by id.
    pub skills: Vec<Skill>,
    /// Instructions, project overriding global by id.
    pub instructions: Vec<Instruction>,
}

/// The global customization base dir: `<config>/notesmith`, honouring
/// `XDG_CONFIG_HOME` and matching [`notesmith_config::GlobalConfig::default_path`].
pub fn global_base_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .map(|dir| dir.join("notesmith"))
}

/// The project customization base dir for a vault: `<vault>/.notesmith`.
pub fn vault_base_dir(vault_root: &Path) -> PathBuf {
    vault_root.join(".notesmith")
}

/// A file parsed into its common fields. Type-specific extras (agent backend,
/// model) are read from `frontmatter` by the caller.
struct ParsedFile {
    id: String,
    name: String,
    description: String,
    body: String,
    frontmatter: notesmith_core::Frontmatter,
}

/// Parse one customization file. Returns `None` (after logging) for untrusted
/// input we cannot turn into a usable item. Missing frontmatter is tolerated
/// (the whole file becomes the body); only a *malformed* frontmatter block is
/// skipped.
fn parse_file(path: &Path, contents: &str) -> Option<ParsedFile> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if stem.is_empty() {
        tracing::warn!(
            note = %path.display(),
            stage = "parse_customization",
            reason = "empty file stem",
            "skipping customization file",
        );
        return None;
    }

    let (raw_frontmatter, body) = notesmith_vault::extract_frontmatter(contents);

    // ADR 0009: never `?`-propagate a YAML parse of file-derived bytes. A
    // *malformed* frontmatter block is skipped; an *absent* one degrades to an
    // empty map so the body still becomes a usable item.
    let frontmatter = match raw_frontmatter {
        Some(raw) => match serde_yaml::from_str::<notesmith_core::Frontmatter>(&raw) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(
                    note = %path.display(),
                    stage = "parse_customization",
                    reason = %error,
                    "malformed frontmatter; skipping",
                );
                return None;
            }
        },
        None => notesmith_core::Frontmatter::default(),
    };

    let name = frontmatter
        .get_string("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| stem.clone());
    let description = frontmatter
        .get_string("description")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Some(ParsedFile {
        id: stem,
        name,
        description,
        body: body.trim().to_string(),
        frontmatter,
    })
}

/// Read every `*.md` file in `dir`, parsing each with `make`. A `None` from
/// `make` (or from [`parse_file`]) drops that file. Resilient (ADR 0009): a
/// missing dir yields an empty vec; unreadable entries/files are logged and
/// skipped. Results are sorted by `id`.
fn read_md_dir<T, F>(dir: &Path, make: F) -> Vec<T>
where
    F: Fn(ParsedFile) -> T,
{
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    note = %dir.display(),
                    stage = "read_customizations",
                    reason = %error,
                    "could not read customization directory; treating as empty",
                );
            }
            return Vec::new();
        }
    };

    let mut items = Vec::new();
    let mut ids = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(stage = "read_customizations", reason = %error, "skipping unreadable dir entry");
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
                    stage = "read_customizations",
                    reason = %error,
                    "could not read customization file; skipping",
                );
                continue;
            }
        };
        if let Some(parsed) = parse_file(&path, &contents) {
            ids.push(parsed.id.clone());
            items.push(make(parsed));
        }
    }
    // Sort by id for a stable response (ids captured alongside items).
    let mut paired: Vec<(String, T)> = ids.into_iter().zip(items).collect();
    paired.sort_by(|a, b| a.0.cmp(&b.0));
    paired.into_iter().map(|(_, item)| item).collect()
}

fn make_agent(source: Source) -> impl Fn(ParsedFile) -> CustomAgent {
    move |p| CustomAgent {
        backend: p
            .frontmatter
            .get_string("backend")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        model: p
            .frontmatter
            .get_string("model")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        id: p.id,
        name: p.name,
        description: p.description,
        body: p.body,
        source,
    }
}

fn make_skill(source: Source) -> impl Fn(ParsedFile) -> Skill {
    move |p| Skill {
        id: p.id,
        name: p.name,
        description: p.description,
        body: p.body,
        source,
    }
}

fn make_instruction(source: Source) -> impl Fn(ParsedFile) -> Instruction {
    move |p| Instruction {
        id: p.id,
        name: p.name,
        description: p.description,
        body: p.body,
        source,
    }
}

/// Load custom agents from a single `agents/` directory.
pub fn load_agents_from_dir(dir: &Path, source: Source) -> Vec<CustomAgent> {
    read_md_dir(dir, make_agent(source))
}

/// Load skills from a single `skills/` directory.
pub fn load_skills_from_dir(dir: &Path, source: Source) -> Vec<Skill> {
    read_md_dir(dir, make_skill(source))
}

/// Load instructions from a single `instructions/` directory.
pub fn load_instructions_from_dir(dir: &Path, source: Source) -> Vec<Instruction> {
    read_md_dir(dir, make_instruction(source))
}

/// Merge `global` with `project`, where a project entry wins on an `id`
/// collision. `id_of` extracts the id; the result is sorted by id.
fn merge_by_id<T, F>(global: Vec<T>, project: Vec<T>, id_of: F) -> Vec<T>
where
    F: Fn(&T) -> String,
{
    use std::collections::BTreeMap;
    let mut merged: BTreeMap<String, T> = BTreeMap::new();
    for item in global {
        merged.insert(id_of(&item), item);
    }
    for item in project {
        merged.insert(id_of(&item), item);
    }
    merged.into_values().collect()
}

/// Discover the merged customization set for a vault: global entries under
/// `~/.config/notesmith/` overridden by the vault's `.notesmith/` entries, per
/// type, by id. Both sources are resilient to missing dirs and malformed files.
pub fn discover(vault_root: &Path) -> Customizations {
    discover_in(global_base_dir().as_deref(), vault_root)
}

/// Like [`discover`] but with an explicit `global_base` (the `~/.config/notesmith`
/// equivalent), so callers and tests can isolate the global scope. `None`
/// disables global discovery.
pub fn discover_in(global_base: Option<&Path>, vault_root: &Path) -> Customizations {
    let global = global_base.map(Path::to_path_buf);
    let project = vault_base_dir(vault_root);

    let load = |base: Option<&Path>, sub: &str| base.map(|b| b.join(sub));

    let agents = merge_by_id(
        load(global.as_deref(), AGENTS_SUBDIR)
            .map(|d| load_agents_from_dir(&d, Source::Global))
            .unwrap_or_default(),
        load_agents_from_dir(&project.join(AGENTS_SUBDIR), Source::Project),
        |a| a.id.clone(),
    );
    let skills = merge_by_id(
        load(global.as_deref(), SKILLS_SUBDIR)
            .map(|d| load_skills_from_dir(&d, Source::Global))
            .unwrap_or_default(),
        load_skills_from_dir(&project.join(SKILLS_SUBDIR), Source::Project),
        |s| s.id.clone(),
    );
    let instructions = merge_by_id(
        load(global.as_deref(), INSTRUCTIONS_SUBDIR)
            .map(|d| load_instructions_from_dir(&d, Source::Global))
            .unwrap_or_default(),
        load_instructions_from_dir(&project.join(INSTRUCTIONS_SUBDIR), Source::Project),
        |i| i.id.clone(),
    );

    Customizations {
        agents,
        skills,
        instructions,
    }
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
    fn parses_agent_frontmatter_backend_and_model() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "researcher.md",
            "---\nname: Researcher\ndescription: Deep research\nbackend: copilot\nmodel: gpt-4o\n---\nYou are a meticulous researcher.",
        );
        let agents = load_agents_from_dir(dir, Source::Project);
        assert_eq!(agents.len(), 1);
        let a = &agents[0];
        assert_eq!(a.id, "researcher");
        assert_eq!(a.name, "Researcher");
        assert_eq!(a.description, "Deep research");
        assert_eq!(a.backend.as_deref(), Some("copilot"));
        assert_eq!(a.model.as_deref(), Some("gpt-4o"));
        assert_eq!(a.body, "You are a meticulous researcher.");
        assert_eq!(a.source, Source::Project);
    }

    #[test]
    fn name_falls_back_to_file_stem() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "editor.md", "Just a body, no frontmatter.");
        let agents = load_agents_from_dir(tmp.path(), Source::Project);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "editor");
        assert_eq!(agents[0].name, "editor");
        assert_eq!(agents[0].backend, None);
        assert_eq!(agents[0].body, "Just a body, no frontmatter.");
    }

    #[test]
    fn malformed_frontmatter_is_skipped_not_fatal() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        // Broken YAML (unbalanced bracket) between fences.
        write(dir, "broken.md", "---\nname: [unclosed\n---\nbody");
        write(dir, "ok.md", "---\nname: Fine\n---\nbody");
        let agents = load_agents_from_dir(dir, Source::Project);
        assert_eq!(agents.len(), 1, "malformed file should be skipped");
        assert_eq!(agents[0].name, "Fine");
    }

    #[test]
    fn missing_directory_is_empty() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(load_agents_from_dir(&missing, Source::Project).is_empty());
        assert!(load_skills_from_dir(&missing, Source::Project).is_empty());
        assert!(load_instructions_from_dir(&missing, Source::Project).is_empty());
    }

    #[test]
    fn non_md_files_are_ignored() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        write(dir, "note.txt", "not markdown");
        write(dir, "skill.md", "---\nname: S\n---\nbody");
        let skills = load_skills_from_dir(dir, Source::Project);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "S");
    }

    #[test]
    fn results_are_sorted_by_id() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        write(dir, "zebra.md", "body");
        write(dir, "alpha.md", "body");
        write(dir, "mango.md", "body");
        let skills = load_skills_from_dir(dir, Source::Project);
        let ids: Vec<_> = skills.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "mango", "zebra"]);
    }

    #[test]
    fn project_overrides_global_by_id() {
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global").join(AGENTS_SUBDIR);
        let project = tmp.path().join("project").join(AGENTS_SUBDIR);
        write(&global, "shared.md", "---\nname: Global\n---\nglobal body");
        write(&global, "global-only.md", "---\nname: GlobalOnly\n---\nx");
        write(
            &project,
            "shared.md",
            "---\nname: Project\n---\nproject body",
        );
        write(
            &project,
            "project-only.md",
            "---\nname: ProjectOnly\n---\ny",
        );

        let g = load_agents_from_dir(&global, Source::Global);
        let p = load_agents_from_dir(&project, Source::Project);
        let merged = merge_by_id(g, p, |a| a.id.clone());

        let ids: Vec<_> = merged.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["global-only", "project-only", "shared"]);
        let shared = merged.iter().find(|a| a.id == "shared").unwrap();
        assert_eq!(shared.name, "Project", "project must win the collision");
        assert_eq!(shared.source, Source::Project);
    }

    #[test]
    fn discover_merges_all_three_types() {
        let vault = TempDir::new().unwrap();
        let base = vault.path().join(".notesmith");
        write(&base.join(AGENTS_SUBDIR), "a.md", "---\nname: A\n---\nx");
        write(&base.join(SKILLS_SUBDIR), "s.md", "---\nname: S\n---\nx");
        write(
            &base.join(INSTRUCTIONS_SUBDIR),
            "i.md",
            "---\nname: I\n---\nx",
        );

        // Isolate the global scope to an empty temp dir so the test is hermetic.
        let global = TempDir::new().unwrap();
        let found = discover_in(Some(global.path()), vault.path());
        assert_eq!(found.agents.len(), 1);
        assert_eq!(found.agents[0].name, "A");
        assert_eq!(found.skills.len(), 1);
        assert_eq!(found.skills[0].name, "S");
        assert_eq!(found.instructions.len(), 1);
        assert_eq!(found.instructions[0].name, "I");
    }

    #[test]
    fn discover_in_project_overrides_global() {
        let global = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        write(
            &global.path().join(AGENTS_SUBDIR),
            "shared.md",
            "---\nname: Global\n---\nx",
        );
        write(
            &vault.path().join(".notesmith").join(AGENTS_SUBDIR),
            "shared.md",
            "---\nname: Project\n---\nx",
        );
        let found = discover_in(Some(global.path()), vault.path());
        assert_eq!(found.agents.len(), 1);
        assert_eq!(found.agents[0].name, "Project");
        assert_eq!(found.agents[0].source, Source::Project);
    }

    #[test]
    fn empty_vault_discovers_nothing() {
        let vault = TempDir::new().unwrap();
        let global = TempDir::new().unwrap();
        let found = discover_in(Some(global.path()), vault.path());
        assert!(found.agents.is_empty());
        assert!(found.skills.is_empty());
        assert!(found.instructions.is_empty());
    }
}
