//! Context injected into ACP prompts (ADR 0012, Decisions 10–11).
//!
//! Two kinds of context steer the agent:
//!
//! - a **session preamble**, assembled once at session start, that carries a
//!   compact vault summary plus the vault's `.notesmith/skill.md` (when
//!   present); and
//! - an **editor-context block**, rebuilt at the start of each turn from the
//!   desktop app's editor state (active note, selection, open tabs).
//!
//! Both the summary and the editor state are *provided by the caller* — the
//! daemon owns the vault index and the Tauri app owns the editor — so this
//! module only formats and bounds them. Everything is size-bounded so the
//! preamble's token footprint stays small and predictable, and missing or
//! empty inputs degrade to nothing rather than panicking (ADR 0009).

/// Maximum characters of `skill.md` carried in the preamble. Skill files are
/// author-controlled and may be long; bound them so the preamble stays small.
pub(crate) const MAX_SKILL_CHARS: usize = 4_000;

/// Maximum number of tags/folders listed in the vault summary.
const MAX_SUMMARY_ITEMS: usize = 8;

/// Maximum characters of a selection echoed into the editor-context block.
const MAX_SELECTION_CHARS: usize = 2_000;

/// Maximum number of open tabs listed in the editor-context block.
const MAX_TABS: usize = 12;

/// Truncate `text` to at most `max` characters on a char boundary, appending an
/// ellipsis marker when anything was dropped.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…[truncated]")
}

/// A compact, auto-generated description of the vault, supplied by the caller
/// (the daemon owns the index). Rendered into the one-time session preamble.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VaultSummary {
    /// The vault's display name.
    pub name: String,
    /// Total number of notes in the vault.
    pub note_count: usize,
    /// The most common tags, most frequent first.
    pub top_tags: Vec<String>,
    /// The most populated folders, most populated first.
    pub top_folders: Vec<String>,
}

impl VaultSummary {
    /// Render a single-line, bounded summary, e.g.
    /// `Vault "Notes": 142 notes. Top tags: #a, #b. Top folders: daily/.`
    pub fn render(&self) -> String {
        let name = if self.name.trim().is_empty() {
            "this vault".to_string()
        } else {
            format!("\"{}\"", self.name.trim())
        };
        let mut parts = vec![format!("Vault {name}: {} notes.", self.note_count)];
        if !self.top_tags.is_empty() {
            parts.push(format!("Top tags: {}.", join_capped(&self.top_tags)));
        }
        if !self.top_folders.is_empty() {
            parts.push(format!("Top folders: {}.", join_capped(&self.top_folders)));
        }
        parts.join(" ")
    }
}

/// Join up to [`MAX_SUMMARY_ITEMS`] items with commas.
fn join_capped(items: &[String]) -> String {
    items
        .iter()
        .take(MAX_SUMMARY_ITEMS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

/// The desktop editor's current state, injected at the start of each turn so
/// the agent knows what the user is looking at (ADR 0012, Decision 10). All
/// fields are optional; an entirely empty context renders to `None` and is
/// simply omitted from the turn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorContext {
    /// Path of the active note, relative to the vault root.
    pub active_path: Option<String>,
    /// Human-readable title of the active note.
    pub active_title: Option<String>,
    /// The user's current selection, if any.
    pub selection: Option<String>,
    /// Paths of the currently open tabs.
    pub open_tabs: Vec<String>,
}

impl EditorContext {
    /// `true` when there is no meaningful editor state to inject.
    pub fn is_empty(&self) -> bool {
        self.active_path.is_none()
            && self.active_title.is_none()
            && self
                .selection
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            && self.open_tabs.is_empty()
    }

    /// Render a compact, bounded editor-context block, or `None` when there is
    /// nothing to inject (so the turn degrades gracefully).
    pub fn render(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut lines = vec!["Current editor context:".to_string()];
        match (self.active_title.as_deref(), self.active_path.as_deref()) {
            (Some(title), Some(path)) => lines.push(format!("- Active note: {title} ({path})")),
            (Some(title), None) => lines.push(format!("- Active note: {title}")),
            (None, Some(path)) => lines.push(format!("- Active note: {path}")),
            (None, None) => {}
        }
        if let Some(selection) = self.selection.as_deref() {
            if !selection.trim().is_empty() {
                lines.push(format!(
                    "- Selection: {}",
                    truncate_chars(selection, MAX_SELECTION_CHARS)
                ));
            }
        }
        if !self.open_tabs.is_empty() {
            let tabs = self
                .open_tabs
                .iter()
                .take(MAX_TABS)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("- Open tabs: {tabs}"));
        }
        // Only the header survived — nothing concrete to inject.
        if lines.len() == 1 {
            return None;
        }
        Some(lines.join("\n"))
    }
}

