//! notesmith-templates: Minijinja template engine, prompt specs, and template instantiation

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use chrono::{Datelike, Local};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

const DEFAULT_PRE_RENDER_HOOK_TIMEOUT: Duration = Duration::from_secs(10);

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
    #[serde(default)]
    pub default: Option<String>,
    /// For `type: field-picker`, the registry field whose values to suggest.
    /// Defaults to the prompt name — set it when they differ, e.g. a `customer`
    /// prompt that picks from the plural `customers` list field.
    #[serde(default)]
    pub field: Option<String>,
}

impl PromptSpec {
    /// The `fields.toml` key backing this prompt's suggestions.
    pub fn suggestion_field(&self) -> &str {
        self.field.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub prompts: Vec<PromptSpec>,
    #[serde(default)]
    pub context_queries: HashMap<String, String>,
    #[serde(default)]
    pub pre_render_hook: Option<String>,
}

pub type TemplateSpec = TemplateMetadata;

type JsonContext = HashMap<String, JsonValue>;

#[derive(Debug, Clone)]
pub struct TemplateMeta {
    pub spec: TemplateMetadata,
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

    fn template_dirs(&self) -> Vec<PathBuf> {
        let legacy_dir = self.vault_root.join("Assets").join("templates");
        let notesmith_dir = self.vault_root.join(".notesmith").join("templates");
        [legacy_dir, notesmith_dir]
            .into_iter()
            .filter(|path| path.exists())
            .collect()
    }

    pub fn list_templates(&self) -> Result<Vec<TemplateMeta>, TemplateError> {
        let mut templates = BTreeMap::new();
        for dir in self.template_dirs() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if !is_template_file(&path) {
                    continue;
                }
                match parse_template_file(&path) {
                    Ok((spec, _body)) => {
                        templates.insert(
                            spec.name.clone(),
                            TemplateMeta {
                                spec,
                                file_path: path,
                            },
                        );
                    }
                    Err(_) => continue,
                }
            }
        }
        Ok(templates.into_values().collect())
    }

    pub fn render(
        &self,
        name: &str,
        prompts: &HashMap<String, String>,
    ) -> Result<RenderedTemplate, TemplateError> {
        self.render_with_output_path(name, prompts, None, DEFAULT_PRE_RENDER_HOOK_TIMEOUT)
    }

    pub fn render_to_path(
        &self,
        name: &str,
        prompts: &HashMap<String, String>,
        output_path: &str,
    ) -> Result<RenderedTemplate, TemplateError> {
        self.render_with_output_path(
            name,
            prompts,
            Some(output_path),
            DEFAULT_PRE_RENDER_HOOK_TIMEOUT,
        )
    }

    pub fn render_text(
        &self,
        template: &str,
        prompts: &HashMap<String, String>,
    ) -> Result<String, TemplateError> {
        let env = build_env(&self.db_path, prompts.clone());
        let mut context = build_static_context_json();
        context.insert(
            "vault".to_string(),
            JsonValue::String(vault_name(&self.vault_root)),
        );
        context.insert("filename".to_string(), JsonValue::String(String::new()));
        extend_prompt_context(&mut context, prompts);
        render_string(&env, template, &context)
    }

    fn render_with_output_path(
        &self,
        name: &str,
        prompts: &HashMap<String, String>,
        output_path_override: Option<&str>,
        hook_timeout: Duration,
    ) -> Result<RenderedTemplate, TemplateError> {
        let templates = self.list_templates()?;
        let meta = templates
            .into_iter()
            .find(|m| m.spec.name == name)
            .ok_or_else(|| TemplateError::NotFound {
                name: name.to_string(),
            })?;

        let (spec, body) = parse_template_file(&meta.file_path)?;
        let merged_prompts = merge_prompt_values(&spec.prompts, prompts);
        validate_required_prompts(&spec.prompts, &merged_prompts)?;

        let env = build_env(&self.db_path, merged_prompts.clone());
        let mut context = build_static_context_json();
        context.insert(
            "vault".to_string(),
            JsonValue::String(vault_name(&self.vault_root)),
        );
        context.insert("filename".to_string(), JsonValue::String(String::new()));
        extend_prompt_context(&mut context, &merged_prompts);
        extend_sql_context(&mut context, &spec.context_queries, self.db_path.as_deref());

        if let Some(hook_script) = spec.pre_render_hook.as_deref() {
            context.extend(run_pre_render_hook_sync(
                &self.vault_root,
                hook_script,
                &context,
                hook_timeout,
            ));
        }

        let path = if let Some(output_path) = output_path_override {
            output_path.to_string()
        } else {
            let output_path =
                spec.output_path
                    .as_deref()
                    .ok_or_else(|| TemplateError::InvalidFormat {
                        path: meta.file_path.clone(),
                        reason: "missing output_path".to_string(),
                    })?;
            let initial_path = render_string(&env, output_path, &context)?;
            set_filename_context(&mut context, &initial_path);
            render_string(&env, output_path, &context)?
        };
        set_filename_context(&mut context, &path);
        let content = render_string(&env, &body, &context)?;

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

fn parse_template_file(path: &Path) -> Result<(TemplateMetadata, String), TemplateError> {
    let content = std::fs::read_to_string(path)?;
    if !content.starts_with("---") {
        return Err(TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "missing opening ---".to_string(),
        });
    }

    let (frontmatter_str, body) = notesmith_vault::extract_frontmatter(&content);
    let Some(frontmatter_str) = frontmatter_str else {
        return Err(TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "missing closing ---".to_string(),
        });
    };

    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&frontmatter_str).map_err(|e| TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: format!("YAML parse error: {e}"),
        })?;

    let metadata_value = yaml
        .get("notesmith")
        .cloned()
        .unwrap_or_else(|| yaml.clone());
    let spec: TemplateMetadata =
        serde_yaml::from_value(metadata_value).map_err(|e| TemplateError::InvalidFormat {
            path: path.to_path_buf(),
            reason: format!("invalid TemplateMetadata: {e}"),
        })?;

    Ok((spec, body.to_string()))
}

