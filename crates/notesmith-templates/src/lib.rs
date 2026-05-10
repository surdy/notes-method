//! notesmith-templates: Minijinja template engine, prompt specs, and template instantiation

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Data types ───────────────────────────────────────────────────────────────

fn default_text_type() -> String {
    "text".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSpec {
    pub name: String,
    #[serde(default = "default_text_type", rename = "type")]
    pub prompt_type: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSpec {
    pub name: String,
    pub description: String,
    pub output_path: String,
    #[serde(default)]
    pub prompts: Vec<PromptSpec>,
}

#[derive(Debug, Clone)]
pub struct TemplateMeta {
    pub spec: TemplateSpec,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RenderedTemplate {
    pub path: String,
    pub content: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("template not found: {name}")]
    NotFound { name: String },
    #[error("missing required prompts: {}", prompts.join(", "))]
    MissingPrompts { prompts: Vec<String> },
    #[error("render error: {0}")]
    RenderError(String),
    #[error("invalid template format in {path}: {reason}")]
    InvalidFormat { path: PathBuf, reason: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Template Engine ──────────────────────────────────────────────────────────

pub struct TemplateEngine {
    vault_root: PathBuf,
    db_path: Option<PathBuf>,
}

impl TemplateEngine {
    pub fn new(vault_root: PathBuf, db_path: Option<PathBuf>) -> Self {
        Self {
            vault_root,
            db_path,
        }
    }

    fn templates_dir(&self) -> PathBuf {
        self.vault_root.join("Assets").join("templates")
    }

    pub fn list_templates(&self) -> Result<Vec<TemplateMeta>, TemplateError> {
        let dir = self.templates_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut templates = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("j2") {
                match parse_template_file(&path) {
                    Ok((spec, _body)) => templates.push(TemplateMeta {
                        spec,
                        file_path: path,
                    }),
                    Err(_) => continue,
                }
            }
        }
        templates.sort_by(|a, b| a.spec.name.cmp(&b.spec.name));
        Ok(templates)
    }

    pub fn render(
        &self,
        name: &str,
        prompts: &HashMap<String, String>,
    ) -> Result<RenderedTemplate, TemplateError> {
        let templates = self.list_templates()?;
        let meta = templates
            .into_iter()
            .find(|m| m.spec.name == name)
            .ok_or_else(|| TemplateError::NotFound {
                name: name.to_string(),
            })?;

        let (spec, body) = parse_template_file(&meta.file_path)?;

        // Validate required prompts
        let missing: Vec<String> = spec
            .prompts
            .iter()
            .filter(|p| p.required && !prompts.contains_key(&p.name))
            .map(|p| p.name.clone())
            .collect();
        if !missing.is_empty() {
            return Err(TemplateError::MissingPrompts { prompts: missing });
        }

        let env = build_env(&self.db_path, prompts.clone());

        let path = env
            .render_str(&spec.output_path, minijinja::context! {})
            .map_err(|e| TemplateError::RenderError(e.to_string()))?;

        let content = env
            .render_str(&body, minijinja::context! {})
            .map_err(|e| TemplateError::RenderError(e.to_string()))?;

        Ok(RenderedTemplate { path, content })
    }

    pub fn instantiate(
        &self,
        name: &str,
        prompts: &HashMap<String, String>,
        engine: &dyn notesmith_core::VaultEngine,
    ) -> Result<RenderedTemplate, TemplateError> {
        let rendered = self.render(name, prompts)?;
        let note_path = notesmith_core::VaultPath::new(rendered.path.clone());
        let content = notesmith_vault::apply_save_pipeline(&rendered.content);
        engine
            .write(&self.vault_root, &note_path, None, &content)
            .map_err(|e| TemplateError::RenderError(e.to_string()))?;
        Ok(rendered)
    }
}

// ── Template parsing ─────────────────────────────────────────────────────────

fn parse_template_file(path: &Path) -> Result<(TemplateSpec, String), TemplateError> {
    let content = std::fs::read_to_string(path)?;

    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return Err(TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "missing opening ---".to_string(),
        });
    }

    let after_opening = &content[4..]; // skip "---\n"
    let closing_pos = after_opening
        .find("\n---\n")
        .or_else(|| after_opening.find("\n---\r\n"))
        .ok_or_else(|| TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "missing closing ---".to_string(),
        })?;

    let frontmatter_str = &after_opening[..closing_pos];
    let body_start = closing_pos + "\n---\n".len();
    let body = &after_opening[body_start..];

    let yaml: serde_yaml::Value =
        serde_yaml::from_str(frontmatter_str).map_err(|e| TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: format!("YAML parse error: {e}"),
        })?;

    let notesmith_val = yaml
        .get("notesmith")
        .ok_or_else(|| TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "missing 'notesmith' key in frontmatter".to_string(),
        })?;

    let spec: TemplateSpec = serde_yaml::from_value(notesmith_val.clone()).map_err(|e| {
        TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: format!("invalid TemplateSpec: {e}"),
        }
    })?;

    Ok((spec, body.to_string()))
}

