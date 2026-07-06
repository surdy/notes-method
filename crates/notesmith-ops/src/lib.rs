//! notesmith-ops: the canonical vault operations layer.
//!
//! [`Ops`] defines every vault operation an agent surface needs (reads and
//! writes). [`LocalOps`] is the in-process implementation backed by the
//! engine, cache, search index and template engine. [`ReadOnlyOps`] wraps any
//! [`Ops`] and rejects every mutating operation, so a read-only agent surface
//! can be exposed without authentication.
//!
//! See `docs/adr/0010-agent-access-architecture.md`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use chrono::{Local, NaiveDate};
use notesmith_config::VaultConfig;
use notesmith_core::{Note, NotesmithError, VaultEngine, VaultName, VaultPath, WriteResult};
use notesmith_index::{SearchIndex, VaultCache};
use notesmith_query::execute_sql;
use notesmith_routing::RoutingEngine;
use notesmith_tasks::toggle_task;
use notesmith_templates::TemplateEngine;
use notesmith_vault::{NativeVaultEngine, apply_save_pipeline, parse_note};
use rusqlite::{Connection, params};
use serde_json::{Map, Value, json};
use serde_yaml::{Mapping, Value as YamlValue};

pub mod hybrid;
pub use hybrid::{DEFAULT_RRF_K, HybridHit, HybridSearch, rrf_fuse};

/// Result alias for vault operations.
pub type Result<T> = anyhow::Result<T>;

/// The canonical vault operation surface.
///
/// Read operations never mutate the vault; write operations create, update,
/// move or delete note content. [`ReadOnlyOps`] exploits this split to expose
/// a surface where the write operations are unavailable.
pub trait Ops: Send + Sync {
    // --- reads ---

    /// Read a single note's raw content and parsed frontmatter.
    fn get_note(&self, path: &str) -> Result<Value>;
    /// Full-text search across the vault.
    fn search_notes(&self, query: &str, limit: Option<usize>) -> Result<Value>;
    /// Hybrid lexical + semantic search: fuses Tantivy lexical ranking with
    /// vector similarity via RRF, returning path + snippet hits for grounding.
    /// Degrades to lexical-only until embeddings are available.
    fn vault_search(&self, query: &str, limit: Option<usize>) -> Result<Value>;
    /// Run a read-only SQL query against the vault cache.
    fn query_sql(&self, sql: &str) -> Result<Value>;
    /// List notes, optionally filtered by type/customer/archived.
    fn list_notes(
        &self,
        note_type: Option<&str>,
        customer: Option<&str>,
        archived: Option<bool>,
    ) -> Result<Value>;
    /// List tasks, optionally filtered by status/customer.
    fn list_tasks(&self, status: Option<&str>, customer: Option<&str>) -> Result<Value>;
    /// Resolve an MCP-style resource URI to its text content.
    fn read_resource(&self, uri: &str) -> Result<String>;

    // --- writes ---

    /// Create a new note; errors if the target already exists.
    fn create_note(
        &self,
        title: &str,
        content: Option<&str>,
        folder: Option<&str>,
        frontmatter: Option<&Map<String, Value>>,
    ) -> Result<Value>;
    /// Replace a note's content.
    fn update_note(&self, path: &str, content: &str) -> Result<Value>;
    /// Append content to an existing note.
    fn append_to_note(&self, path: &str, content: &str) -> Result<Value>;
    /// Route/archive a note according to the vault routing rules.
    fn archive_note(&self, path: &str) -> Result<Value>;
    /// Toggle a task's status within a note.
    fn update_task_status(&self, note_path: &str, task_hash: &str, status: &str) -> Result<Value>;
    /// Capture a quick note into the capture folder.
    fn inbox_add(&self, content: &str, title: Option<&str>) -> Result<Value>;
    /// Create today's (or a given date's) daily note if missing.
    fn create_daily_note(&self, date: Option<&str>) -> Result<Value>;
    /// Instantiate a template into a new note.
    fn create_from_template(
        &self,
        template_name: &str,
        prompts: Option<HashMap<String, String>>,
    ) -> Result<Value>;
}

/// Error returned when a write operation is attempted on a read-only surface.
fn read_only_error(op: &str) -> anyhow::Error {
    anyhow::anyhow!("operation '{op}' is not permitted on a read-only surface")
}

/// In-process implementation of [`Ops`], backed by the vault engine, cache,
/// search index and template engine.
pub struct LocalOps {
    vault_name: String,
    vault_root: PathBuf,
    engine: NativeVaultEngine,
    cache: Arc<VaultCache>,
    search_index: Arc<SearchIndex>,
    template_engine: Arc<TemplateEngine>,
    vault_config: VaultConfig,
    /// Lazily-built hybrid (lexical+semantic) searcher. Memoised once the
    /// vault's `embeddings.db` exists and opens cleanly; until then each
    /// `vault_search` degrades to lexical-only.
    hybrid: std::sync::OnceLock<Arc<HybridSearch>>,
}

