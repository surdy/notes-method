//! In-app model selection over ACP Session Config Options (ADR 0012,
//! Decision 12).
//!
//! Agents advertise the models a session can use either through the
//! `session/new` result's `configOptions` (the preferred, current mechanism —
//! an entry whose `category` is `model`) or, for older agents, through the
//! deprecated `modes` field. Notesmith **hardcodes no model list**: it renders
//! whatever the agent advertises and applies the user's choice back to the
//! session.
//!
//! This module normalizes either source into a single UI-agnostic
//! [`ModelPicker`]. Parsing prefers `configOptions`, falls back to `modes`, and
//! yields `None` when an agent advertises neither (the caller then shows no
//! picker — never an error). The picker also records which mechanism backs it
//! so the session can route the setter to `session/set_config_option` or
//! `session/set_mode`.

use agent_client_protocol::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption,
    SessionConfigSelectOptions, SessionModeState,
};

/// One selectable model, as advertised by the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    /// Stable identifier sent back to the agent when this model is chosen.
    pub id: String,
    /// Human-readable label for display.
    pub name: String,
    /// Optional longer description.
    pub description: Option<String>,
}

/// Which ACP mechanism backs a [`ModelPicker`], so the session knows whether to
/// apply a choice via `session/set_config_option` (config option) or
/// `session/set_mode` (the deprecated modes fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
enum PickerKind {
    /// A `configOptions` model selector, set by its config id.
    ConfigOption { config_id: String },
    /// The deprecated `modes` selector, set by mode id.
    Mode,
}

/// A normalized, UI-agnostic model selector derived from a `session/new`
/// result. Carries the current selection, the advertised options, and the
/// mechanism used to apply a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPicker {
    current: String,
    options: Vec<ModelOption>,
    kind: PickerKind,
}

impl ModelPicker {
    /// The id of the currently selected model.
    pub fn current(&self) -> &str {
        &self.current
    }

    /// The advertised model options, in the order the agent listed them.
    pub fn options(&self) -> &[ModelOption] {
        &self.options
    }

    /// Whether `value` is one of the advertised option ids.
    pub fn contains(&self, value: &str) -> bool {
        self.options.iter().any(|option| option.id == value)
    }

    /// The backing config-option id when this picker is a `configOptions`
    /// selector, or `None` when it is the deprecated `modes` selector. The
    /// session uses this to route the setter to the right ACP method.
    pub(crate) fn config_id(&self) -> Option<&str> {
        match &self.kind {
            PickerKind::ConfigOption { config_id } => Some(config_id),
            PickerKind::Mode => None,
        }
    }
}

/// Flatten an ungrouped or grouped option list into [`ModelOption`]s.
fn flatten_options(options: &SessionConfigSelectOptions) -> Vec<ModelOption> {
    match options {
        SessionConfigSelectOptions::Ungrouped(list) => list.iter().map(to_model_option).collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter().map(to_model_option))
            .collect(),
        // The enum is `#[non_exhaustive]`; an unknown shape contributes nothing.
        _ => Vec::new(),
    }
}

fn to_model_option(option: &SessionConfigSelectOption) -> ModelOption {
    ModelOption {
        id: option.value.0.to_string(),
        name: option.name.clone(),
        description: option.description.clone(),
    }
}

/// Build a [`ModelPicker`] from a `session/new` result. Prefers a model
/// `configOptions` entry, falls back to the deprecated `modes` field, and
/// returns `None` when neither advertises selectable values (so the caller
/// renders no picker, without error).
pub(crate) fn parse_model_picker(
    config_options: Option<&[SessionConfigOption]>,
    modes: Option<&SessionModeState>,
) -> Option<ModelPicker> {
    if let Some(picker) = config_options.and_then(picker_from_config_options) {
        return Some(picker);
    }
    modes.and_then(picker_from_modes)
}

/// Find the first `category: "model"` select option and turn it into a picker.
fn picker_from_config_options(options: &[SessionConfigOption]) -> Option<ModelPicker> {
    options.iter().find_map(|option| {
        if !matches!(option.category, Some(SessionConfigOptionCategory::Model)) {
            return None;
        }
        let SessionConfigKind::Select(select) = &option.kind else {
            return None;
        };
        let models = flatten_options(&select.options);
        if models.is_empty() {
            return None;
        }
        Some(ModelPicker {
            current: select.current_value.0.to_string(),
            options: models,
            kind: PickerKind::ConfigOption {
                config_id: option.id.0.to_string(),
            },
        })
    })
}

