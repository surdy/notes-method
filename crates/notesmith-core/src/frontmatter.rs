use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;

/// Generic frontmatter — a flat map of string keys to YAML values.
/// No hardcoded note types. `type`/`kind` is just another field.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Frontmatter {
    /// All key-value pairs from the YAML frontmatter
    #[serde(flatten)]
    pub fields: HashMap<String, Value>,
}

impl Frontmatter {
    /// Get the `tags` field as a Vec<String>, if present
    pub fn tags(&self) -> Vec<String> {
        self.fields
            .get("tags")
            .and_then(|v| match v {
                Value::Sequence(seq) => Some(
                    seq.iter()
                        .filter_map(|item| item.as_str().map(String::from))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Get a field value as a string
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(|v| v.as_str())
    }

    /// Get a field value as a string (owned), handling non-string YAML values
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.fields.get(key).map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::Null => String::new(),
            other => serde_yaml::to_string(other)
                .unwrap_or_default()
                .trim()
                .to_string(),
        })
    }

    /// Check if a field exists
    pub fn has_field(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    /// Get the title field if present
    pub fn title(&self) -> Option<&str> {
        self.get_str("title")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_generic_frontmatter_preserves_field_types() {
        let yaml = r#"
title: Prototype Notes
type: dashboard
date: 2025-01-15
tags:
  - research
  - prototype
published: true
score: 3
nested:
  owner: surdy
  links:
    - "[[Acme Corp]]"
"#;

        let frontmatter: Frontmatter = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(frontmatter.get_str("title"), Some("Prototype Notes"));
        assert_eq!(
            frontmatter.get_string("published"),
            Some("true".to_string())
        );
        assert_eq!(frontmatter.get_string("score"), Some("3".to_string()));
        assert_eq!(frontmatter.tags(), vec!["research", "prototype"]);
        assert_eq!(
            frontmatter.get_string("date"),
            Some("2025-01-15".to_string())
        );
        assert!(matches!(
            frontmatter.fields.get("nested"),
            Some(Value::Mapping(_))
        ));
    }

    #[test]
    fn get_str_and_title_return_string_values() {
        let yaml = r#"
title: Inbox
date: 2025-01-15
published: false
"#;
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(frontmatter.title(), Some("Inbox"));
        assert_eq!(frontmatter.get_str("date"), Some("2025-01-15"));
        assert_eq!(frontmatter.get_str("published"), None);
    }

    #[test]
    fn get_string_handles_scalar_and_nested_values() {
        let yaml = r#"
name: Widget
flag: false
count: 7
nothing: null
nested:
  stage: discovery
list:
  - alpha
  - beta
"#;
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(frontmatter.get_string("name"), Some("Widget".to_string()));
        assert_eq!(frontmatter.get_string("flag"), Some("false".to_string()));
        assert_eq!(frontmatter.get_string("count"), Some("7".to_string()));
        assert_eq!(frontmatter.get_string("nothing"), Some(String::new()));
        assert_eq!(
            frontmatter.get_string("nested"),
            Some("stage: discovery".to_string())
        );
        assert_eq!(
            frontmatter.get_string("list"),
            Some(
                "- alpha
- beta"
                    .to_string()
            )
        );
    }

    #[test]
    fn tags_ignores_missing_or_non_sequence_values() {
        let no_tags: Frontmatter = serde_yaml::from_str("title: Note").unwrap();
        let string_tags: Frontmatter = serde_yaml::from_str("tags: research").unwrap();

        assert!(no_tags.tags().is_empty());
        assert!(string_tags.tags().is_empty());
    }

    #[test]
    fn flatten_preserves_unknown_fields() {
        let yaml = r#"
kind: experiment
custom_field: keep me
another_one:
  nested: true
"#;
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml).unwrap();

        assert!(frontmatter.has_field("kind"));
        assert_eq!(frontmatter.get_str("custom_field"), Some("keep me"));
        assert!(matches!(
            frontmatter.fields.get("another_one"),
            Some(Value::Mapping(_))
        ));
    }
}
