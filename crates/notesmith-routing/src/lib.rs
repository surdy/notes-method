//! notesmith-routing: YAML-driven routing rules, destination resolver, and archive workflow

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use globset::Glob;
use minijinja::Environment;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::{Mapping, Value};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoutingConfig {
    pub version: u32,
    pub defaults: Defaults,
    pub rules: Vec<RoutingRule>,
}

#[derive(Debug, Deserialize)]
struct RawRoutingConfig {
    version: u32,
    #[serde(default)]
    defaults: Option<Defaults>,
    #[serde(default)]
    default_on_exists: Option<OnExists>,
    rules: Vec<RoutingRule>,
}

impl<'de> Deserialize<'de> for RoutingConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawRoutingConfig::deserialize(deserializer)?;
        let defaults = raw.defaults.unwrap_or_else(|| Defaults {
            on_exists: raw.default_on_exists.unwrap_or_else(default_on_exists),
        });
        Ok(Self {
            version: raw.version,
            defaults,
            rules: raw.rules,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Defaults {
    #[serde(default = "default_on_exists")]
    pub on_exists: OnExists,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            on_exists: default_on_exists(),
        }
    }
}

fn default_on_exists() -> OnExists {
    OnExists::Skip
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OnExists {
    Skip,
    Overwrite,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingRule {
    pub id: String,
    #[serde(default)]
    pub auto: bool,
    pub when: Predicate,
    pub then: RoutingAction,
    #[serde(default)]
    pub on_exists: Option<OnExists>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Predicate {
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    FieldEquals { key: String, value: String },
    FieldExists(String),
    TagsInclude(Vec<String>),
    TagsExclude(Vec<String>),
    PathGlob(String),
}

impl<'de> Deserialize<'de> for Predicate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_predicate_value(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RoutingAction {
    #[serde(default)]
    pub move_to: Option<String>,
    #[serde(default)]
    pub set_fields: BTreeMap<String, String>,
    #[serde(default)]
    pub remove_fields: Vec<String>,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub remove_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RouteMatch {
    pub rule_id: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RouteLogEntry {
    pub note_path: String,
    pub rule_id: Option<String>,
    pub from_path: String,
    pub to_path: String,
    pub mutations_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RouteResult {
    pub from: String,
    pub to: String,
    pub rule_id: String,
    pub route_log: RouteLogEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteContext {
    pub path: String,
    pub fields: HashMap<String, String>,
    pub tags: Vec<String>,
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
    #[error("invalid frontmatter for {path}: {reason}")]
    InvalidFrontmatter { path: String, reason: String },
    #[error("note already archived: {path}")]
    AlreadyArchived { path: String },
    #[error("render error: {0}")]
    RenderError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("vault error: {0}")]
    VaultError(String),
}

#[derive(Debug, Clone)]
struct ParsedNote {
    mapping: Mapping,
    body: String,
    context: NoteContext,
}

#[derive(Debug, Clone)]
struct RoutePlan {
    rule_id: String,
    destination: String,
    updated_content: String,
    on_exists: OnExists,
    route_log: RouteLogEntry,
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
        let plan = self.plan_route(note_path, content)?;
        Ok(RouteMatch {
            rule_id: plan.rule_id,
            destination: plan.destination,
        })
    }

    /// Route a single note: read → match → mutate → write → move.
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

        let plan = self.plan_route(note_path, &content)?;
        let destination =
            resolve_destination(vault_root, note_path, &plan.destination, &plan.on_exists)?;

        engine
            .write(vault_root, &vault_path, None, &plan.updated_content)
            .map_err(|e| RoutingError::VaultError(e.to_string()))?;

        if destination != note_path {
            if matches!(plan.on_exists, OnExists::Overwrite)
                && vault_root.join(&destination).exists()
            {
                let destination_path = notesmith_core::VaultPath::new(destination.clone());
                engine
                    .delete(vault_root, &destination_path)
                    .map_err(|e| RoutingError::VaultError(e.to_string()))?;
            }

            let destination_path = notesmith_core::VaultPath::new(destination.clone());
            engine
                .move_path(vault_root, &vault_path, &destination_path)
                .map_err(|e| RoutingError::VaultError(e.to_string()))?;
        }

        let route_log = RouteLogEntry {
            to_path: destination.clone(),
            note_path: destination.clone(),
            ..plan.route_log
        };

        Ok(RouteResult {
            from: note_path.to_string(),
            to: destination,
            rule_id: plan.rule_id,
            route_log,
        })
    }

    fn plan_route(&self, note_path: &str, content: &str) -> Result<RoutePlan, RoutingError> {
        let parsed = parse_note(note_path, content)?;
        if is_archived(&parsed.mapping) {
            return Err(RoutingError::AlreadyArchived {
                path: note_path.to_string(),
            });
        }

        for rule in &self.config.rules {
            if evaluate(&rule.when, &parsed.context) {
                let mut draft_mapping = parsed.mapping.clone();
                apply_action_mutations(&mut draft_mapping, &rule.then);
                stamp_archived_fields(&mut draft_mapping);

                let draft_context = note_context_from_mapping(note_path, &draft_mapping);
                let destination = match &rule.then.move_to {
                    Some(template) => render_destination(template, &draft_context, note_path)?,
                    None => note_path.to_string(),
                };
                let updated_content = rebuild_content(&draft_mapping, &parsed.body);
                let route_log = build_route_log(note_path, &destination, &rule.id, &rule.then);

                return Ok(RoutePlan {
                    rule_id: rule.id.clone(),
                    destination,
                    updated_content,
                    on_exists: rule
                        .on_exists
                        .clone()
                        .unwrap_or_else(|| self.config.defaults.on_exists.clone()),
                    route_log,
                });
            }
        }

        Err(RoutingError::NoMatch {
            path: note_path.to_string(),
        })
    }
}

// ── Predicate parsing and evaluation ──────────────────────────────────────────

fn parse_predicate_value(value: &Value) -> Result<Predicate, String> {
    let Value::Mapping(mapping) = value else {
        return Err("predicates must be mappings".to_string());
    };
    parse_predicate_mapping(mapping)
}

fn parse_predicate_mapping(mapping: &Mapping) -> Result<Predicate, String> {
    if mapping.is_empty() {
        return Ok(Predicate::All(Vec::new()));
    }

    let mut predicates = Vec::new();
    for (raw_key, raw_value) in mapping {
        let key = yaml_key(raw_key).ok_or_else(|| "predicate keys must be strings".to_string())?;
        let predicate = match key.as_str() {
            "all" => Predicate::All(parse_predicate_list(raw_value)?),
            "any" => Predicate::Any(parse_predicate_list(raw_value)?),
            "not" => Predicate::Not(Box::new(parse_predicate_value(raw_value)?)),
            "field_exists" => Predicate::FieldExists(required_string(raw_value, "field_exists")?),
            "tags_include" => Predicate::TagsInclude(string_list(raw_value, "tags_include")?),
            "tags_exclude" => Predicate::TagsExclude(string_list(raw_value, "tags_exclude")?),
            "path" | "path_glob" => Predicate::PathGlob(required_string(raw_value, "path")?),
            _ if key.starts_with("field.") => Predicate::FieldEquals {
                key: key.trim_start_matches("field.").to_string(),
                value: required_string(raw_value, &key)?,
            },
            _ => Predicate::FieldEquals {
                key,
                value: required_string(raw_value, "field")?,
            },
        };
        predicates.push(predicate);
    }

    if predicates.len() == 1 {
        Ok(predicates
            .into_iter()
            .next()
            .unwrap_or(Predicate::All(Vec::new())))
    } else {
        Ok(Predicate::All(predicates))
    }
}

fn parse_predicate_list(value: &Value) -> Result<Vec<Predicate>, String> {
    let Value::Sequence(items) = value else {
        return Err("predicate combinators require a sequence".to_string());
    };

    items.iter().map(parse_predicate_value).collect()
}

fn required_string(value: &Value, label: &str) -> Result<String, String> {
    scalar_to_string(value)
        .ok_or_else(|| format!("{label} must be a scalar string, number, or bool"))
}

fn string_list(value: &Value, label: &str) -> Result<Vec<String>, String> {
    match value {
        Value::Sequence(items) => items
            .iter()
            .map(|item| {
                scalar_to_string(item).ok_or_else(|| {
                    format!("{label} entries must be scalar strings, numbers, or bools")
                })
            })
            .collect(),
        _ => Ok(vec![required_string(value, label)?]),
    }
}

pub fn evaluate(predicate: &Predicate, ctx: &NoteContext) -> bool {
    match predicate {
        Predicate::All(predicates) => predicates.iter().all(|predicate| evaluate(predicate, ctx)),
        Predicate::Any(predicates) => predicates.iter().any(|predicate| evaluate(predicate, ctx)),
        Predicate::Not(predicate) => !evaluate(predicate, ctx),
        Predicate::FieldEquals { key, value } => ctx.fields.get(key).is_some_and(|actual| {
            if value == "*" {
                !actual.trim().is_empty()
            } else {
                actual == value
            }
        }),
        Predicate::FieldExists(key) => ctx.fields.contains_key(key),
        Predicate::TagsInclude(tags) => tags
            .iter()
            .all(|tag| ctx.tags.iter().any(|item| item == tag)),
        Predicate::TagsExclude(tags) => tags
            .iter()
            .all(|tag| ctx.tags.iter().all(|item| item != tag)),
        Predicate::PathGlob(pattern) => path_matches(pattern, &ctx.path),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_note(note_path: &str, content: &str) -> Result<ParsedNote, RoutingError> {
    let (raw_frontmatter, body) = notesmith_vault::extract_frontmatter(content);
    let raw_frontmatter = raw_frontmatter.ok_or_else(|| RoutingError::NoFrontmatter {
        path: note_path.to_string(),
    })?;

    let mapping =
        notesmith_vault::parse_frontmatter_mapping(&raw_frontmatter).ok_or_else(|| {
            RoutingError::InvalidFrontmatter {
                path: note_path.to_string(),
                reason: "frontmatter must be a YAML mapping".to_string(),
            }
        })?;

    let context = note_context_from_mapping(note_path, &mapping);
    Ok(ParsedNote {
        mapping,
        body: body.to_string(),
        context,
    })
}

fn note_context_from_mapping(note_path: &str, mapping: &Mapping) -> NoteContext {
    let mut fields = HashMap::new();
    let mut tags = Vec::new();

    for (key, value) in mapping {
        let Some(key) = yaml_key(key) else {
            continue;
        };

        if key == "tags" {
            tags = parse_tags(value);
            continue;
        }

        fields.insert(key, first_scalar_value(value));
    }

    NoteContext {
        path: note_path.to_string(),
        fields,
        tags,
    }
}

fn apply_action_mutations(mapping: &mut Mapping, action: &RoutingAction) {
    for key in &action.remove_fields {
        mapping.remove(Value::String(key.clone()));
    }

    for (key, value) in &action.set_fields {
        mapping.insert(Value::String(key.clone()), Value::String(value.clone()));
    }

    let tags_key = Value::String("tags".to_string());
    let existing_tags = mapping.get(&tags_key).map(parse_tags).unwrap_or_default();
    let updated_tags = mutate_tags(existing_tags, action);
    if updated_tags.is_empty() {
        mapping.remove(&tags_key);
    } else {
        mapping.insert(
            tags_key,
            Value::Sequence(updated_tags.into_iter().map(Value::String).collect()),
        );
    }
}

fn mutate_tags(mut tags: Vec<String>, action: &RoutingAction) -> Vec<String> {
    tags.retain(|tag| !action.remove_tags.iter().any(|remove| remove == tag));
    for tag in &action.add_tags {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.clone());
        }
    }
    tags
}

fn stamp_archived_fields(mapping: &mut Mapping) {
    mapping.insert(Value::String("archived".to_string()), Value::Bool(true));
    mapping.insert(
        Value::String("archived-at".to_string()),
        Value::String(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
    );
}

fn rebuild_content(mapping: &Mapping, body: &str) -> String {
    let yaml =
        notesmith_vault::serialize_frontmatter(&notesmith_vault::sort_mapping(mapping.clone()));
    if body.is_empty() {
        format!("---\n{yaml}\n---\n")
    } else {
        format!("---\n{yaml}\n---\n{body}")
    }
}

fn render_destination(
    template: &str,
    ctx: &NoteContext,
    note_path: &str,
) -> Result<String, RoutingError> {
    let mut env = Environment::new();
    env.add_filter("unwikilink", |val: String| -> String {
        val.trim_start_matches("[[")
            .trim_end_matches("]]")
            .to_string()
    });
    env.add_filter("year", |val: String| -> String {
        val.get(..4).unwrap_or("").to_string()
    });
    env.add_filter("month", |val: String| -> String {
        val.get(5..7).unwrap_or("").to_string()
    });
    env.add_filter("slug", slug);

    let filename = Path::new(note_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    let field_object = serde_json::Value::Object(
        ctx.fields
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect(),
    );
    let tags_value = serde_json::Value::Array(
        ctx.tags
            .iter()
            .map(|tag| serde_json::Value::String(tag.clone()))
            .collect(),
    );

    let mut template_context = serde_json::Map::new();
    template_context.insert("field".to_string(), field_object.clone());
    template_context.insert("fields".to_string(), field_object);
    template_context.insert("tags".to_string(), tags_value);
    template_context.insert(
        "filename".to_string(),
        serde_json::Value::String(filename.clone()),
    );
    template_context.insert(
        "path".to_string(),
        serde_json::Value::String(ctx.path.clone()),
    );

    for (key, value) in &ctx.fields {
        template_context.insert(key.clone(), serde_json::Value::String(value.clone()));
    }

    let rendered = env
        .render_str(template, template_context)
        .map_err(|e| RoutingError::RenderError(e.to_string()))?;

    if rendered.ends_with('/') {
        Ok(format!("{rendered}{filename}"))
    } else {
        Ok(rendered)
    }
}

fn slug(value: String) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn build_route_log(
    note_path: &str,
    destination: &str,
    rule_id: &str,
    action: &RoutingAction,
) -> RouteLogEntry {
    RouteLogEntry {
        note_path: destination.to_string(),
        rule_id: Some(rule_id.to_string()),
        from_path: note_path.to_string(),
        to_path: destination.to_string(),
        mutations_json: serde_json::json!({
            "set_fields": action.set_fields,
            "remove_fields": action.remove_fields,
            "add_tags": action.add_tags,
            "remove_tags": action.remove_tags,
        }),
    }
}

fn resolve_destination(
    vault_root: &Path,
    source_path: &str,
    destination: &str,
    on_exists: &OnExists,
) -> Result<String, RoutingError> {
    if destination == source_path || !vault_root.join(destination).exists() {
        return Ok(destination.to_string());
    }

    match on_exists {
        OnExists::Skip => Err(RoutingError::DestinationExists {
            path: destination.to_string(),
        }),
        OnExists::Overwrite => Ok(destination.to_string()),
        OnExists::Rename => next_available_destination(vault_root, destination),
    }
}

fn next_available_destination(
    vault_root: &Path,
    destination: &str,
) -> Result<String, RoutingError> {
    let destination_path = Path::new(destination);
    let stem = destination_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| RoutingError::RenderError("destination filename is missing".to_string()))?;
    let extension = destination_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let parent = destination_path.parent().unwrap_or_else(|| Path::new(""));

    for index in 1usize.. {
        let candidate_name = format!("{stem} ({index}){extension}");
        let candidate = if parent.as_os_str().is_empty() {
            PathBuf::from(&candidate_name)
        } else {
            parent.join(&candidate_name)
        };
        let candidate_string = candidate.to_string_lossy().replace('\\', "/");
        if !vault_root.join(&candidate_string).exists() {
            return Ok(candidate_string);
        }
    }

    Err(RoutingError::RenderError(
        "could not find a collision-free destination".to_string(),
    ))
}

fn is_archived(mapping: &Mapping) -> bool {
    matches!(
        mapping.get(Value::String("archived".to_string())),
        Some(Value::Bool(true))
    )
}

fn parse_tags(value: &Value) -> Vec<String> {
    match value {
        Value::String(tag) => vec![tag.clone()],
        Value::Sequence(items) => items
            .iter()
            .filter_map(scalar_to_string)
            .filter(|tag| !tag.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn first_scalar_value(value: &Value) -> String {
    match value {
        Value::Sequence(items) => items.iter().find_map(scalar_to_string).unwrap_or_default(),
        _ => scalar_to_string(value).unwrap_or_default(),
    }
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

fn yaml_key(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

fn path_matches(pattern: &str, path: &str) -> bool {
    match Glob::new(pattern) {
        Ok(glob) => glob.compile_matcher().is_match(path),
        Err(_) => false,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(rules: Vec<RoutingRule>) -> RoutingConfig {
        RoutingConfig {
            version: 1,
            defaults: Defaults {
                on_exists: OnExists::Skip,
            },
            rules,
        }
    }

    fn make_rule(id: &str, when: &[(&str, &str)], move_to: &str) -> RoutingRule {
        let predicates = when
            .iter()
            .map(|(key, value)| Predicate::FieldEquals {
                key: (*key).to_string(),
                value: (*value).to_string(),
            })
            .collect::<Vec<_>>();

        RoutingRule {
            id: id.to_string(),
            auto: false,
            when: Predicate::All(predicates),
            then: RoutingAction {
                move_to: Some(move_to.to_string()),
                ..RoutingAction::default()
            },
            on_exists: None,
        }
    }

    fn context(path: &str, fields: &[(&str, &str)], tags: &[&str]) -> NoteContext {
        NoteContext {
            path: path.to_string(),
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
        }
    }

    fn mapping_from_yaml(yaml: &str) -> Mapping {
        notesmith_vault::parse_frontmatter_mapping(yaml).unwrap_or_default()
    }

    #[test]
    fn parses_new_dsl_config_with_nested_predicates() {
        let config: RoutingConfig = serde_yaml::from_str(
            r#"
version: 1
defaults:
  on_exists: rename
rules:
  - id: route-meeting
    auto: true
    when:
      all:
        - tags_include: [meeting]
        - any:
            - field.customer: "*"
            - field.account: "*"
        - not:
            tags_exclude: [inbox]
    then:
      move_to: "Customers/{{ field.customer }}/{{ filename }}"
      set_fields:
        status: filed
      remove_fields: [temp_notes]
      add_tags: [archived]
      remove_tags: [inbox]
"#,
        )
        .unwrap();

        assert_eq!(config.defaults.on_exists, OnExists::Rename);
        assert!(config.rules[0].auto);
        assert_eq!(
            config.rules[0].when,
            Predicate::All(vec![
                Predicate::TagsInclude(vec!["meeting".to_string()]),
                Predicate::Any(vec![
                    Predicate::FieldEquals {
                        key: "customer".to_string(),
                        value: "*".to_string(),
                    },
                    Predicate::FieldEquals {
                        key: "account".to_string(),
                        value: "*".to_string(),
                    },
                ]),
                Predicate::Not(Box::new(Predicate::TagsExclude(vec!["inbox".to_string()]))),
            ])
        );
    }

    #[test]
    fn parses_legacy_flat_when_as_implicit_all() {
        let config: RoutingConfig = serde_yaml::from_str(
            r#"
version: 1
default_on_exists: skip
rules:
  - id: external-meeting
    when:
      type: meeting
      customer: "*"
    then:
      move_to: "Customers/{{ customer }}/{{ filename }}"
"#,
        )
        .unwrap();

        assert_eq!(config.defaults.on_exists, OnExists::Skip);
        assert_eq!(
            config.rules[0].when,
            Predicate::All(vec![
                Predicate::FieldEquals {
                    key: "type".to_string(),
                    value: "meeting".to_string(),
                },
                Predicate::FieldEquals {
                    key: "customer".to_string(),
                    value: "*".to_string(),
                },
            ])
        );
    }

    #[test]
    fn evaluates_boolean_combinators_and_tags() {
        let predicate = Predicate::All(vec![
            Predicate::TagsInclude(vec!["meeting".to_string()]),
            Predicate::Any(vec![
                Predicate::FieldEquals {
                    key: "meeting_type".to_string(),
                    value: "external".to_string(),
                },
                Predicate::FieldEquals {
                    key: "priority".to_string(),
                    value: "high".to_string(),
                },
            ]),
            Predicate::Not(Box::new(Predicate::TagsInclude(vec![
                "archived".to_string(),
            ]))),
        ]);

        assert!(evaluate(
            &predicate,
            &context(
                "Inbox/meeting.md",
                &[("meeting_type", "external")],
                &["meeting", "inbox"],
            ),
        ));
        assert!(!evaluate(
            &predicate,
            &context(
                "Inbox/meeting.md",
                &[("meeting_type", "internal")],
                &["meeting", "archived"],
            ),
        ));
    }

    #[test]
    fn evaluates_field_exists_and_path_glob() {
        let predicate = Predicate::All(vec![
            Predicate::PathGlob("Inbox/**".to_string()),
            Predicate::FieldExists("customer".to_string()),
            Predicate::TagsExclude(vec!["archived".to_string()]),
        ]);

        assert!(evaluate(
            &predicate,
            &context("Inbox/idea.md", &[("customer", "")], &["inbox"]),
        ));
        assert!(!evaluate(
            &predicate,
            &context("General/idea.md", &[("customer", "Acme")], &["inbox"]),
        ));
    }

    #[test]
    fn renders_destination_with_field_namespace_tags_and_filename() {
        let result = render_destination(
            "Customers/{{ field.customer | unwikilink }}/{{ tags[0] }}/{{ filename }}",
            &context(
                "Inbox/Meeting with Acme.md",
                &[("customer", "[[Acme Corp]]")],
                &["meeting", "inbox"],
            ),
            "Inbox/Meeting with Acme.md",
        )
        .unwrap();

        assert_eq!(result, "Customers/Acme Corp/meeting/Meeting with Acme.md");
    }

    #[test]
    fn applies_field_and_tag_mutations_to_frontmatter() {
        let mut mapping = mapping_from_yaml(
            r#"
customer: "[[Acme Corp]]"
temp_notes: scratch
tags: [meeting, inbox]
"#,
        );
        let action = RoutingAction {
            set_fields: BTreeMap::from([("status".to_string(), "filed".to_string())]),
            remove_fields: vec!["temp_notes".to_string()],
            add_tags: vec!["archived".to_string()],
            remove_tags: vec!["inbox".to_string()],
            ..RoutingAction::default()
        };

        apply_action_mutations(&mut mapping, &action);

        assert_eq!(
            mapping.get(Value::String("status".to_string())),
            Some(&Value::String("filed".to_string()))
        );
        assert!(!mapping.contains_key(Value::String("temp_notes".to_string())));
        assert_eq!(
            mapping.get(Value::String("tags".to_string())),
            Some(&Value::Sequence(vec![
                Value::String("meeting".to_string()),
                Value::String("archived".to_string()),
            ]))
        );
    }

    #[test]
    fn preview_matches_new_dsl_rule() {
        let routing = RoutingEngine::from_config(make_config(vec![RoutingRule {
            id: "route-meeting".to_string(),
            auto: false,
            when: Predicate::All(vec![
                Predicate::TagsInclude(vec!["meeting".to_string()]),
                Predicate::FieldEquals {
                    key: "customer".to_string(),
                    value: "*".to_string(),
                },
                Predicate::FieldEquals {
                    key: "meeting_type".to_string(),
                    value: "external".to_string(),
                },
            ]),
            then: RoutingAction {
                move_to: Some(
                    "Customers/{{ field.customer | unwikilink }}/Meetings/{{ filename }}"
                        .to_string(),
                ),
                set_fields: BTreeMap::from([("status".to_string(), "filed".to_string())]),
                remove_fields: vec!["temp_notes".to_string()],
                add_tags: vec!["archived".to_string()],
                remove_tags: vec!["inbox".to_string()],
            },
            on_exists: None,
        }]));
        let content = "---\ncustomer: \"[[Acme Corp]]\"\nmeeting_type: external\ntags: [meeting, inbox]\ntemp_notes: scratch\n---\n# Meeting\n";

        let route_match = routing
            .preview("Inbox/Meeting with Acme.md", content)
            .unwrap();

        assert_eq!(route_match.rule_id, "route-meeting");
        assert_eq!(
            route_match.destination,
            "Customers/Acme Corp/Meetings/Meeting with Acme.md"
        );
    }

    #[test]
    fn apply_moves_note_and_persists_mutations_and_route_log() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("Inbox")).unwrap();
        std::fs::write(
            root.join("Inbox/Meeting with Acme.md"),
            "---\ncustomer: \"[[Acme Corp]]\"\nmeeting_type: external\ntags: [meeting, inbox]\ntemp_notes: scratch\n---\n# Meeting\n",
        )
        .unwrap();

        let routing = RoutingEngine::from_config(make_config(vec![RoutingRule {
            id: "route-meeting".to_string(),
            auto: false,
            when: Predicate::All(vec![
                Predicate::PathGlob("Inbox/**".to_string()),
                Predicate::TagsInclude(vec!["meeting".to_string()]),
                Predicate::FieldEquals {
                    key: "customer".to_string(),
                    value: "*".to_string(),
                },
                Predicate::FieldEquals {
                    key: "meeting_type".to_string(),
                    value: "external".to_string(),
                },
            ]),
            then: RoutingAction {
                move_to: Some(
                    "Customers/{{ field.customer | unwikilink }}/Meetings/{{ filename }}"
                        .to_string(),
                ),
                set_fields: BTreeMap::from([("status".to_string(), "filed".to_string())]),
                remove_fields: vec!["temp_notes".to_string()],
                add_tags: vec!["archived".to_string()],
                remove_tags: vec!["inbox".to_string()],
            },
            on_exists: None,
        }]));

        let engine = notesmith_vault::NativeVaultEngine;
        let result = routing
            .apply(root, "Inbox/Meeting with Acme.md", &engine)
            .unwrap();

        assert_eq!(result.from, "Inbox/Meeting with Acme.md");
        assert_eq!(
            result.to,
            "Customers/Acme Corp/Meetings/Meeting with Acme.md"
        );
        assert_eq!(result.rule_id, "route-meeting");
        assert_eq!(
            result.route_log.mutations_json,
            serde_json::json!({
                "set_fields": {"status": "filed"},
                "remove_fields": ["temp_notes"],
                "add_tags": ["archived"],
                "remove_tags": ["inbox"],
            })
        );

        let content =
            std::fs::read_to_string(root.join("Customers/Acme Corp/Meetings/Meeting with Acme.md"))
                .unwrap();
        assert!(content.contains("status: filed"));
        assert!(!content.contains("temp_notes"));
        assert!(content.contains("- meeting"));
        assert!(content.contains("- archived"));
        assert!(!content.contains("- inbox"));
        assert!(content.contains("archived: true"));
        assert!(content.contains("archived-at:"));
        assert!(content.contains("# Meeting"));
    }

    #[test]
    fn apply_renames_destination_when_conflict_policy_is_rename() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("Inbox")).unwrap();
        std::fs::create_dir_all(root.join("General")).unwrap();
        std::fs::write(root.join("Inbox/idea.md"), "---\ntype: note\n---\n# Idea\n").unwrap();
        std::fs::write(root.join("General/idea.md"), "existing").unwrap();

        let routing = RoutingEngine::from_config(RoutingConfig {
            version: 1,
            defaults: Defaults {
                on_exists: OnExists::Rename,
            },
            rules: vec![make_rule("note-general", &[("type", "note")], "General/")],
        });

        let engine = notesmith_vault::NativeVaultEngine;
        let result = routing.apply(root, "Inbox/idea.md", &engine).unwrap();

        assert_eq!(result.to, "General/idea (1).md");
        assert!(root.join("General/idea (1).md").exists());
        assert!(root.join("General/idea.md").exists());
    }

    #[test]
    fn preview_rejects_archived_notes() {
        let routing = RoutingEngine::from_config(make_config(vec![make_rule(
            "note-general",
            &[("type", "note")],
            "General/",
        )]));

        let result = routing.preview(
            "Inbox/test.md",
            "---\ntype: note\narchived: true\n---\nBody",
        );
        assert!(matches!(result, Err(RoutingError::AlreadyArchived { .. })));
    }

    #[test]
    fn preview_rejects_missing_frontmatter() {
        let routing = RoutingEngine::from_config(make_config(vec![]));
        let result = routing.preview("Inbox/test.md", "No frontmatter here");
        assert!(matches!(result, Err(RoutingError::NoFrontmatter { .. })));
    }

    #[test]
    fn preview_rejects_invalid_frontmatter_without_panicking() {
        let routing = RoutingEngine::from_config(make_config(vec![make_rule(
            "note-general",
            &[("type", "note")],
            "General/",
        )]));
        let result = routing.preview("Inbox/bad.md", "---\ntype: [note\n---\nBody\n");
        assert!(matches!(
            result,
            Err(RoutingError::InvalidFrontmatter { .. })
        ));
    }

    #[test]
    fn legacy_preview_still_matches_flat_field_rules() {
        let routing = RoutingEngine::from_config(make_config(vec![make_rule(
            "external-meeting",
            &[("type", "meeting"), ("meeting-kind", "external")],
            "Customers/{{ customer | unwikilink }}/External Meetings/",
        )]));
        let content = "---\ntype: meeting\nmeeting-kind: external\ncustomer: \"[[Acme Corp]]\"\n---\n# Meeting Notes\n";
        let route_match = routing
            .preview("Inbox/Meeting with Acme.md", content)
            .unwrap();

        assert_eq!(route_match.rule_id, "external-meeting");
        assert_eq!(
            route_match.destination,
            "Customers/Acme Corp/External Meetings/Meeting with Acme.md"
        );
    }
}