// ── Context building ─────────────────────────────────────────────────────────

pub fn build_static_context() -> HashMap<String, minijinja::Value> {
    json_context_to_minijinja(&build_static_context_json())
}

fn build_static_context_json() -> JsonContext {
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let mut ctx = HashMap::new();
    ctx.insert("date".into(), JsonValue::String(date.clone()));
    ctx.insert(
        "time".into(),
        JsonValue::String(now.format("%H:%M").to_string()),
    );
    ctx.insert("datetime".into(), JsonValue::String(now.to_rfc3339()));
    ctx.insert(
        "day_name".into(),
        JsonValue::String(now.format("%A").to_string()),
    );
    ctx.insert(
        "week".into(),
        JsonValue::String(now.format("%G-W%V").to_string()),
    );
    ctx.insert(
        "month".into(),
        JsonValue::String(now.format("%Y-%m").to_string()),
    );
    ctx.insert(
        "quarter".into(),
        JsonValue::String(format!(
            "{}-Q{}",
            now.format("%Y"),
            (now.month() - 1) / 3 + 1
        )),
    );
    ctx.insert(
        "year".into(),
        JsonValue::String(now.format("%Y").to_string()),
    );

    ctx.insert("today".into(), JsonValue::String(date));
    ctx.insert(
        "tomorrow".into(),
        JsonValue::String(
            (now + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        ),
    );
    ctx.insert(
        "yesterday".into(),
        JsonValue::String(
            (now - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        ),
    );
    ctx.insert(
        "now".into(),
        JsonValue::String(now.format("%Y-%m-%d %H:%M:%S").to_string()),
    );
    ctx
}

pub fn execute_context_queries(
    conn: &rusqlite::Connection,
    queries: &HashMap<String, String>,
) -> HashMap<String, minijinja::Value> {
    json_context_to_minijinja(&execute_context_queries_json(conn, queries))
}

fn execute_context_queries_json(
    conn: &rusqlite::Connection,
    queries: &HashMap<String, String>,
) -> JsonContext {
    let mut results = HashMap::new();
    for (name, sql) in queries {
        match execute_readonly_query(conn, sql) {
            Ok(rows) => {
                results.insert(
                    name.clone(),
                    serde_json::to_value(rows).unwrap_or(JsonValue::Array(vec![])),
                );
            }
            Err(error) => {
                tracing::warn!(query_name = %name, error = %error, "context query failed");
                results.insert(name.clone(), JsonValue::Array(vec![]));
            }
        }
    }
    results
}

fn execute_readonly_query(
    conn: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<HashMap<String, JsonValue>>, rusqlite::Error> {
    let trimmed = sql.trim_start();
    if !(trimmed.starts_with("SELECT")
        || trimmed.starts_with("select")
        || trimmed.starts_with("WITH")
        || trimmed.starts_with("with"))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }

    let mut stmt = conn.prepare(sql)?;
    if !stmt.readonly() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |row| {
        let mut map = HashMap::new();
        for (i, name) in column_names.iter().enumerate() {
            let value: rusqlite::types::Value = row.get(i)?;
            let json_val = match value {
                rusqlite::types::Value::Null => JsonValue::Null,
                rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                rusqlite::types::Value::Real(f) => serde_json::json!(f),
                rusqlite::types::Value::Text(s) => serde_json::json!(s),
                rusqlite::types::Value::Blob(_) => JsonValue::Null,
            };
            map.insert(name.clone(), json_val);
        }
        Ok(map)
    })?;
    rows.collect()
}

pub async fn run_pre_render_hook(
    vault_root: &Path,
    hook_script: &str,
    current_context: &JsonContext,
) -> JsonContext {
    run_pre_render_hook_with_timeout(
        vault_root,
        hook_script,
        current_context,
        DEFAULT_PRE_RENDER_HOOK_TIMEOUT,
    )
    .await
}

async fn run_pre_render_hook_with_timeout(
    vault_root: &Path,
    hook_script: &str,
    current_context: &JsonContext,
    timeout: Duration,
) -> JsonContext {
    let script_path = vault_root.join(hook_script);
    if !script_path.exists() {
        tracing::warn!(script = %hook_script, "pre_render_hook script not found");
        return HashMap::new();
    }

    let context_json = serde_json::to_string(current_context).unwrap_or_default();
    match tokio::process::Command::new("sh")
        .arg(&script_path)
        .current_dir(vault_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(context_json.as_bytes()).await;
            }

            let stdout_task = read_stream(child.stdout.take());
            let stderr_task = read_stream(child.stderr.take());

            match tokio::time::timeout(timeout, child.wait()).await {
                Ok(Ok(_status)) => {
                    let stdout = join_stream(stdout_task).await;
                    let stderr = join_stream(stderr_task).await;
                    if !stderr.trim().is_empty() {
                        tracing::debug!(script = %hook_script, stderr = %stderr.trim(), "pre_render_hook stderr");
                    }
                    serde_json::from_str(&stdout).unwrap_or_default()
                }
                Ok(Err(error)) => {
                    tracing::warn!(script = %hook_script, error = %error, "pre_render_hook failed");
                    HashMap::new()
                }
                Err(_) => {
                    kill_child_processes(&mut child).await;
                    stdout_task.abort();
                    stderr_task.abort();
                    tracing::warn!(script = %hook_script, "pre_render_hook timed out or failed");
                    HashMap::new()
                }
            }
        }
        Err(error) => {
            tracing::warn!(script = %hook_script, error = %error, "failed to spawn pre_render_hook");
            HashMap::new()
        }
    }
}