/// Assemble the one-time session preamble: the MCP/local-I/O `steering` text,
/// then the vault `summary` (when present), then `skill.md` (when present and
/// non-empty), each as its own section. The skill body is bounded; a missing
/// or blank skill simply contributes nothing (ADR 0009 resilience).
pub(crate) fn assemble_preamble(
    steering: &str,
    summary: Option<&VaultSummary>,
    skill: Option<&str>,
) -> String {
    let mut sections = vec![steering.to_string()];
    if let Some(summary) = summary {
        sections.push(summary.render());
    }
    if let Some(skill) = skill {
        let skill = skill.trim();
        if !skill.is_empty() {
            sections.push(format!(
                "--- Vault skill (.notesmith/skill.md) ---\n{}",
                truncate_chars(skill, MAX_SKILL_CHARS)
            ));
        }
    }
    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_summary_renders_a_compact_line() {
        let summary = VaultSummary {
            name: "Notes".to_string(),
            note_count: 142,
            top_tags: vec!["#work".to_string(), "#idea".to_string()],
            top_folders: vec!["daily/".to_string()],
        };
        assert_eq!(
            summary.render(),
            "Vault \"Notes\": 142 notes. Top tags: #work, #idea. Top folders: daily/."
        );
    }

    #[test]
    fn vault_summary_without_tags_or_folders_is_just_the_count() {
        let summary = VaultSummary {
            name: "Notes".to_string(),
            note_count: 3,
            ..Default::default()
        };
        assert_eq!(summary.render(), "Vault \"Notes\": 3 notes.");
    }

    #[test]
    fn vault_summary_falls_back_when_name_is_blank() {
        let summary = VaultSummary {
            name: "  ".to_string(),
            note_count: 0,
            ..Default::default()
        };
        assert_eq!(summary.render(), "Vault this vault: 0 notes.");
    }

    #[test]
    fn vault_summary_caps_the_number_of_listed_items() {
        let many: Vec<String> = (0..50).map(|i| format!("#tag{i}")).collect();
        let summary = VaultSummary {
            name: "Big".to_string(),
            note_count: 9_000,
            top_tags: many,
            top_folders: Vec::new(),
        };
        let rendered = summary.render();
        // Only MAX_SUMMARY_ITEMS tags are listed.
        assert!(rendered.contains("#tag0"));
        assert!(rendered.contains(&format!("#tag{}", MAX_SUMMARY_ITEMS - 1)));
        assert!(!rendered.contains(&format!("#tag{MAX_SUMMARY_ITEMS}")));
    }

    #[test]
    fn editor_context_renders_active_note_selection_and_tabs() {
        let editor = EditorContext {
            active_path: Some("projects/acp.md".to_string()),
            active_title: Some("ACP Rebuild".to_string()),
            selection: Some("the relevant paragraph".to_string()),
            open_tabs: vec!["a.md".to_string(), "b.md".to_string()],
        };
        let rendered = editor.render().expect("non-empty context renders");
        assert!(rendered.starts_with("Current editor context:"));
        assert!(rendered.contains("- Active note: ACP Rebuild (projects/acp.md)"));
        assert!(rendered.contains("- Selection: the relevant paragraph"));
        assert!(rendered.contains("- Open tabs: a.md, b.md"));
    }

    #[test]
    fn empty_editor_context_renders_none() {
        assert!(EditorContext::default().render().is_none());
        // Whitespace-only selection is not meaningful state.
        let blank = EditorContext {
            selection: Some("   \n".to_string()),
            ..Default::default()
        };
        assert!(blank.is_empty());
        assert!(blank.render().is_none());
    }

    #[test]
    fn editor_context_truncates_a_huge_selection() {
        let huge = "x".repeat(MAX_SELECTION_CHARS + 500);
        let editor = EditorContext {
            selection: Some(huge),
            ..Default::default()
        };
        let rendered = editor.render().expect("renders");
        assert!(rendered.contains("…[truncated]"));
        // Bounded: header + selection line, the selection capped near the limit.
        assert!(rendered.chars().count() < MAX_SELECTION_CHARS + 100);
    }

    #[test]
    fn editor_context_caps_open_tabs() {
        let many: Vec<String> = (0..40).map(|i| format!("t{i}.md")).collect();
        let editor = EditorContext {
            open_tabs: many,
            ..Default::default()
        };
        let rendered = editor.render().expect("renders");
        assert!(rendered.contains("t0.md"));
        assert!(rendered.contains(&format!("t{}.md", MAX_TABS - 1)));
        assert!(!rendered.contains(&format!("t{MAX_TABS}.md")));
    }

    #[test]
    fn preamble_without_skill_is_steering_plus_summary() {
        let summary = VaultSummary {
            name: "Notes".to_string(),
            note_count: 5,
            ..Default::default()
        };
        let preamble = assemble_preamble("STEER", Some(&summary), None);
        assert_eq!(preamble, "STEER\n\nVault \"Notes\": 5 notes.");
    }

    #[test]
    fn preamble_includes_skill_when_present() {
        let preamble = assemble_preamble("STEER", None, Some("Always tag meeting notes."));
        assert!(preamble.starts_with("STEER"));
        assert!(preamble.contains("--- Vault skill (.notesmith/skill.md) ---"));
        assert!(preamble.contains("Always tag meeting notes."));
    }

    #[test]
    fn preamble_skips_blank_or_missing_skill() {
        assert_eq!(assemble_preamble("STEER", None, None), "STEER");
        assert_eq!(assemble_preamble("STEER", None, Some("   \n\t")), "STEER");
    }

    #[test]
    fn preamble_bounds_a_huge_skill_file() {
        let huge = "S".repeat(MAX_SKILL_CHARS * 4);
        let preamble = assemble_preamble("STEER", None, Some(&huge));
        assert!(preamble.contains("…[truncated]"));
        // The whole preamble stays close to the skill bound, not 4x it.
        assert!(preamble.chars().count() < MAX_SKILL_CHARS + 200);
    }
}
