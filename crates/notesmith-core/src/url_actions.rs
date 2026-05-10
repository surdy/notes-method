//! User-defined URL actions loaded from `.notesmith/url-actions.yaml`.
//!
//! An action is a named sequence of steps (API calls, open-note navigations)
//! triggered by a `notesmith://user/{action_name}?params…` URL.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Top-level file loaded from `.notesmith/url-actions.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UrlActionsFile {
    pub actions: Vec<UrlAction>,
}

/// A single named action containing one or more steps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UrlAction {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<ActionStep>,
}

/// One step inside an action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ActionStep {
    /// Call an HTTP endpoint on the Notesmith daemon.
    #[serde(rename = "api")]
    ApiCall {
        method: String,
        /// Path template — supports `{param}` interpolation.
        path: String,
        #[serde(default)]
        body: Option<serde_json::Value>,
    },
    /// Open a note in the vault.
    #[serde(rename = "open")]
    OpenNote {
        vault: String,
        /// Path template — supports `{param}` interpolation.
        path: String,
    },
}

/// Load and parse `.notesmith/url-actions.yaml` from a vault root.
///
/// Returns `Ok(file)` on success, or an error if the file is missing or malformed.
pub fn load_url_actions(vault_root: &Path) -> Result<UrlActionsFile, UrlActionsError> {
    let path = vault_root.join(".notesmith").join("url-actions.yaml");
    let content = std::fs::read_to_string(&path).map_err(|e| UrlActionsError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let file: UrlActionsFile =
        serde_yaml::from_str(&content).map_err(|e| UrlActionsError::Parse {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
    Ok(file)
}

/// Find a named action inside a [`UrlActionsFile`].
pub fn find_action<'a>(file: &'a UrlActionsFile, name: &str) -> Option<&'a UrlAction> {
    file.actions.iter().find(|a| a.name == name)
}

/// Interpolate `{param}` placeholders in a template string using the provided map.
pub fn interpolate(template: &str, params: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

/// Errors from loading or executing URL actions.
#[derive(Debug, thiserror::Error)]
pub enum UrlActionsError {
    #[error("could not read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {message}")]
    Parse { path: String, message: String },
    #[error("action not found: {0}")]
    ActionNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"
actions:
  - name: standup
    description: Create today's standup note from template
    steps:
      - type: api
        method: POST
        path: /api/v/main/templates/daily-note/instantiate
        body:
          folder: "Inbox/Daily"
      - type: open
        vault: main
        path: "Inbox/Daily/{date}.md"
  - name: weekly-review
    description: Open the weekly review dashboard
    steps:
      - type: open
        vault: main
        path: "Dashboards/Weekly Review.md"
"#;

    #[test]
    fn parse_url_actions_yaml() {
        let file: UrlActionsFile = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        assert_eq!(file.actions.len(), 2);
        assert_eq!(file.actions[0].name, "standup");
        assert_eq!(
            file.actions[0].description.as_deref(),
            Some("Create today's standup note from template")
        );
        assert_eq!(file.actions[0].steps.len(), 2);
    }

    #[test]
    fn parse_api_step() {
        let file: UrlActionsFile = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        match &file.actions[0].steps[0] {
            ActionStep::ApiCall { method, path, body } => {
                assert_eq!(method, "POST");
                assert_eq!(path, "/api/v/main/templates/daily-note/instantiate");
                assert!(body.is_some());
            }
            other => panic!("expected ApiCall, got {other:?}"),
        }
    }

    #[test]
    fn parse_open_step() {
        let file: UrlActionsFile = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        match &file.actions[0].steps[1] {
            ActionStep::OpenNote { vault, path } => {
                assert_eq!(vault, "main");
                assert_eq!(path, "Inbox/Daily/{date}.md");
            }
            other => panic!("expected OpenNote, got {other:?}"),
        }
    }

    #[test]
    fn find_action_by_name() {
        let file: UrlActionsFile = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        let action = find_action(&file, "weekly-review").unwrap();
        assert_eq!(action.name, "weekly-review");
    }

    #[test]
    fn find_action_missing_returns_none() {
        let file: UrlActionsFile = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        assert!(find_action(&file, "nonexistent").is_none());
    }

    #[test]
    fn interpolate_params_in_template() {
        let mut params = HashMap::new();
        params.insert("date".into(), "2026-05-10".into());
        params.insert("vault".into(), "main".into());

        assert_eq!(
            interpolate("Inbox/Daily/{date}.md", &params),
            "Inbox/Daily/2026-05-10.md"
        );
        assert_eq!(
            interpolate("/api/v/{vault}/inbox", &params),
            "/api/v/main/inbox"
        );
    }

    #[test]
    fn interpolate_missing_param_left_as_is() {
        let params = HashMap::new();
        assert_eq!(interpolate("Inbox/{date}.md", &params), "Inbox/{date}.md");
    }

    #[test]
    fn load_from_disk() {
        // Use a path that doesn't exist to verify the error handling
        let result = load_url_actions(Path::new("/nonexistent/vault"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("could not read"));
    }

    #[test]
    fn load_from_golden_vault() {
        let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("golden-vault");
        // Only run this test when the golden vault fixture has the file
        if golden.join(".notesmith").join("url-actions.yaml").exists() {
            let file = load_url_actions(&golden).unwrap();
            assert!(!file.actions.is_empty());
        }
    }
}