fn run_pre_render_hook_sync(
    vault_root: &Path,
    hook_script: &str,
    current_context: &JsonContext,
    timeout: Duration,
) -> JsonContext {
    let vault_root = vault_root.to_path_buf();
    let hook_script = hook_script.to_string();
    let current_context = current_context.clone();
    std::thread::spawn(move || {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(run_pre_render_hook_with_timeout(
                &vault_root,
                &hook_script,
                &current_context,
                timeout,
            )),
            Err(error) => {
                tracing::warn!(error = %error, "failed to build pre_render_hook runtime");
                HashMap::new()
            }
        }
    })
    .join()
    .unwrap_or_default()
}

// ── Minijinja environment ────────────────────────────────────────────────────

fn build_env<'a>(
    db_path: &Option<PathBuf>,
    prompts: HashMap<String, String>,
) -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();

    env.add_filter("slug", slug);
    env.add_filter("as_wikilink", |val: String| format!("[[{val}]]"));
    env.add_filter("title_case", title_case);

    env.add_function("slug", slug);
    env.add_function("title_case", title_case);
    env.add_function("next_id", || Local::now().timestamp_millis().to_string());

    if let Some(db) = db_path.clone() {
        env.add_function("query", move |sql: String| -> minijinja::Value {
            query_db(&db, &sql)
        });
    }

    let prompts_clone = prompts.clone();
    env.add_function("prompt", move |name: String| -> String {
        prompts_clone.get(&name).cloned().unwrap_or_default()
    });

    for (key, value) in &prompts {
        env.add_global(key.clone(), minijinja::Value::from(value.clone()));
    }

    env
}

