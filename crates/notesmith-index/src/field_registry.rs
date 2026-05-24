use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FieldRegistry {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub fields: HashMap<String, FieldDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FieldDefinition {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub values: Option<Vec<String>>,
    #[serde(default)]
    pub suggest_from: Option<String>,
    #[serde(default)]
    pub multivalue: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Number,
    Integer,
    Date,
    Link,
    Enum,
    List,
    Boolean,
}

impl FieldRegistry {
    /// Load from .notesmith/fields.toml, returning empty registry if file doesn't exist.
    pub fn load(vault_root: &Path) -> Self {
        let path = vault_root.join(".notesmith").join("fields.toml");
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|error| {
                tracing::warn!(path = %path.display(), "Failed to parse fields.toml: {error}");
                Self::default()
            }),
            Err(error) => {
                tracing::warn!(path = %path.display(), "Failed to read fields.toml: {error}");
                Self::default()
            }
        }
    }

    /// Get the definition for a field key.
    pub fn get(&self, key: &str) -> Option<&FieldDefinition> {
        self.fields.get(key)
    }

    /// Validate a field value against the registry (advisory only).
    pub fn validate(&self, key: &str, value: &str) -> Option<ValidationWarning> {
        let def = self.fields.get(key)?;
        match def.field_type {
            FieldType::Enum => {
                if let Some(allowed) = &def.values {
                    if !allowed.iter().any(|entry| entry == value) {
                        return Some(ValidationWarning {
                            key: key.to_string(),
                            value: value.to_string(),
                            reason: format!("Value '{value}' not in allowed values: {:?}", allowed),
                        });
                    }
                }
                None
            }
            FieldType::Number | FieldType::Integer => {
                if value.parse::<f64>().is_err() {
                    return Some(ValidationWarning {
                        key: key.to_string(),
                        value: value.to_string(),
                        reason: format!("Value '{value}' is not a valid number"),
                    });
                }
                None
            }
            FieldType::Boolean => {
                if !matches!(value, "true" | "false" | "yes" | "no") {
                    return Some(ValidationWarning {
                        key: key.to_string(),
                        value: value.to_string(),
                        reason: format!("Value '{value}' is not a boolean"),
                    });
                }
                None
            }
            _ => None,
        }
    }
}

impl Default for FieldRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            fields: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    pub key: String,
    pub value: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::{FieldRegistry, FieldType};
    use std::fs;

    fn write_fields_file(root: &std::path::Path, content: &str) {
        fs::create_dir_all(root.join(".notesmith")).unwrap();
        fs::write(root.join(".notesmith/fields.toml"), content).unwrap();
    }

    fn golden_vault() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
    }

    #[test]
    fn parses_fields_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_fields_file(
            temp_dir.path(),
            r#"
version = 1

[fields.status]
type = "enum"
description = "Customer status"
values = ["active", "paused", "closed"]

[fields.customer]
type = "link"
suggest_from = "SELECT DISTINCT value FROM v_fields WHERE key = 'customer' ORDER BY value"
multivalue = true
"#,
        );

        let registry = FieldRegistry::load(temp_dir.path());

        assert_eq!(registry.version, 1);
        assert_eq!(registry.fields.len(), 2);
        let status = registry.get("status").unwrap();
        assert_eq!(status.field_type, FieldType::Enum);
        assert_eq!(status.description.as_deref(), Some("Customer status"));
        assert_eq!(
            status.values.as_ref().unwrap(),
            &vec![
                "active".to_string(),
                "paused".to_string(),
                "closed".to_string()
            ]
        );
        let customer = registry.get("customer").unwrap();
        assert_eq!(customer.field_type, FieldType::Link);
        assert_eq!(customer.multivalue, Some(true));
        assert!(
            customer
                .suggest_from
                .as_ref()
                .unwrap()
                .contains("SELECT DISTINCT value")
        );
    }

    #[test]
    fn validates_enum_values() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_fields_file(
            temp_dir.path(),
            r#"
[fields.status]
type = "enum"
values = ["active", "paused"]
"#,
        );

        let registry = FieldRegistry::load(temp_dir.path());

        assert!(registry.validate("status", "active").is_none());
        let warning = registry.validate("status", "closed").unwrap();
        assert_eq!(warning.key, "status");
        assert_eq!(warning.value, "closed");
        assert!(warning.reason.contains("allowed values"));
    }

    #[test]
    fn validates_number_values() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_fields_file(
            temp_dir.path(),
            r#"
[fields.score]
type = "number"
"#,
        );

        let registry = FieldRegistry::load(temp_dir.path());

        assert!(registry.validate("score", "42").is_none());
        let warning = registry.validate("score", "forty-two").unwrap();
        assert_eq!(warning.key, "score");
        assert!(warning.reason.contains("not a valid number"));
    }

    #[test]
    fn returns_default_registry_when_file_missing() {
        let temp_dir = tempfile::tempdir().unwrap();

        let registry = FieldRegistry::load(temp_dir.path());

        assert_eq!(registry, FieldRegistry::default());
    }

    #[test]
    fn loads_golden_vault_registry_fixture() {
        let registry = FieldRegistry::load(&golden_vault());

        assert_eq!(registry.version, 1);
        assert_eq!(registry.get("type").unwrap().field_type, FieldType::Enum);
        assert_eq!(
            registry.get("customer").unwrap().field_type,
            FieldType::String
        );
        assert!(
            registry
                .get("customer")
                .unwrap()
                .suggest_from
                .as_deref()
                .unwrap()
                .contains("SELECT DISTINCT value")
        );
    }
}