// ── Minijinja environment ────────────────────────────────────────────────────

fn build_env<'a>(
    db_path: &Option<PathBuf>,
    prompts: HashMap<String, String>,
) -> minijinja::Environment<'a> {
    use chrono::Local;
    let mut env = minijinja::Environment::new();

    let now = Local::now();
    env.add_global(
        "today",
        minijinja::Value::from(now.format("%Y-%m-%d").to_string()),
    );
    env.add_global(
        "tomorrow",
        minijinja::Value::from(
            (now + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        ),
    );
    env.add_global(
        "yesterday",
        minijinja::Value::from(
            (now - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        ),
    );
    env.add_global(
        "now",
        minijinja::Value::from(now.format("%Y-%m-%d %H:%M:%S").to_string()),
    );

    // Filters
    env.add_filter("slug", slug);
    env.add_filter("as_wikilink", |val: String| format!("[[{val}]]"));
    env.add_filter("title_case", title_case);

    // Functions
    env.add_function("slug", slug);
    env.add_function("title_case", title_case);
    env.add_function("next_id", || {
        chrono::Local::now().timestamp_millis().to_string()
    });

    // query() helper
    if let Some(db) = db_path.clone() {
        env.add_function("query", move |sql: String| -> minijinja::Value {
            query_db(&db, &sql)
        });
    }

    // prompt() accessor
    let prompts_clone = prompts.clone();
    env.add_function("prompt", move |name: String| -> String {
        prompts_clone.get(&name).cloned().unwrap_or_default()
    });

    // Add prompts directly as globals
    for (k, v) in &prompts {
        env.add_global(k.clone(), minijinja::Value::from(v.clone()));
    }

    env
}

fn slug(s: String) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn title_case(s: String) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn query_db(db_path: &Path, sql: &str) -> minijinja::Value {
    use rusqlite::Connection;

    let conn =
        match Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(c) => c,
            Err(_) => return minijinja::Value::from(Vec::<minijinja::Value>::new()),
        };
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return minijinja::Value::from(Vec::<minijinja::Value>::new()),
    };
    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    let mapped = match stmt.query_map([], |row| {
        let mut map = std::collections::BTreeMap::new();
        for (i, col) in columns.iter().enumerate() {
            let val: rusqlite::types::Value = row.get(i)?;
            let mj_val = match val {
                rusqlite::types::Value::Null => minijinja::Value::UNDEFINED,
                rusqlite::types::Value::Integer(n) => minijinja::Value::from(n),
                rusqlite::types::Value::Real(f) => minijinja::Value::from(f),
                rusqlite::types::Value::Text(s) => minijinja::Value::from(s),
                rusqlite::types::Value::Blob(_) => minijinja::Value::from("<blob>"),
            };
            map.insert(col.clone(), mj_val);
        }
        Ok(minijinja::Value::from(
            map.into_iter()
                .collect::<std::collections::BTreeMap<_, _>>(),
        ))
    }) {
        Ok(rows) => rows,
        Err(_) => return minijinja::Value::from(Vec::<minijinja::Value>::new()),
    };
    let rows: Vec<minijinja::Value> = mapped.filter_map(|r| r.ok()).collect();

    minijinja::Value::from(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden_vault() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("golden-vault")
    }

    fn engine() -> TemplateEngine {
        TemplateEngine::new(golden_vault(), None)
    }

    #[test]
    fn list_templates_returns_all_nine() {
        let engine = engine();
        let templates = engine.list_templates().unwrap();
        assert_eq!(templates.len(), 9);
        let names: Vec<&str> = templates.iter().map(|t| t.spec.name.as_str()).collect();
        assert!(names.contains(&"generic-note"));
        assert!(names.contains(&"daily-note"));
        assert!(names.contains(&"external-meeting"));
        assert!(names.contains(&"internal-meeting"));
        assert!(names.contains(&"account-info"));
        assert!(names.contains(&"customer-index"));
        assert!(names.contains(&"glossary"));
        assert!(names.contains(&"milestones"));
        assert!(names.contains(&"stream"));
    }

    #[test]
    fn render_generic_note_with_title() {
        let engine = engine();
        let prompts = HashMap::from([("title".to_string(), "Hello World".to_string())]);
        let rendered = engine.render("generic-note", &prompts).unwrap();
        assert_eq!(rendered.path, "Inbox/hello-world.md");
        assert!(rendered.content.contains("# Hello World"));
    }

    #[test]
    fn render_generic_note_with_folder() {
        let engine = engine();
        let prompts = HashMap::from([
            ("title".to_string(), "My Note".to_string()),
            ("folder".to_string(), "Customers/Acme".to_string()),
        ]);
        let rendered = engine.render("generic-note", &prompts).unwrap();
        assert_eq!(rendered.path, "Customers/Acme/my-note.md");
    }

    #[test]
    fn render_daily_note_uses_today() {
        let engine = engine();
        let prompts = HashMap::new();
        let rendered = engine.render("daily-note", &prompts).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(rendered.path, format!("Inbox/Daily/{today}.md"));
        assert!(rendered.content.contains(&format!("# {today}")));
        assert!(rendered.content.contains(&format!("date: {today}")));
    }

    #[test]
    fn render_external_meeting() {
        let engine = engine();
        let prompts = HashMap::from([
            ("customer".to_string(), "Acme".to_string()),
            ("title".to_string(), "Check-in".to_string()),
        ]);
        let rendered = engine.render("external-meeting", &prompts).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(
            rendered.path,
            format!("Customers/Acme/External Meetings/{today} Check-in.md")
        );
        assert!(rendered.content.contains("**Customer:** Acme"));
    }

    #[test]
    fn render_stream_uses_title_case() {
        let engine = engine();
        let prompts = HashMap::from([
            ("customer".to_string(), "Acme".to_string()),
            ("title".to_string(), "migration to v2".to_string()),
        ]);
        let rendered = engine.render("stream", &prompts).unwrap();
        assert_eq!(rendered.path, "Customers/Acme/Streams/Migration To V2.md");
        assert!(rendered.content.contains("# Migration To V2"));
    }

    #[test]
    fn render_missing_required_prompt_returns_error() {
        let engine = engine();
        let prompts = HashMap::new();
        let result = engine.render("generic-note", &prompts);
        match result {
            Err(TemplateError::MissingPrompts { prompts }) => {
                assert_eq!(prompts, vec!["title"]);
            }
            other => panic!("expected MissingPrompts, got {other:?}"),
        }
    }

    #[test]
    fn render_unknown_template_returns_not_found() {
        let engine = engine();
        let result = engine.render("nonexistent", &HashMap::new());
        assert!(matches!(result, Err(TemplateError::NotFound { .. })));
    }

    #[test]
    fn slug_function_works() {
        assert_eq!(slug("Hello World".to_string()), "hello-world");
        assert_eq!(slug("  Multiple   Spaces  ".to_string()), "multiple-spaces");
        assert_eq!(slug("Special!@#chars".to_string()), "special-chars");
    }

    #[test]
    fn title_case_function_works() {
        assert_eq!(title_case("hello world".to_string()), "Hello World");
        assert_eq!(title_case("already OK".to_string()), "Already OK");
    }

    #[test]
    fn as_wikilink_filter_wraps() {
        let prompts = HashMap::from([("title".to_string(), "Test Note".to_string())]);
        let env = build_env(&None, prompts);
        let result = env
            .render_str("{{ title | as_wikilink }}", minijinja::context! {})
            .unwrap();
        assert_eq!(result, "[[Test Note]]");
    }
}