fn render_string(
    env: &minijinja::Environment<'_>,
    template: &str,
    context: &JsonContext,
) -> Result<String, TemplateError> {
    env.render_str(template, context)
        .map_err(|error| TemplateError::RenderError(error.to_string()))
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
    let conn = match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(conn) => conn,
        Err(_) => return minijinja::Value::from_serialize(Vec::<JsonValue>::new()),
    };

    match execute_readonly_query(&conn, sql) {
        Ok(rows) => minijinja::Value::from_serialize(rows),
        Err(_) => minijinja::Value::from_serialize(Vec::<JsonValue>::new()),
    }
}

fn json_context_to_minijinja(context: &JsonContext) -> HashMap<String, minijinja::Value> {
    context
        .iter()
        .map(|(key, value)| (key.clone(), minijinja::Value::from_serialize(value.clone())))
        .collect()
}

fn merge_prompt_values(
    specs: &[PromptSpec],
    prompts: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = prompts.clone();
    for spec in specs {
        if !merged.contains_key(&spec.name) {
            if let Some(default) = &spec.default {
                merged.insert(spec.name.clone(), default.clone());
            }
        }
    }
    merged
}

fn validate_required_prompts(
    specs: &[PromptSpec],
    prompts: &HashMap<String, String>,
) -> Result<(), TemplateError> {
    let missing: Vec<String> = specs
        .iter()
        .filter(|prompt| prompt.required && !prompts.contains_key(&prompt.name))
        .map(|prompt| prompt.name.clone())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(TemplateError::MissingPrompts { prompts: missing })
    }
}

fn extend_prompt_context(context: &mut JsonContext, prompts: &HashMap<String, String>) {
    for (key, value) in prompts {
        context.insert(key.clone(), JsonValue::String(value.clone()));
    }
    context.insert(
        "prompt".to_string(),
        serde_json::to_value(prompts).unwrap_or_else(|_| JsonValue::Object(Default::default())),
    );
}

fn extend_sql_context(
    context: &mut JsonContext,
    queries: &HashMap<String, String>,
    db_path: Option<&Path>,
) {
    if queries.is_empty() {
        return;
    }

    let query_results = match db_path {
        Some(path) => match rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(conn) => execute_context_queries_json(&conn, queries),
            Err(error) => {
                tracing::warn!(error = %error, "failed to open template context database");
                empty_context_query_results(queries)
            }
        },
        None => empty_context_query_results(queries),
    };

    context.extend(query_results);
}