impl LocalOps {
    /// Construct from owned cache/search index (builds a default template
    /// engine rooted at the vault).
    pub fn new(
        vault_name: String,
        vault_root: PathBuf,
        cache: VaultCache,
        search_index: SearchIndex,
        vault_config: VaultConfig,
    ) -> Self {
        let template_engine = Arc::new(TemplateEngine::new(vault_root.clone(), None));
        Self {
            vault_name,
            vault_root,
            engine: NativeVaultEngine,
            cache: Arc::new(cache),
            search_index: Arc::new(search_index),
            template_engine,
            vault_config,
            hybrid: std::sync::OnceLock::new(),
        }
    }

    /// Construct from shared (`Arc`) cache/search index/template engine, so the
    /// daemon can back this with its live per-vault state.
    pub fn from_shared(
        vault_name: String,
        vault_root: PathBuf,
        cache: Arc<VaultCache>,
        search_index: Arc<SearchIndex>,
        template_engine: Arc<TemplateEngine>,
        vault_config: VaultConfig,
    ) -> Self {
        Self {
            vault_name,
            vault_root,
            engine: NativeVaultEngine,
            cache,
            search_index,
            template_engine,
            vault_config,
            hybrid: std::sync::OnceLock::new(),
        }
    }

    /// Return the memoised hybrid searcher, building it on first use once the
    /// vault's `embeddings.db` exists. Returns `None` (⇒ lexical-only) when
    /// embeddings are disabled for this vault, not yet available, or the
    /// embedder/model disagrees with the stored one — never an error, so search
    /// always works.
    fn hybrid_search(&self) -> Option<&Arc<HybridSearch>> {
        // Per-vault gate (ADR 0018 §9.1): a disabled vault is lexical-only even
        // if a stale `embeddings.db` exists on disk or a hybrid searcher was
        // previously memoised. Checked before the memo so `enabled = false`
        // always short-circuits.
        if !self.vault_config.embed.enabled {
            return None;
        }
        if let Some(h) = self.hybrid.get() {
            return Some(h);
        }
        let db_path = notesmith_embed::embeddings_db_path(&self.vault_name).ok()?;
        if !db_path.exists() {
            return None; // worker hasn't produced embeddings yet
        }
        let cache_path = self.cache.cache_path();
        if cache_path.as_os_str() == ":memory:" {
            return None; // no on-disk index to ATTACH for metadata filters
        }
        let embedder = match notesmith_embed::default_embedder() {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(vault = %self.vault_name, error = %e, "embedder init failed; lexical-only search");
                return None;
            }
        };
        let embedding = match notesmith_embed::EmbeddingSearch::open(
            self.vault_name.clone(),
            &db_path,
            cache_path,
            embedder,
        ) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                tracing::warn!(vault = %self.vault_name, error = %e, "opening embedding search failed; lexical-only search");
                return None;
            }
        };
        let hybrid = Arc::new(HybridSearch::new(
            self.search_index.clone(),
            embedding,
            self.vault_root.clone(),
        ));
        // Memoise; if another thread won the race, keep theirs.
        let _ = self.hybrid.set(hybrid);
        self.hybrid.get()
    }

    fn refresh_indexes(&self, path: &VaultPath) -> Result<()> {
        let note = self.load_note(path)?;
        self.cache.update_note_with_periodic(
            &self.vault_name,
            &note,
            &self.vault_config.periodic,
        )?;
        self.search_index.update_note(&self.vault_name, &note)?;
        Ok(())
    }

    fn remove_from_indexes(&self, path: &str) -> Result<()> {
        self.cache.remove_note(&self.vault_name, path)?;
        self.search_index.remove_note(&self.vault_name, path)?;
        Ok(())
    }

    fn load_note(&self, path: &VaultPath) -> Result<Note> {
        let content = self.engine.read(&self.vault_root, path)?;
        Ok(parse_note(
            &VaultName::new(self.vault_name.clone()),
            path,
            &content,
        ))
    }

    fn ensure_note_missing(&self, path: &VaultPath) -> Result<()> {
        match self.engine.read(&self.vault_root, path) {
            Ok(_) => anyhow::bail!("note already exists: {}", path.as_str()),
            Err(NotesmithError::NoteNotFound { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_content(
        &self,
        path: &VaultPath,
        expected_hash: Option<&str>,
        content: &str,
    ) -> Result<String> {
        match self
            .engine
            .write(&self.vault_root, path, expected_hash, content)?
        {
            WriteResult::Written { hash } => Ok(hash),
            WriteResult::Conflict { expected, actual } => anyhow::bail!(
                "write conflict for {} (expected {}, actual {})",
                path.as_str(),
                expected,
                actual
            ),
        }
    }
}

impl Ops for LocalOps {
    fn get_note(&self, path: &str) -> Result<Value> {
        let note_path = VaultPath::new(path.to_string());
        let content = self.engine.read(&self.vault_root, &note_path)?;
        let parsed = parse_note(
            &VaultName::new(self.vault_name.clone()),
            &note_path,
            &content,
        );

        Ok(json!({
            "path": note_path.as_str(),
            "content": content,
            "frontmatter": parsed.frontmatter,
        }))
    }

    fn search_notes(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        let results = self.search_index.search(query, limit.unwrap_or(20))?;
        Ok(serde_json::to_value(results)?)
    }

    fn vault_search(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        let limit = limit.unwrap_or(20);
        if let Some(hybrid) = self.hybrid_search() {
            let hits = hybrid.search(query, limit)?;
            return Ok(serde_json::to_value(hits)?);
        }
        // Lexical-only fallback: shape lexical results like hybrid hits so the
        // tool's response schema is stable whether or not embeddings exist.
        let lexical = self.search_index.search(query, limit)?;
        let hits: Vec<HybridHit> = lexical
            .into_iter()
            .enumerate()
            .map(|(idx, r)| HybridHit {
                path: r.path,
                title: r.title,
                snippet: r.snippet,
                score: 1.0 / (DEFAULT_RRF_K as f32 + (idx + 1) as f32),
                lexical_rank: Some(idx + 1),
                semantic_rank: None,
                char_start: None,
                char_end: None,
            })
            .collect();
        Ok(serde_json::to_value(hits)?)
    }

    fn query_sql(&self, sql: &str) -> Result<Value> {
        Ok(serde_json::to_value(execute_sql(&self.cache, sql)?)?)
    }

    fn list_notes(
        &self,
        note_type: Option<&str>,
        customer: Option<&str>,
        archived: Option<bool>,
    ) -> Result<Value> {
        let conn = self.cache.connection();
        let mut stmt = conn.prepare(
            "SELECT path, title, created_at, updated_at, mtime_unix FROM notes ORDER BY path",
        )?;
        let base_rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut rows = Vec::new();
        for (path, title, created_at, updated_at, mtime_unix) in base_rows {
            let frontmatter = load_note_frontmatter(&conn, &self.vault_name, &path)?;
            let resolved_type =
                frontmatter_string(&frontmatter, "type").unwrap_or_else(|| "note".to_string());
            let resolved_customer = frontmatter_string(&frontmatter, "customer");
            let resolved_archived = frontmatter_bool(&frontmatter, "archived");
            if note_type.is_some_and(|expected| expected != resolved_type) {
                continue;
            }
            if customer.is_some_and(|expected| resolved_customer.as_deref() != Some(expected)) {
                continue;
            }
            if archived.is_some_and(|expected| expected != resolved_archived) {
                continue;
            }

            rows.push(json!({
                "path": path,
                "title": title,
                "type": resolved_type,
                "customer": resolved_customer,
                "stream": frontmatter_string(&frontmatter, "stream"),
                "state": frontmatter_string(&frontmatter, "state"),
                "status": frontmatter_string(&frontmatter, "status"),
                "date": frontmatter_string(&frontmatter, "date"),
                "created_at": created_at,
                "updated_at": updated_at,
                "archived": resolved_archived,
                "mtime_unix": mtime_unix,
                "frontmatter": frontmatter,
            }));
        }

        Ok(Value::Array(rows))
    }

    fn list_tasks(&self, status: Option<&str>, customer: Option<&str>) -> Result<Value> {
        let mut conditions = vec!["1=1".to_string()];
        if let Some(status) = status {
            let status_char = parse_status_str(status).map_err(anyhow::Error::msg)?;
            conditions.push(format!(
                "t.status_char = '{}'",
                escape_sql_string(&status_char.to_string())
            ));
        }
        if let Some(customer) = customer {
            conditions.push(format!(
                "customer.value = '{}'",
                escape_sql_string(customer)
            ));
        }

        let sql = format!(
            "SELECT t.content_hash, t.note_path, t.line_number, t.status_char, t.status_group, t.text, n.title, customer.value, stream.value, owner.value, due.value, priority.value \
             FROM tasks t \
             JOIN notes n ON n.vault_name = t.vault_name AND n.path = t.note_path \
             LEFT JOIN task_fields customer ON customer.vault_name = t.vault_name AND customer.task_id = t.id AND customer.key = 'customer' \
             LEFT JOIN task_fields stream ON stream.vault_name = t.vault_name AND stream.task_id = t.id AND stream.key = 'stream' \
             LEFT JOIN task_fields owner ON owner.vault_name = t.vault_name AND owner.task_id = t.id AND owner.key = 'owner' \
             LEFT JOIN task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due' \
             LEFT JOIN task_fields priority ON priority.vault_name = t.vault_name AND priority.task_id = t.id AND priority.key = 'priority' \
             WHERE {} ORDER BY due.value IS NULL, due.value ASC, t.line_number ASC",
            conditions.join(" AND ")
        );

        let conn = self.cache.connection();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map([], |row| {
                let status_char: String = row.get(3)?;
                Ok(json!({
                    "task_hash": row.get::<_, Option<String>>(0)?,
                    "note_path": row.get::<_, String>(1)?,
                    "line_number": row.get::<_, i64>(2)?,
                    "status": status_name_for_char(&status_char),
                    "status_char": status_char,
                    "status_group": row.get::<_, String>(4)?,
                    "text": row.get::<_, String>(5)?,
                    "note_title": row.get::<_, Option<String>>(6)?,
                    "customer": row.get::<_, Option<String>>(7)?,
                    "stream": row.get::<_, Option<String>>(8)?,
                    "owner": row.get::<_, Option<String>>(9)?,
                    "due": row.get::<_, Option<String>>(10)?,
                    "priority": row.get::<_, Option<String>>(11)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Value::Array(rows))
    }

    fn read_resource(&self, uri: &str) -> Result<String> {
        if uri == "note:///vault/structure" {
            let notes = self.engine.scan(&self.vault_root)?;
            let paths = notes
                .into_iter()
                .map(|note| note.path.to_string())
                .collect::<Vec<_>>();
            return Ok(serde_json::to_string_pretty(&paths)?);
        }

        if let Some(date) = uri.strip_prefix("note:///daily/") {
            let path = format!("{}/{}.md", self.vault_config.daily.folder, date);
            return Ok(self.engine.read(&self.vault_root, &VaultPath::new(path))?);
        }

        if let Some(path) = uri.strip_prefix("note:///") {
            return Ok(self
                .engine
                .read(&self.vault_root, &VaultPath::new(path.to_string()))?);
        }

        anyhow::bail!("unknown resource: {uri}")
    }

    fn create_note(
        &self,
        title: &str,
        content: Option<&str>,
        folder: Option<&str>,
        frontmatter: Option<&Map<String, Value>>,
    ) -> Result<Value> {
        let folder = folder.unwrap_or("Inbox");
        let note_path = VaultPath::new(format!("{folder}/{title}.md"));
        self.ensure_note_missing(&note_path)?;

        let initial_content = build_note_document(frontmatter, content)?;
        let content = apply_save_pipeline(&initial_content);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    fn update_note(&self, path: &str, content: &str) -> Result<Value> {
        let note_path = VaultPath::new(path.to_string());
        let content = apply_save_pipeline(content);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    fn append_to_note(&self, path: &str, content: &str) -> Result<Value> {
        let note_path = VaultPath::new(path.to_string());
        let current_content = self.engine.read(&self.vault_root, &note_path)?;
        let separator = if current_content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let appended = format!("{current_content}{separator}{content}");
        let content = apply_save_pipeline(&appended);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    fn archive_note(&self, path: &str) -> Result<Value> {
        let routing = RoutingEngine::load(&self.vault_root)?;
        let result = routing.apply(&self.vault_root, path, &self.engine)?;
        self.remove_from_indexes(path)?;
        self.refresh_indexes(&VaultPath::new(result.to.clone()))?;
        Ok(serde_json::to_value(result)?)
    }

    fn update_task_status(&self, note_path: &str, task_hash: &str, status: &str) -> Result<Value> {
        let new_status = parse_status_str(status).map_err(anyhow::Error::msg)?;
        let note_path = VaultPath::new(note_path.to_string());
        let current_content = self.engine.read(&self.vault_root, &note_path)?;
        let updated = toggle_task(&current_content, task_hash, new_status)?;
        let content = apply_save_pipeline(&updated);
        let hash = self.write_content(&note_path, None, &content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
            "status": status,
        }))
    }

    fn inbox_add(&self, content: &str, title: Option<&str>) -> Result<Value> {
        let capture_folder = &self.vault_config.capture.folder;
        let timestamp = Local::now().format("%Y-%m-%d %H-%M-%S").to_string();
        let slug = match title {
            Some(title) => sanitize_slug(title),
            None => sanitize_slug(&content.chars().take(40).collect::<String>()),
        };
        let filename = if slug.is_empty() {
            format!("{timestamp}.md")
        } else {
            format!("{timestamp} - {slug}.md")
        };
        let note_path = if capture_folder.is_empty() {
            VaultPath::new(filename)
        } else {
            VaultPath::new(format!("{capture_folder}/{filename}"))
        };
        let hash = self.write_content(&note_path, None, content)?;
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": note_path.as_str(),
            "hash": hash,
        }))
    }

    fn create_daily_note(&self, date: Option<&str>) -> Result<Value> {
        let parsed_date = match date {
            Some(date) => NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .with_context(|| format!("invalid date: {date}"))?,
            None => Local::now().date_naive(),
        };
        let date_str = parsed_date.format("%Y-%m-%d").to_string();
        let note_path = VaultPath::new(format!(
            "{}/{}.md",
            self.vault_config.daily.folder, date_str
        ));

        match self.engine.read(&self.vault_root, &note_path) {
            Ok(_) => {
                return Ok(json!({
                    "path": note_path.as_str(),
                    "created": false,
                }));
            }
            Err(NotesmithError::NoteNotFound { .. }) => {}
            Err(error) => return Err(error.into()),
        }

        let mut prompts = HashMap::new();
        prompts.insert("today".to_string(), date_str);
        let rendered = self.template_engine.instantiate(
            &self.vault_config.daily.template,
            &prompts,
            &self.engine,
        )?;
        let rendered_path = VaultPath::new(rendered.path.clone());
        self.refresh_indexes(&rendered_path)?;

        Ok(json!({
            "path": rendered.path,
            "created": true,
        }))
    }

    fn create_from_template(
        &self,
        template_name: &str,
        prompts: Option<HashMap<String, String>>,
    ) -> Result<Value> {
        let rendered = self.template_engine.instantiate(
            template_name,
            &prompts.unwrap_or_default(),
            &self.engine,
        )?;
        let note_path = VaultPath::new(rendered.path.clone());
        self.refresh_indexes(&note_path)?;

        Ok(json!({
            "path": rendered.path,
            "content": rendered.content,
        }))
    }
}

/// Decorator over any [`Ops`] that permits reads and rejects every write.
///
/// Backs read-only agent surfaces (e.g. the daemon's `/mcp-ro/<vault>`
/// endpoint). Because the write operations are unavailable rather than guarded
/// by identity, this works without authentication — it guards against agent
/// mistakes, not against a malicious caller who can reach the full surface.
pub struct ReadOnlyOps<O: Ops> {
    inner: O,
}

impl<O: Ops> ReadOnlyOps<O> {
    /// Wrap an inner [`Ops`] in a read-only surface.
    pub fn new(inner: O) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped [`Ops`].
    pub fn inner(&self) -> &O {
        &self.inner
    }

    /// Unwrap back to the inner [`Ops`].
    pub fn into_inner(self) -> O {
        self.inner
    }
}

impl<O: Ops> Ops for ReadOnlyOps<O> {
    fn get_note(&self, path: &str) -> Result<Value> {
        self.inner.get_note(path)
    }

    fn search_notes(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        self.inner.search_notes(query, limit)
    }

    fn vault_search(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        self.inner.vault_search(query, limit)
    }

    fn query_sql(&self, sql: &str) -> Result<Value> {
        self.inner.query_sql(sql)
    }

    fn list_notes(
        &self,
        note_type: Option<&str>,
        customer: Option<&str>,
        archived: Option<bool>,
    ) -> Result<Value> {
        self.inner.list_notes(note_type, customer, archived)
    }

    fn list_tasks(&self, status: Option<&str>, customer: Option<&str>) -> Result<Value> {
        self.inner.list_tasks(status, customer)
    }

    fn read_resource(&self, uri: &str) -> Result<String> {
        self.inner.read_resource(uri)
    }

    fn create_note(
        &self,
        _title: &str,
        _content: Option<&str>,
        _folder: Option<&str>,
        _frontmatter: Option<&Map<String, Value>>,
    ) -> Result<Value> {
        Err(read_only_error("create_note"))
    }

    fn update_note(&self, _path: &str, _content: &str) -> Result<Value> {
        Err(read_only_error("update_note"))
    }

    fn append_to_note(&self, _path: &str, _content: &str) -> Result<Value> {
        Err(read_only_error("append_to_note"))
    }

    fn archive_note(&self, _path: &str) -> Result<Value> {
        Err(read_only_error("archive_note"))
    }

    fn update_task_status(
        &self,
        _note_path: &str,
        _task_hash: &str,
        _status: &str,
    ) -> Result<Value> {
        Err(read_only_error("update_task_status"))
    }

    fn inbox_add(&self, _content: &str, _title: Option<&str>) -> Result<Value> {
        Err(read_only_error("inbox_add"))
    }

    fn create_daily_note(&self, _date: Option<&str>) -> Result<Value> {
        Err(read_only_error("create_daily_note"))
    }

    fn create_from_template(
        &self,
        _template_name: &str,
        _prompts: Option<HashMap<String, String>>,
    ) -> Result<Value> {
        Err(read_only_error("create_from_template"))
    }
}

fn build_note_document(
    frontmatter: Option<&Map<String, Value>>,
    body: Option<&str>,
) -> Result<String> {
    let frontmatter = match frontmatter {
        Some(frontmatter) => json_frontmatter_to_mapping(frontmatter)?,
        None => Mapping::new(),
    };
    build_note_document_from_yaml(&frontmatter, body.unwrap_or_default())
}

fn build_note_document_from_yaml(frontmatter: &Mapping, body: &str) -> Result<String> {
    let yaml = serialize_yaml_mapping(frontmatter)?;
    Ok(if yaml.is_empty() {
        format!("---\n---\n{body}")
    } else {
        format!("---\n{yaml}\n---\n{body}")
    })
}

fn serialize_yaml_mapping(frontmatter: &Mapping) -> Result<String> {
    let serialized = serde_yaml::to_string(&YamlValue::Mapping(frontmatter.clone()))?;
    Ok(serialized
        .strip_prefix("---\n")
        .unwrap_or(&serialized)
        .trim_end_matches('\n')
        .to_string())
}

fn json_frontmatter_to_mapping(frontmatter: &Map<String, Value>) -> Result<Mapping> {
    if frontmatter.is_empty() {
        return Ok(Mapping::new());
    }

    let yaml_value = serde_yaml::to_value(Value::Object(frontmatter.clone()))?;
    match yaml_value {
        YamlValue::Mapping(mapping) => Ok(mapping),
        other => anyhow::bail!("expected frontmatter object, got {other:?}"),
    }
}

fn parse_status_str(s: &str) -> std::result::Result<char, String> {
    match s {
        "todo" => Ok(' '),
        "in_progress" => Ok('/'),
        "blocked" => Ok('b'),
        "waiting" => Ok('w'),
        "on_hold" => Ok('h'),
        "done" => Ok('x'),
        "cancelled" => Ok('-'),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Ok(ch),
                _ => Err(format!(
                    "unknown status '{other}'; expected one of: todo, in_progress, blocked, waiting, on_hold, done, cancelled"
                )),
            }
        }
    }
}

fn load_note_frontmatter(conn: &Connection, vault_name: &str, path: &str) -> Result<Value> {
    let mut fields_stmt = conn.prepare(
        "SELECT key, value, value_type FROM fields WHERE vault_name = ?1 AND note_path = ?2 ORDER BY key",
    )?;
    let mut field_rows = fields_stmt.query(params![vault_name, path])?;
    let mut frontmatter = Map::new();
    while let Some(row) = field_rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        let value_type: String = row.get(2)?;
        frontmatter.insert(key, parse_field_json_value(&value, &value_type));
    }
    drop(field_rows);
    drop(fields_stmt);

    let mut tags_stmt =
        conn.prepare("SELECT tag FROM tags WHERE vault_name = ?1 AND note_path = ?2 ORDER BY tag")?;
    let mut tag_rows = tags_stmt.query(params![vault_name, path])?;
    let mut tags = Vec::new();
    while let Some(row) = tag_rows.next()? {
        tags.push(row.get::<_, String>(0)?);
    }
    if !tags.is_empty() {
        frontmatter.insert(
            "tags".to_string(),
            Value::Array(tags.into_iter().map(Value::String).collect()),
        );
    }

    Ok(Value::Object(frontmatter))
}

fn parse_field_json_value(value: &str, value_type: &str) -> Value {
    match value_type {
        "boolean" => Value::Bool(value == "true"),
        "number" => value
            .parse::<i64>()
            .map(|number| Value::Number(number.into()))
            .or_else(|_| {
                value
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or(())
            })
            .unwrap_or_else(|_| Value::String(value.to_string())),
        "list" => serde_yaml::from_str::<Value>(value)
            .unwrap_or_else(|_| Value::String(value.to_string())),
        _ => Value::String(value.to_string()),
    }
}

fn frontmatter_string(frontmatter: &Value, key: &str) -> Option<String> {
    frontmatter.get(key).and_then(|value| match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn frontmatter_bool(frontmatter: &Value, key: &str) -> bool {
    match frontmatter.get(key) {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => text == "true",
        _ => false,
    }
}

fn status_name_for_char(status_char: &str) -> String {
    match status_char.chars().next().unwrap_or(' ') {
        ' ' => "todo",
        '/' => "in_progress",
        'b' => "blocked",
        'w' => "waiting",
        'h' => "on_hold",
        'x' | 'X' => "done",
        '-' => "cancelled",
        other => return other.to_string(),
    }
    .to_string()
}

fn sanitize_slug(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == ' ' || ch == '-' {
                ch
            } else {
                ' '
            }
        })
        .collect();
    sanitized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape_sql_string(input: &str) -> String {
    input.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn vault_config() -> VaultConfig {
        VaultConfig {
            name: "test-vault".to_string(),
            capture: notesmith_config::CaptureConfig {
                folder: "Inbox".to_string(),
                template: "generic-note".to_string(),
            },
            ..Default::default()
        }
    }

    fn build_test_ops(root: &Path) -> LocalOps {
        let engine = NativeVaultEngine;
        let notes = engine.scan(root).unwrap();
        let cache = VaultCache::open_in_memory().unwrap();
        cache
            .reindex_with_periodic("test-vault", &notes, &vault_config().periodic)
            .unwrap();
        let search_index = SearchIndex::open_in_memory().unwrap();
        search_index.reindex("test-vault", &notes).unwrap();
        LocalOps::new(
            "test-vault".to_string(),
            root.to_path_buf(),
            cache,
            search_index,
            vault_config(),
        )
    }

    fn write_note(root: &Path, path: &str, content: &str) {
        let engine = NativeVaultEngine;
        let note_path = VaultPath::new(path.to_string());
        let content = apply_save_pipeline(content);
        engine.write(root, &note_path, None, &content).unwrap();
    }

    #[test]
    fn create_and_get_note() {
        let temp_dir = TempDir::new().unwrap();
        let ops = build_test_ops(temp_dir.path());

        let created = ops
            .create_note("Hello", Some("# Hello"), Some("Inbox"), None)
            .unwrap();
        assert_eq!(created["path"], "Inbox/Hello.md");

        let fetched = ops.get_note("Inbox/Hello.md").unwrap();
        assert_eq!(fetched["path"], "Inbox/Hello.md");
        assert!(fetched["content"].as_str().unwrap().contains("# Hello"));
        assert!(fetched["content"].as_str().unwrap().contains("created:"));
    }

    #[test]
    fn search_returns_matching_note() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Inbox/Launch Plan.md",
            "---\ntype: note\n---\nDiscuss launch timeline",
        );
        write_note(
            temp_dir.path(),
            "Inbox/Other.md",
            "---\ntype: note\n---\nUnrelated",
        );
        let ops = build_test_ops(temp_dir.path());

        let results = ops.search_notes("launch", Some(10)).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Inbox/Launch Plan.md");
    }

    #[test]
    fn query_sql_returns_rows() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Inbox/Query Me.md",
            "---\ntype: note\n---\nBody",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops
            .query_sql("SELECT path, title FROM v_notes ORDER BY path")
            .unwrap();
        assert_eq!(result["columns"], json!(["path", "title"]));
        assert_eq!(result["row_count"], 1);
    }

    #[test]
    fn list_notes_filters_by_type() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Customers/Acme.md",
            "---\ntype: customer\ncustomer: Acme\nstate: Active\n---\n# Acme",
        );
        write_note(
            temp_dir.path(),
            "Inbox/Scratch.md",
            "---\ntype: note\n---\n# Scratch",
        );
        let ops = build_test_ops(temp_dir.path());

        let results = ops.list_notes(Some("customer"), None, None).unwrap();
        let results = results.as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "Customers/Acme.md");
    }

    #[test]
    fn inbox_add_writes_capture_note() {
        let temp_dir = TempDir::new().unwrap();
        let ops = build_test_ops(temp_dir.path());

        let created = ops
            .inbox_add("Captured thought", Some("Quick Note"))
            .unwrap();
        let path = created["path"].as_str().unwrap();
        assert!(path.starts_with("Inbox/"));
        assert!(path.ends_with("Quick Note.md"));
        let stored = std::fs::read_to_string(temp_dir.path().join(path)).unwrap();
        assert_eq!(stored, "Captured thought");
    }

    #[test]
    fn read_only_allows_reads() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Inbox/Readable.md",
            "---\ntype: note\n---\nVisible",
        );
        let ops = ReadOnlyOps::new(build_test_ops(temp_dir.path()));

        let fetched = ops.get_note("Inbox/Readable.md").unwrap();
        assert!(fetched["content"].as_str().unwrap().contains("Visible"));
        assert!(ops.list_notes(None, None, None).is_ok());
        assert!(ops.search_notes("Visible", None).is_ok());
    }

    #[test]
    fn read_only_blocks_every_write() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Inbox/Existing.md",
            "---\ntype: note\n---\nBody",
        );
        let ops = ReadOnlyOps::new(build_test_ops(temp_dir.path()));

        assert!(ops.create_note("New", None, None, None).is_err());
        assert!(ops.update_note("Inbox/Existing.md", "changed").is_err());
        assert!(ops.append_to_note("Inbox/Existing.md", "more").is_err());
        assert!(ops.archive_note("Inbox/Existing.md").is_err());
        assert!(
            ops.update_task_status("Inbox/Existing.md", "deadbeef", "done")
                .is_err()
        );
        assert!(ops.inbox_add("nope", None).is_err());
        assert!(ops.create_daily_note(None).is_err());
        assert!(ops.create_from_template("generic-note", None).is_err());

        // The blocked writes must not have touched the filesystem.
        let stored = std::fs::read_to_string(temp_dir.path().join("Inbox/Existing.md")).unwrap();
        assert!(stored.contains("Body"));
        assert!(!stored.contains("changed"));
        assert!(!stored.contains("more"));
    }

    #[test]
    fn read_only_error_names_the_operation() {
        let temp_dir = TempDir::new().unwrap();
        let ops = ReadOnlyOps::new(build_test_ops(temp_dir.path()));
        let err = ops.create_note("X", None, None, None).unwrap_err();
        assert!(err.to_string().contains("create_note"));
        assert!(err.to_string().contains("read-only"));
    }

    #[test]
    fn malformed_frontmatter_does_not_panic() {
        let temp_dir = TempDir::new().unwrap();
        // Broken YAML frontmatter (mapping value where a scalar is expected).
        write_note(
            temp_dir.path(),
            "Inbox/Broken.md",
            "---\nslack: slack: slack://x\n---\nBody text",
        );
        write_note(
            temp_dir.path(),
            "Inbox/Good.md",
            "---\ntype: note\n---\nGood body",
        );
        let ops = build_test_ops(temp_dir.path());

        // Listing tolerates the malformed note and still returns the good one.
        let results = ops.list_notes(None, None, None).unwrap();
        let paths: Vec<&str> = results
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"Inbox/Good.md"));

        // Reading the broken note degrades gracefully rather than panicking.
        let fetched = ops.get_note("Inbox/Broken.md").unwrap();
        assert!(fetched["content"].as_str().unwrap().contains("Body text"));
    }

    /// Build `LocalOps` with an on-disk cache and a populated `embeddings.db`,
    /// so that `hybrid_search()` *could* construct a hybrid searcher. The embed
    /// flag is the only thing gating it. Data dir is overridden to `data_root`
    /// so `embeddings_db_path` resolves under the temp dir.
    fn build_gated_ops(
        root: &Path,
        cache_dir: &Path,
        vault: &str,
        embed_enabled: bool,
    ) -> LocalOps {
        let engine = NativeVaultEngine;
        let notes = engine.scan(root).unwrap();

        // On-disk cache is required for the hybrid path (it ATTACHes the cache
        // for metadata filters; an in-memory cache short-circuits to lexical).
        let cache = VaultCache::open(&cache_dir.join("cache.db")).unwrap();
        cache
            .reindex_with_periodic(vault, &notes, &vault_config().periodic)
            .unwrap();
        let search_index = SearchIndex::open_in_memory().unwrap();
        search_index.reindex(vault, &notes).unwrap();

        let mut config = vault_config();
        config.name = vault.to_string();
        config.embed.enabled = embed_enabled;

        LocalOps::new(
            vault.to_string(),
            root.to_path_buf(),
            cache,
            search_index,
            config,
        )
    }

    /// The per-vault `[embed] enabled` flag gates the query-time hybrid path
    /// (ADR 0018 §9.1): a disabled vault is lexical-only even with a populated
    /// `embeddings.db` on disk; enabling it lets the hybrid searcher be built.
    #[test]
    fn hybrid_search_gated_by_embed_enabled_flag() {
        let vault_name = "ops-embed-gate";
        let data_root = TempDir::new().unwrap();
        // SAFETY: env override for a single, self-contained test path. Other
        // tests use disabled vaults and never resolve embeddings_db_path.
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_root.path());
        }

        let vault = TempDir::new().unwrap();
        write_note(
            vault.path(),
            "Inbox/Semantic.md",
            "---\ntype: note\n---\nSemantic content worth embedding",
        );

        // Populate a real embeddings.db with the same (hash) embedder the
        // query path uses, so a hybrid searcher would open cleanly.
        let db_path = notesmith_embed::embeddings_db_path(vault_name).unwrap();
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        {
            let store = notesmith_embed::EmbeddingStore::open(&db_path).unwrap();
            let embedder = notesmith_embed::HashEmbedder::default();
            let worker = notesmith_embed::EmbedWorker::new(
                vault_name.to_string(),
                vault.path().to_path_buf(),
                &store,
                &embedder,
            );
            worker.run().unwrap();
        }

        let cache_disabled = TempDir::new().unwrap();
        let disabled = build_gated_ops(vault.path(), cache_disabled.path(), vault_name, false);
        assert!(
            disabled.hybrid_search().is_none(),
            "disabled vault must stay lexical-only despite embeddings.db"
        );

        let cache_enabled = TempDir::new().unwrap();
        let enabled = build_gated_ops(vault.path(), cache_enabled.path(), vault_name, true);
        assert!(
            enabled.hybrid_search().is_some(),
            "enabled vault with embeddings.db must use the hybrid searcher"
        );

        // SAFETY: paired with the set_var above.
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }
    }
}
