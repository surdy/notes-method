//! notesmith-routing: YAML-driven routing rules, destination resolver, and archive workflow

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub version: u32,
    #[serde(default = "default_on_exists")]
    pub default_on_exists: OnExists,
    pub rules: Vec<RoutingRule>,
}

fn default_on_exists() -> OnExists {
    OnExists::Skip
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OnExists {
    Skip,
    Overwrite,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub when: HashMap<String, String>,
    pub then: RoutingAction,
    #[serde(default)]
    pub on_exists: Option<OnExists>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingAction {
    pub move_to: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteMatch {
    pub rule_id: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteResult {
    pub from: String,
    pub to: String,
    pub rule_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("routing config not found at {path}")]
    ConfigNotFound { path: PathBuf },
    #[error("invalid routing config: {reason}")]
    InvalidConfig { reason: String },
    #[error("no matching rule for {path}")]
    NoMatch { path: String },
    #[error("destination already exists: {path}")]
    DestinationExists { path: String },
    #[error("note has no frontmatter: {path}")]
    NoFrontmatter { path: String },
    #[error("note already archived: {path}")]
    AlreadyArchived { path: String },
    #[error("render error: {0}")]
    RenderError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("vault error: {0}")]
    VaultError(String),
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct RoutingEngine {
    config: RoutingConfig,
}

impl RoutingEngine {
    pub fn load(vault_root: &Path) -> Result<Self, RoutingError> {
        let config_path = vault_root.join(".notesmith").join("routing.yaml");
        if !config_path.exists() {
            return Err(RoutingError::ConfigNotFound { path: config_path });
        }
        let content = std::fs::read_to_string(&config_path)?;
        let config: RoutingConfig =
            serde_yaml::from_str(&content).map_err(|e| RoutingError::InvalidConfig {
                reason: e.to_string(),
            })?;
        Ok(Self { config })
    }

    pub fn from_config(config: RoutingConfig) -> Self {
        Self { config }
    }

    /// Preview where a note would be routed without moving it.
    pub fn preview(&self, note_path: &str, content: &str) -> Result<RouteMatch, RoutingError> {
        let (raw_fm, _body) = notesmith_vault::extract_frontmatter(content);
        let raw_fm = raw_fm.ok_or_else(|| RoutingError::NoFrontmatter {
            path: note_path.to_string(),
        })?;

        let fm_mapping: Mapping =
            serde_yaml::from_str(&raw_fm).map_err(|e| RoutingError::InvalidConfig {
                reason: e.to_string(),
            })?;

        // Check if already archived
        if let Some(Value::Bool(true)) = fm_mapping.get(Value::String("archived".to_string())) {
            return Err(RoutingError::AlreadyArchived {
                path: note_path.to_string(),
            });
        }

        for rule in &self.config.rules {
            if matches_rule(&fm_mapping, &rule.when) {
                let destination = render_destination(&rule.then.move_to, &fm_mapping, note_path)?;
                return Ok(RouteMatch {
                    rule_id: rule.id.clone(),
                    destination,
                });
            }
        }

        Err(RoutingError::NoMatch {
            path: note_path.to_string(),
        })
    }

    /// Route a single note: read → match → stamp archive → write → move.
    pub fn apply(
        &self,
        vault_root: &Path,
        note_path: &str,
        engine: &dyn notesmith_core::VaultEngine,
    ) -> Result<RouteResult, RoutingError> {
        let vault_path = notesmith_core::VaultPath::new(note_path.to_string());
        let content = engine
            .read(vault_root, &vault_path)
            .map_err(|e| RoutingError::VaultError(e.to_string()))?;

        let route_match = self.preview(note_path, &content)?;

        let stamped_content = stamp_archived(&content)?;

        engine
            .write(vault_root, &vault_path, None, &stamped_content)
            .map_err(|e| RoutingError::VaultError(e.to_string()))?;

        let dest_path = notesmith_core::VaultPath::new(route_match.destination.clone());
        engine
            .move_path(vault_root, &vault_path, &dest_path)
            .map_err(|e| RoutingError::VaultError(e.to_string()))?;

        Ok(RouteResult {
            from: note_path.to_string(),
            to: route_match.destination,
            rule_id: route_match.rule_id,
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn matches_rule(fm: &Mapping, conditions: &HashMap<String, String>) -> bool {
    for (field, expected) in conditions {
        let key = Value::String(field.clone());
        match fm.get(&key) {
            None => return false,
            Some(value) => {
                if expected == "*" {
                    match value {
                        Value::String(s) if s.is_empty() => return false,
                        Value::Null => return false,
                        _ => {}
                    }
                } else {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Bool(b) => b.to_string(),
                        Value::Number(n) => n.to_string(),
                        _ => return false,
                    };
                    if value_str != *expected {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn render_destination(
    template: &str,
    fm: &Mapping,
    note_path: &str,
) -> Result<String, RoutingError> {
    let mut env = minijinja::Environment::new();

    env.add_filter("unwikilink", |val: String| -> String {
        val.trim_start_matches("[[")
            .trim_end_matches("]]")
            .to_string()
    });

    // Custom filters for date slicing (minijinja may not support Python slicing)
    env.add_filter("year", |val: String| -> String {
        val.get(..4).unwrap_or("").to_string()
    });
    env.add_filter("month", |val: String| -> String {
        val.get(5..7).unwrap_or("").to_string()
    });

    // Build context from frontmatter fields
    let mut ctx = std::collections::BTreeMap::new();
    for (key, value) in fm {
        if let Value::String(k) = key {
            let v = match value {
                Value::String(s) => minijinja::Value::from(s.clone()),
                Value::Bool(b) => minijinja::Value::from(*b),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        minijinja::Value::from(i)
                    } else if let Some(f) = n.as_f64() {
                        minijinja::Value::from(f)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            ctx.insert(k.clone(), v);
        }
    }

    let rendered = env
        .render_str(template, ctx)
        .map_err(|e| RoutingError::RenderError(e.to_string()))?;

    if rendered.ends_with('/') {
        let filename = Path::new(note_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(format!("{rendered}{filename}"))
    } else {
        Ok(rendered)
    }
}

fn stamp_archived(content: &str) -> Result<String, RoutingError> {
    let (raw_fm, body) = notesmith_vault::extract_frontmatter(content);
    let raw_fm =
        raw_fm.ok_or_else(|| RoutingError::RenderError("no frontmatter to stamp".into()))?;

    let mut mapping = notesmith_vault::parse_frontmatter_mapping(&raw_fm)
        .ok_or_else(|| RoutingError::RenderError("invalid frontmatter".into()))?;

    mapping.insert(Value::String("archived".to_string()), Value::Bool(true));
    mapping.insert(
        Value::String("archived-at".to_string()),
        Value::String(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
    );

    let sorted = notesmith_vault::sort_mapping(mapping);
    let yaml = notesmith_vault::serialize_frontmatter(&sorted);

    let result = if body.is_empty() {
        format!("---\n{yaml}\n---\n")
    } else {
        format!("---\n{yaml}\n---\n{body}")
    };

    Ok(result)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(rules: Vec<RoutingRule>) -> RoutingConfig {
        RoutingConfig {
            version: 1,
            default_on_exists: OnExists::Skip,
            rules,
        }
    }

    fn make_rule(id: &str, when: &[(&str, &str)], move_to: &str) -> RoutingRule {
        RoutingRule {
            id: id.to_string(),
            when: when
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            then: RoutingAction {
                move_to: move_to.to_string(),
            },
            on_exists: None,
        }
    }

    fn mapping_from_pairs(pairs: &[(&str, &str)]) -> Mapping {
        let mut m = Mapping::new();
        for (k, v) in pairs {
            m.insert(Value::String(k.to_string()), Value::String(v.to_string()));
        }
        m
    }

    // ── matches_rule tests ────────────────────────────────────────────────

    #[test]
    fn test_matches_rule_exact() {
        let fm = mapping_from_pairs(&[
            ("type", "meeting"),
            ("meeting-kind", "external"),
            ("customer", "[[Acme Corp]]"),
        ]);
        let when: HashMap<String, String> = [
            ("type".to_string(), "meeting".to_string()),
            ("meeting-kind".to_string(), "external".to_string()),
        ]
        .into();

        assert!(matches_rule(&fm, &when));
    }

    #[test]
    fn test_matches_rule_wildcard() {
        let fm = mapping_from_pairs(&[("type", "note"), ("customer", "[[Acme Corp]]")]);
        let when: HashMap<String, String> = [
            ("type".to_string(), "note".to_string()),
            ("customer".to_string(), "*".to_string()),
        ]
        .into();

        assert!(matches_rule(&fm, &when));
    }

    #[test]
    fn test_matches_rule_wildcard_empty_string_fails() {
        let fm = mapping_from_pairs(&[("type", "note"), ("customer", "")]);
        let when: HashMap<String, String> = [
            ("type".to_string(), "note".to_string()),
            ("customer".to_string(), "*".to_string()),
        ]
        .into();

        assert!(!matches_rule(&fm, &when));
    }

    #[test]
    fn test_matches_rule_missing_field() {
        let fm = mapping_from_pairs(&[("type", "note")]);
        let when: HashMap<String, String> = [
            ("type".to_string(), "note".to_string()),
            ("customer".to_string(), "*".to_string()),
        ]
        .into();

        assert!(!matches_rule(&fm, &when));
    }

    #[test]
    fn test_matches_rule_wrong_value() {
        let fm = mapping_from_pairs(&[("type", "meeting"), ("meeting-kind", "internal")]);
        let when: HashMap<String, String> = [
            ("type".to_string(), "meeting".to_string()),
            ("meeting-kind".to_string(), "external".to_string()),
        ]
        .into();

        assert!(!matches_rule(&fm, &when));
    }

    // ── render_destination tests ──────────────────────────────────────────

    #[test]
    fn test_render_destination_with_unwikilink() {
        let fm = mapping_from_pairs(&[("type", "meeting"), ("customer", "[[Acme Corp]]")]);
        let result = render_destination(
            "Customers/{{ customer | unwikilink }}/External Meetings/",
            &fm,
            "Inbox/My Meeting.md",
        )
        .unwrap();

        assert_eq!(
            result,
            "Customers/Acme Corp/External Meetings/My Meeting.md"
        );
    }

    #[test]
    fn test_render_destination_appends_filename() {
        let fm = mapping_from_pairs(&[("type", "note")]);
        let result = render_destination("General/", &fm, "Inbox/some note.md").unwrap();

        assert_eq!(result, "General/some note.md");
    }

    #[test]
    fn test_render_destination_no_trailing_slash() {
        let fm = mapping_from_pairs(&[("type", "note")]);
        let result =
            render_destination("General/my-fixed-name.md", &fm, "Inbox/whatever.md").unwrap();

        assert_eq!(result, "General/my-fixed-name.md");
    }

    #[test]
    fn test_render_destination_daily_with_date_filters() {
        let fm = mapping_from_pairs(&[("type", "daily"), ("date", "2025-01-15")]);
        let result = render_destination(
            "General/Journal/{{ date | year }}/{{ date | month }}/",
            &fm,
            "Inbox/2025-01-15.md",
        )
        .unwrap();

        assert_eq!(result, "General/Journal/2025/01/2025-01-15.md");
    }

    // ── stamp_archived tests ──────────────────────────────────────────────

    #[test]
    fn test_stamp_archived() {
        let content = "---\ntype: meeting\ncustomer: \"[[Acme]]\"\n---\n# Meeting\n";
        let result = stamp_archived(content).unwrap();

        assert!(result.contains("archived: true"));
        assert!(result.contains("archived-at:"));
        assert!(result.contains("# Meeting"));
    }

    #[test]
    fn test_stamp_archived_no_frontmatter() {
        let result = stamp_archived("Hello world");
        assert!(result.is_err());
    }

    // ── preview tests ─────────────────────────────────────────────────────

    #[test]
    fn test_preview_no_frontmatter() {
        let engine = RoutingEngine::from_config(make_config(vec![]));
        let result = engine.preview("Inbox/test.md", "No frontmatter here");
        assert!(matches!(result, Err(RoutingError::NoFrontmatter { .. })));
    }

    #[test]
    fn test_preview_already_archived() {
        let engine = RoutingEngine::from_config(make_config(vec![make_rule(
            "test",
            &[("type", "note")],
            "General/",
        )]));
        let content = "---\ntype: note\narchived: true\n---\nBody";
        let result = engine.preview("Inbox/test.md", content);
        assert!(matches!(result, Err(RoutingError::AlreadyArchived { .. })));
    }

    #[test]
    fn test_preview_no_match() {
        let engine = RoutingEngine::from_config(make_config(vec![make_rule(
            "meeting",
            &[("type", "meeting")],
            "Meetings/",
        )]));
        let content = "---\ntype: note\n---\nBody";
        let result = engine.preview("Inbox/test.md", content);
        assert!(matches!(result, Err(RoutingError::NoMatch { .. })));
    }

    #[test]
    fn test_preview_meeting_external() {
        let engine = RoutingEngine::from_config(make_config(vec![make_rule(
            "external-meeting",
            &[("type", "meeting"), ("meeting-kind", "external")],
            "Customers/{{ customer | unwikilink }}/External Meetings/",
        )]));
        let content = "---\ntype: meeting\nmeeting-kind: external\ncustomer: \"[[Acme Corp]]\"\ndate: 2025-03-15\n---\n# Meeting Notes\n";
        let result = engine
            .preview("Inbox/Meeting with Acme.md", content)
            .unwrap();
        assert_eq!(result.rule_id, "external-meeting");
        assert_eq!(
            result.destination,
            "Customers/Acme Corp/External Meetings/Meeting with Acme.md"
        );
    }

    #[test]
    fn test_preview_daily() {
        let engine = RoutingEngine::from_config(make_config(vec![make_rule(
            "daily",
            &[("type", "daily")],
            "General/Journal/{{ date | year }}/{{ date | month }}/",
        )]));
        let content = "---\ntype: daily\ndate: 2025-01-15\n---\n# Daily\n";
        let result = engine.preview("Inbox/2025-01-15.md", content).unwrap();
        assert_eq!(result.rule_id, "daily");
        assert_eq!(result.destination, "General/Journal/2025/01/2025-01-15.md");
    }

    #[test]
    fn test_preview_note_general() {
        let engine = RoutingEngine::from_config(make_config(vec![make_rule(
            "note-general",
            &[("type", "note")],
            "General/",
        )]));
        let content = "---\ntype: note\n---\n# My Note\n";
        let result = engine.preview("Inbox/idea.md", content).unwrap();
        assert_eq!(result.rule_id, "note-general");
        assert_eq!(result.destination, "General/idea.md");
    }

    #[test]
    fn test_preview_first_match_wins() {
        let engine = RoutingEngine::from_config(make_config(vec![
            make_rule(
                "note-customer",
                &[("type", "note"), ("customer", "*")],
                "Customers/{{ customer | unwikilink }}/",
            ),
            make_rule("note-general", &[("type", "note")], "General/"),
        ]));

        let content_with_customer = "---\ntype: note\ncustomer: \"[[Acme]]\"\n---\nBody";
        let result = engine
            .preview("Inbox/test.md", content_with_customer)
            .unwrap();
        assert_eq!(result.rule_id, "note-customer");

        let content_no_customer = "---\ntype: note\n---\nBody";
        let result = engine
            .preview("Inbox/test.md", content_no_customer)
            .unwrap();
        assert_eq!(result.rule_id, "note-general");
    }

    // ── Config parsing tests ──────────────────────────────────────────────

    #[test]
    fn test_config_parses_from_yaml() {
        let yaml = r#"
version: 1
default_on_exists: skip
rules:
  - id: external-meeting
    when:
      type: meeting
      meeting-kind: external
    then:
      move_to: "Customers/{{ customer | unwikilink }}/External Meetings/"
  - id: daily
    when:
      type: daily
    then:
      move_to: "General/Journal/{{ date | year }}/{{ date | month }}/"
"#;
        let config: RoutingConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.default_on_exists, OnExists::Skip);
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].id, "external-meeting");
        assert_eq!(config.rules[1].id, "daily");
    }

    // ── Integration: apply with real filesystem ───────────────────────────

    #[test]
    fn test_apply_moves_and_stamps() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create the note
        let inbox = root.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(inbox.join("idea.md"), "---\ntype: note\n---\n# My Idea\n").unwrap();

        let engine = notesmith_vault::NativeVaultEngine;
        let routing = RoutingEngine::from_config(make_config(vec![make_rule(
            "note-general",
            &[("type", "note")],
            "General/",
        )]));

        let result = routing.apply(root, "Inbox/idea.md", &engine).unwrap();

        assert_eq!(result.from, "Inbox/idea.md");
        assert_eq!(result.to, "General/idea.md");
        assert_eq!(result.rule_id, "note-general");

        // Source should be gone
        assert!(!inbox.join("idea.md").exists());

        // Destination should exist and have archived stamp
        let dest_content = std::fs::read_to_string(root.join("General/idea.md")).unwrap();
        assert!(dest_content.contains("archived: true"));
        assert!(dest_content.contains("archived-at:"));
        assert!(dest_content.contains("# My Idea"));
    }
}