fn empty_context_query_results(queries: &HashMap<String, String>) -> JsonContext {
    queries
        .keys()
        .map(|name| (name.clone(), JsonValue::Array(vec![])))
        .collect()
}

fn set_filename_context(context: &mut JsonContext, rendered_path: &str) {
    let filename = Path::new(rendered_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    context.insert("filename".to_string(), JsonValue::String(filename));
}

fn vault_name(vault_root: &Path) -> String {
    vault_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_else(|| vault_root.as_os_str().to_str().unwrap_or(""))
        .to_string()
}

fn is_template_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("j2") | Some("md")
    )
}

fn read_stream<R>(stream: Option<R>) -> tokio::task::JoinHandle<std::io::Result<String>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;

        let Some(mut stream) = stream else {
            return Ok(String::new());
        };
        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).await?;
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    })
}

async fn join_stream(handle: tokio::task::JoinHandle<std::io::Result<String>>) -> String {
    match handle.await {
        Ok(Ok(output)) => output,
        _ => String::new(),
    }
}

async fn kill_child_processes(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{id}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rusqlite::Connection;
    use std::time::{Duration, Instant};

    fn golden_vault() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("golden-vault")
    }

    fn engine() -> TemplateEngine {
        TemplateEngine::new(golden_vault(), None)
    }

    struct TestVault {
        root: PathBuf,
    }

    impl TestVault {
        fn new(name: &str) -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("test-work")
                .join(format!(
                    "{name}-{}-{}",
                    std::process::id(),
                    Utc::now().timestamp_nanos_opt().unwrap_or_default()
                ));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn create_template(&self, relative_path: &str, content: &str) {
            self.write_file(relative_path, content);
        }

        fn write_file(&self, relative_path: &str, content: &str) {
            let path = self.root.join(relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, content).unwrap();
        }

        fn db_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }
    }

    impl Drop for TestVault {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn list_templates_returns_the_work_notes_kit() {
        let engine = engine();
        let templates = engine.list_templates().unwrap();
        let names: Vec<&str> = templates.iter().map(|t| t.spec.name.as_str()).collect();

        for expected in [
            "generic-note",
            "daily",
            "weekly",
            "quarterly",
            "internal-meeting",
            "external-meeting",
            "stream",
            "customer",
            "person",
        ] {
            assert!(names.contains(&expected), "missing template {expected}");
        }
        assert_eq!(templates.len(), 9, "unexpected templates: {names:?}");
    }

    #[test]
    fn build_static_context_contains_expected_keys() {
        let context = build_static_context();

        for key in [
            "date", "time", "datetime", "day_name", "week", "month", "quarter", "year",
        ] {
            assert!(context.contains_key(key), "missing static key {key}");
        }
    }

    #[test]
    fn execute_context_queries_returns_row_objects() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE tasks (text TEXT, status_group TEXT)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO tasks (text, status_group) VALUES ('Plan release', 'open'), ('Ship release', 'done')",
            [],
        )
        .unwrap();

        let queries = HashMap::from([(
            "open_tasks".to_string(),
            "SELECT text, status_group FROM tasks WHERE status_group = 'open'".to_string(),
        )]);
        let results = execute_context_queries(&conn, &queries);

        let mut env = minijinja::Environment::new();
        env.add_global("open_tasks", results.get("open_tasks").unwrap().clone());
        let rendered = env
            .render_str(
                "{% for row in open_tasks %}{{ row.text }}={{ row.status_group }}{% endfor %}",
                minijinja::context! {},
            )
            .unwrap();

        assert_eq!(rendered, "Plan release=open");
    }

    #[test]
    fn execute_readonly_query_rejects_non_select_queries() {
        let conn = Connection::open_in_memory().unwrap();
        let error = execute_readonly_query(&conn, "DELETE FROM tasks").unwrap_err();
        assert!(matches!(error, rusqlite::Error::InvalidQuery));
    }

    #[test]
    fn execute_context_queries_failed_query_returns_empty_array() {
        let conn = Connection::open_in_memory().unwrap();
        let queries = HashMap::from([(
            "broken".to_string(),
            "SELECT missing_column FROM missing_table".to_string(),
        )]);
        let results = execute_context_queries(&conn, &queries);

        let mut env = minijinja::Environment::new();
        env.add_global("broken", results.get("broken").unwrap().clone());
        let rendered = env
            .render_str("{{ broken | length }}", minijinja::context! {})
            .unwrap();

        assert_eq!(rendered, "0");
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
        let rendered = engine.render("daily", &prompts).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(rendered.path, format!("Daily/{today}.md"));
        assert!(rendered.content.contains(&format!("# {today}")));
        assert!(rendered.content.contains(&format!("date: {today}")));
    }

    /// Parse the note frontmatter a template emits — the wikilink lists in the
    /// Work Notes kit are only useful if they are valid YAML sequences.
    fn rendered_frontmatter(content: &str) -> serde_yaml::Mapping {
        let (frontmatter, _) = notesmith_vault::extract_frontmatter(content);
        let frontmatter = frontmatter.expect("rendered note should carry frontmatter");
        serde_yaml::from_str(&frontmatter)
            .unwrap_or_else(|error| panic!("rendered frontmatter is not valid YAML: {error}"))
    }

    fn sequence(frontmatter: &serde_yaml::Mapping, key: &str) -> Vec<String> {
        match frontmatter.get(serde_yaml::Value::from(key)) {
            Some(serde_yaml::Value::Sequence(items)) => items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect(),
            other => panic!("expected `{key}` to be a sequence, got {other:?}"),
        }
    }

    #[test]
    fn render_external_meeting_lands_in_inbox_with_one_customer() {
        let engine = engine();
        let prompts = HashMap::from([
            ("customer".to_string(), "Acme Corp".to_string()),
            ("title".to_string(), "Check-in".to_string()),
        ]);
        let rendered = engine.render("external-meeting", &prompts).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        assert_eq!(
            rendered.path,
            format!("Inbox/{today} - Acme Corp - Check-in.md")
        );

        let frontmatter = rendered_frontmatter(&rendered.content);
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("kind")),
            Some(&serde_yaml::Value::from("meeting"))
        );
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("audience")),
            Some(&serde_yaml::Value::from("external"))
        );
        assert_eq!(sequence(&frontmatter, "customers"), vec!["[[Acme Corp]]"]);
        assert!(
            sequence(&frontmatter, "streams").is_empty(),
            "an unanswered optional stream prompt yields an empty list"
        );
        assert!(sequence(&frontmatter, "attendees").is_empty());
    }

    #[test]
    fn render_external_meeting_wikilinks_the_optional_stream() {
        let engine = engine();
        let prompts = HashMap::from([
            ("customer".to_string(), "Acme Corp".to_string()),
            ("title".to_string(), "Check-in".to_string()),
            ("stream".to_string(), "Migration to v2".to_string()),
        ]);
        let rendered = engine.render("external-meeting", &prompts).unwrap();

        let frontmatter = rendered_frontmatter(&rendered.content);
        assert_eq!(
            sequence(&frontmatter, "streams"),
            vec!["[[Migration to v2]]"]
        );
    }

    #[test]
    fn render_stream_lands_in_inbox_for_routing() {
        let engine = engine();
        let prompts = HashMap::from([
            ("customer".to_string(), "Acme Corp".to_string()),
            ("title".to_string(), "Acme Corp - Renewal 2026".to_string()),
            ("priority".to_string(), "P1".to_string()),
        ]);
        let rendered = engine.render("stream", &prompts).unwrap();

        assert_eq!(rendered.path, "Inbox/Acme Corp - Renewal 2026.md");
        assert!(rendered.content.contains("# Acme Corp - Renewal 2026"));

        let frontmatter = rendered_frontmatter(&rendered.content);
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("kind")),
            Some(&serde_yaml::Value::from("stream"))
        );
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("status")),
            Some(&serde_yaml::Value::from("active"))
        );
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("priority")),
            Some(&serde_yaml::Value::from("P1"))
        );
        assert_eq!(sequence(&frontmatter, "customers"), vec!["[[Acme Corp]]"]);
    }

    #[test]
    fn render_internal_stream_emits_an_empty_customers_list() {
        let engine = engine();
        let prompts = HashMap::from([(
            "title".to_string(),
            "Internal - Support Process Redesign".to_string(),
        )]);
        let rendered = engine.render("stream", &prompts).unwrap();

        let frontmatter = rendered_frontmatter(&rendered.content);
        assert!(sequence(&frontmatter, "customers").is_empty());
        assert!(
            !frontmatter.contains_key(serde_yaml::Value::from("priority")),
            "an unanswered optional priority prompt emits no key at all"
        );
    }

    #[test]
    fn field_picker_prompts_name_the_registry_field_they_suggest_from() {
        let engine = engine();
        let templates = engine.list_templates().unwrap();
        let meeting = templates
            .iter()
            .find(|template| template.spec.name == "external-meeting")
            .unwrap();

        let customer = meeting
            .spec
            .prompts
            .iter()
            .find(|prompt| prompt.name == "customer")
            .unwrap();

        // The prompt is singular but the field is the plural list — suggestions
        // must follow the field, not the prompt name.
        assert_eq!(customer.prompt_type, "field-picker");
        assert_eq!(customer.suggestion_field(), "customers");

        // A prompt that omits `field` suggests from its own name.
        let title = meeting
            .spec
            .prompts
            .iter()
            .find(|prompt| prompt.name == "title")
            .unwrap();
        assert_eq!(title.suggestion_field(), "title");
    }

    #[test]
    fn render_internal_meeting_prompts_for_title_only() {
        let engine = engine();
        let meta = engine
            .list_templates()
            .unwrap()
            .into_iter()
            .find(|template| template.spec.name == "internal-meeting")
            .unwrap();
        let prompt_names: Vec<&str> = meta
            .spec
            .prompts
            .iter()
            .map(|prompt| prompt.name.as_str())
            .collect();
        assert_eq!(prompt_names, vec!["title"]);

        let prompts = HashMap::from([("title".to_string(), "Weekly Sync".to_string())]);
        let rendered = engine.render("internal-meeting", &prompts).unwrap();

        let frontmatter = rendered_frontmatter(&rendered.content);
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("audience")),
            Some(&serde_yaml::Value::from("internal"))
        );
        for key in ["customers", "streams", "attendees"] {
            assert!(
                sequence(&frontmatter, key).is_empty(),
                "`{key}` is filled in during enrichment, not capture"
            );
        }
    }

    #[test]
    fn render_person_omits_unanswered_optional_fields() {
        let engine = engine();
        let prompts = HashMap::from([
            ("name".to_string(), "Jane Doe".to_string()),
            ("org".to_string(), "Acme Corp".to_string()),
        ]);
        let rendered = engine.render("person", &prompts).unwrap();

        assert_eq!(rendered.path, "Inbox/Jane Doe.md");
        let frontmatter = rendered_frontmatter(&rendered.content);
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("kind")),
            Some(&serde_yaml::Value::from("person"))
        );
        assert_eq!(
            frontmatter.get(serde_yaml::Value::from("org")),
            Some(&serde_yaml::Value::from("Acme Corp"))
        );
        assert!(!frontmatter.contains_key(serde_yaml::Value::from("role")));
    }

    #[test]
    fn render_customer_writes_straight_to_its_folder_note() {
        let engine = engine();
        let prompts = HashMap::from([("name".to_string(), "Acme Corp".to_string())]);
        let rendered = engine.render("customer", &prompts).unwrap();

        assert_eq!(rendered.path, "Customers/Acme Corp/Acme Corp.md");
        assert!(rendered.content.contains("kind: customer"));
        // The embedded query recipes resolve the customer name at render time.
        assert!(rendered.content.contains("c.value = '[[Acme Corp]]'"));
    }

    #[test]
    fn render_template_merges_static_sql_and_hook_context_layers() {
        let vault = TestVault::new("context-layers");
        vault.create_template(
            ".notesmith/templates/daily.md",
            r#"---
name: daily
output_path: "Daily/{{ date }}-{{ hook_slug }}.md"
prompts:
  - { name: title, type: text, required: true }
context_queries:
  open_tasks: "SELECT text, note_path FROM v_tasks WHERE status_group = 'open' ORDER BY text"
pre_render_hook: ".notesmith/scripts/enrich.sh"
---
---
vault: {{ vault }}
filename: {{ filename }}
hook: {{ hook_slug }}
task_count: {{ task_count }}
---
# {{ title }}
{% for task in open_tasks %}- {{ task.text }} ({{ task.note_path }})
{% endfor %}
"#,
        );
        vault.write_file(
            ".notesmith/scripts/enrich.sh",
            "#!/bin/sh\npython3 -c 'import json,sys; ctx=json.load(sys.stdin); print(json.dumps({\"hook_slug\": \"from-hook\", \"task_count\": len(ctx.get(\"open_tasks\", []))}))'\n",
        );

        let db_path = vault.db_path("cache.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE v_tasks (text TEXT, note_path TEXT, status_group TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO v_tasks (text, note_path, status_group) VALUES ('Alpha', 'Tasks/Alpha.md', 'open'), ('Beta', 'Tasks/Beta.md', 'done')",
            [],
        )
        .unwrap();
        drop(conn);

        let engine = TemplateEngine::new(vault.root.clone(), Some(db_path));
        let prompts = HashMap::from([("title".to_string(), "Daily Note".to_string())]);
        let rendered = engine.render("daily", &prompts).unwrap();
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let vault_name = vault
            .root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap()
            .to_string();

        assert_eq!(rendered.path, format!("Daily/{date}-from-hook.md"));
        assert!(rendered.content.contains(&format!("vault: {vault_name}")));
        assert!(
            rendered
                .content
                .contains(&format!("filename: {date}-from-hook.md"))
        );
        assert!(rendered.content.contains("hook: from-hook"));
        assert!(rendered.content.contains("task_count: 1"));
        assert!(rendered.content.contains("# Daily Note"));
        assert!(rendered.content.contains("- Alpha (Tasks/Alpha.md)"));
        assert!(!rendered.content.contains("Beta"));
    }

    #[test]
    fn pre_render_hook_timeout_does_not_block_rendering() {
        let vault = TestVault::new("hook-timeout");
        vault.create_template(
            ".notesmith/templates/slow.md",
            r#"---
name: slow
output_path: "Inbox/{{ date }}.md"
pre_render_hook: ".notesmith/scripts/slow.sh"
---
# {{ date }}
"#,
        );
        vault.write_file(
            ".notesmith/scripts/slow.sh",
            "#!/bin/sh\nsleep 1\nprintf '{\"late\":true}'\n",
        );

        let engine = TemplateEngine::new(vault.root.clone(), None);
        let start = Instant::now();
        let rendered = engine
            .render_with_output_path("slow", &HashMap::new(), None, Duration::from_millis(50))
            .unwrap();

        assert!(start.elapsed() < Duration::from_millis(500));
        assert!(rendered.content.contains("# "));
        assert!(!rendered.content.contains("late"));
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