/// Turn the deprecated `modes` state into a picker (the fallback path).
fn picker_from_modes(modes: &SessionModeState) -> Option<ModelPicker> {
    if modes.available_modes.is_empty() {
        return None;
    }
    let options = modes
        .available_modes
        .iter()
        .map(|mode| ModelOption {
            id: mode.id.0.to_string(),
            name: mode.name.clone(),
            description: mode.description.clone(),
        })
        .collect();
    Some(ModelPicker {
        current: modes.current_mode_id.0.to_string(),
        options,
        kind: PickerKind::Mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::{SessionConfigOption, SessionMode, SessionModeState};
    use serde_json::json;

    fn config_options_from(value: serde_json::Value) -> Vec<SessionConfigOption> {
        serde_json::from_value(value).expect("valid config options")
    }

    #[test]
    fn prefers_model_config_option_and_lists_its_values() {
        let options = config_options_from(json!([
            {
                "id": "thought",
                "name": "Reasoning",
                "category": "thought_level",
                "type": "select",
                "currentValue": "low",
                "options": [{ "value": "low", "name": "Low" }]
            },
            {
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": "gpt-5",
                "options": [
                    { "value": "gpt-5", "name": "GPT-5" },
                    { "value": "claude", "name": "Claude", "description": "Anthropic" }
                ]
            }
        ]));
        let picker = parse_model_picker(Some(&options), None).expect("model picker");
        assert_eq!(picker.current(), "gpt-5");
        assert_eq!(picker.config_id(), Some("model"));
        assert_eq!(
            picker.options(),
            &[
                ModelOption {
                    id: "gpt-5".to_string(),
                    name: "GPT-5".to_string(),
                    description: None
                },
                ModelOption {
                    id: "claude".to_string(),
                    name: "Claude".to_string(),
                    description: Some("Anthropic".to_string())
                },
            ]
        );
        assert!(picker.contains("claude"));
        assert!(!picker.contains("missing"));
    }

    #[test]
    fn flattens_grouped_model_options() {
        let options = config_options_from(json!([
            {
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": "a",
                "options": [
                    { "group": "fast", "name": "Fast", "options": [{ "value": "a", "name": "A" }] },
                    { "group": "smart", "name": "Smart", "options": [{ "value": "b", "name": "B" }] }
                ]
            }
        ]));
        let picker = parse_model_picker(Some(&options), None).expect("model picker");
        let ids: Vec<&str> = picker.options().iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn falls_back_to_modes_when_no_model_config_option() {
        // A non-model config option must not shadow the modes fallback.
        let options = config_options_from(json!([
            {
                "id": "thought",
                "name": "Reasoning",
                "category": "thought_level",
                "type": "select",
                "currentValue": "low",
                "options": [{ "value": "low", "name": "Low" }]
            }
        ]));
        let modes = SessionModeState::new(
            "default",
            vec![
                SessionMode::new("default", "Default"),
                SessionMode::new("yolo", "Yolo").description("No prompts"),
            ],
        );
        let picker = parse_model_picker(Some(&options), Some(&modes)).expect("modes picker");
        assert_eq!(picker.current(), "default");
        // The modes fallback is not a config option.
        assert_eq!(picker.config_id(), None);
        let ids: Vec<&str> = picker.options().iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["default", "yolo"]);
    }

    #[test]
    fn absent_selectors_yield_no_picker() {
        assert!(parse_model_picker(None, None).is_none());
        // Empty advertisements also degrade to no picker, not an error.
        assert!(parse_model_picker(Some(&[]), None).is_none());
        let empty_modes = SessionModeState::new("x", Vec::new());
        assert!(parse_model_picker(None, Some(&empty_modes)).is_none());
    }

    #[test]
    fn ignores_model_option_with_no_values() {
        let options = config_options_from(json!([
            {
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": "x",
                "options": []
            }
        ]));
        let _ = config_options_from(json!([]));
        let picker = parse_model_picker(Some(&options), None);
        // An empty model selector contributes nothing.
        assert!(picker.is_none());
    }

    #[test]
    fn model_config_option_takes_precedence_over_modes() {
        let options = config_options_from(json!([
            {
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": "gpt-5",
                "options": [{ "value": "gpt-5", "name": "GPT-5" }]
            }
        ]));
        let modes = SessionModeState::new("default", vec![SessionMode::new("default", "Default")]);
        let picker = parse_model_picker(Some(&options), Some(&modes)).expect("picker");
        assert_eq!(picker.config_id(), Some("model"));
        assert_eq!(picker.current(), "gpt-5");
    }
}
