//! notesmith-ops: the canonical vault operations layer.
//!
//! [`Ops`] defines every vault operation an agent surface needs (reads and
//! writes). [`LocalOps`] is the in-process implementation backed by the
//! engine, cache, search index and template engine. [`ReadOnlyOps`] wraps any
//! [`Ops`] and rejects every mutating operation, so a read-only agent surface
//! can be exposed without authentication.
//!
//! See `docs/adr/0010-agent-access-architecture.md`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use chrono::{Local, NaiveDate, NaiveDateTime};
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
pub mod memory;
pub mod related;
pub mod time_query;
pub use hybrid::{DEFAULT_RRF_K, HybridHit, HybridSearch, rrf_fuse};
pub use memory::{
    DEFAULT_MEMORY_RECALL_LIMIT, FactNoteMeta, MAX_MEMORY_RECALL_LIMIT, MemoryRecallHit,
    MemoryRecallResponse,
};

/// Result alias for vault operations.
pub type Result<T> = anyhow::Result<T>;

/// The canonical vault operation surface.
///
/// Read operations never mutate the vault; write operations create, update,
/// move or delete note content. [`ReadOnlyOps`] exploits this split to expose
/// a surface where the write operations are unavailable.
pub trait Ops: Send + Sync {
    // --- identity ---

    /// The name of the vault this operation surface is bound to. Used to ground
    /// agents/tools in the active vault (e.g. naming the vault in MCP tool
    /// descriptions) so they don't confuse it with another vault's tools.
    fn vault_name(&self) -> &str;

    // --- reads ---

    /// Read a single note's raw content and parsed frontmatter.
    fn get_note(&self, path: &str) -> Result<Value>;
    /// Full-text search across the vault.
    fn search_notes(&self, query: &str, limit: Option<usize>) -> Result<Value>;
    /// Hybrid lexical + semantic search: fuses Tantivy lexical ranking with
    /// vector similarity via RRF, returning path + snippet hits for grounding.
    /// Degrades to lexical-only until embeddings are available.
    fn vault_search(&self, query: &str, limit: Option<usize>) -> Result<Value>;
    /// Recall active fact-memory notes matching a query, optionally scoped to
    /// `user` plus an exact companion scope such as `vault:<name>`.
    fn memory_recall(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value>;
    /// Resolve a natural-language time expression (e.g. "last week", "in May")
    /// into a date range and return note references whose chosen date field
    /// (or, for periodic notes, whose period) falls within it. An optional
    /// `query` further restricts results to notes matching a keyword.
    fn time_query(
        &self,
        when: &str,
        date_field: Option<&str>,
        query: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value>;
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

    /// Rank notes related to `path` by blending embedding similarity with
    /// link-graph proximity (issue #201). Degrades to graph-only ranking when
    /// the vault has no usable embeddings. See [`crate::related`].
    pub fn related_notes(&self, path: &str, limit: usize) -> Result<Value> {
        use std::collections::{HashMap, HashSet};

        let active_full = path.to_string();
        let active_stem = stem_of(&active_full);
        let conn = self.cache.connection();

        // Candidate universe: every note, plus title and stem lookups.
        let mut stmt = conn.prepare("SELECT path, title FROM notes")?;
        let note_rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        if !note_rows.iter().any(|(p, _)| p == &active_full) {
            anyhow::bail!("note not found: {active_full}");
        }
        let mut title_by_path: HashMap<String, String> = HashMap::new();
        for (p, t) in &note_rows {
            title_by_path.insert(p.clone(), t.clone().unwrap_or_else(|| stem_of(p)));
        }

        // Link graph. `target_path` holds the wikilink target (a stem), while
        // `source_path` is a full note path, so targets are compared by stem.
        let mut out_stems: HashMap<String, HashSet<String>> = HashMap::new();
        let mut citers: HashMap<String, Vec<String>> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT source_path, target_path FROM links \
             WHERE vault_name = ?1 AND target_path IS NOT NULL",
        )?;
        let link_rows = stmt
            .query_map([&self.vault_name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        for (source, target) in link_rows {
            let target_stem = stem_of(&target);
            out_stems
                .entry(source.clone())
                .or_default()
                .insert(target_stem.clone());
            citers.entry(target_stem).or_default().push(source);
        }

        let empty_out: HashSet<String> = HashSet::new();
        let active_out = out_stems.get(&active_full).unwrap_or(&empty_out);

        // Bibliographic coupling: candidates that link to a target the active
        // note also links to.
        let mut coupling: HashMap<String, u32> = HashMap::new();
        for target in active_out {
            if let Some(sources) = citers.get(target) {
                for source in sources {
                    if source != &active_full {
                        *coupling.entry(source.clone()).or_default() += 1;
                    }
                }
            }
        }

        // Co-citation: candidates linked to by a note that also links to the
        // active note. Keyed by candidate stem (the linked-to targets).
        let mut cocitation: HashMap<String, u32> = HashMap::new();
        if let Some(active_citers) = citers.get(&active_stem) {
            for source in active_citers {
                if source == &active_full {
                    continue;
                }
                if let Some(source_out) = out_stems.get(source) {
                    for target in source_out {
                        if target != &active_stem {
                            *cocitation.entry(target.clone()).or_default() += 1;
                        }
                    }
                }
            }
        }

        // Candidates directly linking *to* the active note.
        let direct_in: HashSet<&String> = citers
            .get(&active_stem)
            .map(|sources| sources.iter().collect())
            .unwrap_or_default();

        // Embedding centroids (mean chunk vector per note); only meaningful when
        // the active note itself has a stored vector.
        let centroids = self.note_centroids();
        let active_vec = centroids
            .as_ref()
            .and_then(|map| map.get(&active_full).cloned());
        let embeddings_used = active_vec.is_some();

        let mut candidates: Vec<related::CandidateSignals> = Vec::new();
        for (p, _) in &note_rows {
            if p == &active_full {
                continue;
            }
            let candidate_stem = stem_of(p);
            let directly_linked = active_out.contains(&candidate_stem) || direct_in.contains(p);
            let shared_neighbors = coupling.get(p).copied().unwrap_or(0)
                + cocitation.get(&candidate_stem).copied().unwrap_or(0);
            let embedding_similarity = match (&active_vec, &centroids) {
                (Some(av), Some(map)) => map.get(p).map(|cv| related::cosine_similarity(av, cv)),
                _ => None,
            };
            if !directly_linked
                && shared_neighbors == 0
                && embedding_similarity.unwrap_or(0.0) <= 0.0
            {
                continue;
            }
            candidates.push(related::CandidateSignals {
                path: p.clone(),
                title: title_by_path
                    .get(p)
                    .cloned()
                    .unwrap_or_else(|| candidate_stem.clone()),
                embedding_similarity,
                directly_linked,
                shared_neighbors,
            });
        }

        let ranked = related::rank_related(candidates, embeddings_used, limit);
        let related_json: Vec<Value> = ranked
            .iter()
            .map(|r| {
                json!({
                    "path": r.path,
                    "title": r.title,
                    "score": r.score,
                    "embedding_similarity": r.embedding_similarity,
                    "directly_linked": r.directly_linked,
                    "shared_neighbors": r.shared_neighbors,
                })
            })
            .collect();

        Ok(json!({
            "path": active_full,
            "embeddings_used": embeddings_used,
            "related": related_json,
        }))
    }

    /// Load every note's centroid (mean chunk vector) from the vault's
    /// `embeddings.db`, or `None` when embeddings are disabled/absent. Mirrors
    /// the [`Self::hybrid_search`] gate so relatedness and search agree on
    /// whether a vault has usable vectors.
    fn note_centroids(&self) -> Option<std::collections::HashMap<String, Vec<f32>>> {
        if !self.vault_config.embed.enabled {
            return None;
        }
        let db_path = notesmith_embed::embeddings_db_path(&self.vault_name).ok()?;
        if !db_path.exists() {
            return None;
        }
        let store = notesmith_embed::EmbeddingStore::open_read_only(&db_path).ok()?;
        let chunks = store.load_chunks(&self.vault_name).ok()?;
        if chunks.is_empty() {
            return None;
        }
        let mut sums: std::collections::HashMap<String, (Vec<f32>, usize)> =
            std::collections::HashMap::new();
        for chunk in chunks {
            let entry = sums
                .entry(chunk.path)
                .or_insert_with(|| (vec![0.0; chunk.vector.len()], 0));
            if entry.0.len() == chunk.vector.len() {
                for (acc, v) in entry.0.iter_mut().zip(chunk.vector.iter()) {
                    *acc += v;
                }
                entry.1 += 1;
            }
        }
        let centroids = sums
            .into_iter()
            .filter_map(|(path, (mut sum, count))| {
                if count == 0 {
                    return None;
                }
                for value in sum.iter_mut() {
                    *value /= count as f32;
                }
                Some((path, sum))
            })
            .collect();
        Some(centroids)
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

    fn active_fact_candidates(&self, scope: Option<&str>) -> Result<HashMap<String, FactNoteMeta>> {
        let conn = self.cache.connection();
        let mut stmt = conn.prepare(
            "SELECT n.path,
                    COALESCE(n.title, ''),
                    COALESCE(NULLIF(TRIM(d.value), ''), NULLIF(TRIM(n.body_excerpt), ''), ''),
                    NULLIF(TRIM(sc.value), ''),
                    NULLIF(TRIM(cert.value), ''),
                    NULLIF(TRIM(src.value), '')
             FROM notes n
             JOIN fields ty
               ON ty.vault_name = n.vault_name
              AND ty.note_path = n.path
              AND ty.key = 'type'
              AND ty.value = 'fact'
             LEFT JOIN fields st
               ON st.vault_name = n.vault_name
              AND st.note_path = n.path
              AND st.key = 'status'
             LEFT JOIN fields d
               ON d.vault_name = n.vault_name
              AND d.note_path = n.path
              AND d.key = 'description'
             LEFT JOIN fields sc
               ON sc.vault_name = n.vault_name
              AND sc.note_path = n.path
              AND sc.key = 'scope'
             LEFT JOIN fields cert
               ON cert.vault_name = n.vault_name
              AND cert.note_path = n.path
              AND cert.key = 'certainty'
             LEFT JOIN fields src
               ON src.vault_name = n.vault_name
              AND src.note_path = n.path
              AND src.key = 'source'
             WHERE n.vault_name = ?1
               AND COALESCE(st.value, 'active') = 'active'
               AND NOT EXISTS (
                   SELECT 1
                   FROM tags t
                   WHERE t.vault_name = n.vault_name
                     AND t.note_path = n.path
                     AND lower(t.tag) = 'example'
               )
             ORDER BY n.path",
        )?;
        let rows = stmt
            .query_map([&self.vault_name], |row| {
                Ok(FactNoteMeta {
                    path: row.get::<_, String>(0)?,
                    title: row.get::<_, String>(1)?,
                    claim: row.get::<_, String>(2)?,
                    scope: row.get::<_, Option<String>>(3)?,
                    certainty: row.get::<_, Option<String>>(4)?,
                    source: row.get::<_, Option<String>>(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut facts = HashMap::new();
        for mut fact in rows {
            if is_example_fact_path(&fact.path) {
                continue;
            }
            if let Some(scope_filter) = scope {
                let Some(fact_scope) = fact.scope.as_deref() else {
                    continue;
                };
                if fact_scope != "user" && fact_scope != scope_filter {
                    continue;
                }
            }
            if fact.title.trim().is_empty() {
                fact.title = stem_of(&fact.path);
            }
            if fact.claim.trim().is_empty() {
                fact.claim = fact.title.clone();
            }
            facts.insert(fact.path.clone(), fact);
        }

        Ok(facts)
    }

    fn search_memory_facts(
        &self,
        query: &str,
        limit: usize,
        allowed_paths: &HashSet<String>,
    ) -> Result<(bool, Vec<HybridHit>)> {
        if allowed_paths.is_empty() {
            return Ok((self.hybrid_search().is_some(), Vec::new()));
        }

        if let Some(hybrid) = self.hybrid_search() {
            let hits = hybrid.search_filtered(query, limit, Some(allowed_paths))?;
            return Ok((true, hits));
        }

        let lexical = self
            .search_index
            .search_in_paths(query, limit, allowed_paths)?;
        let hits = lexical
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
        Ok((false, hits))
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
    fn vault_name(&self) -> &str {
        &self.vault_name
    }

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

    fn memory_recall(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value> {
        let query = query.trim();
        if query.is_empty() {
            anyhow::bail!("empty memory recall query");
        }
        let limit = validate_limit(
            limit,
            DEFAULT_MEMORY_RECALL_LIMIT,
            MAX_MEMORY_RECALL_LIMIT,
            "memory_recall",
        )?;
        let facts = self.active_fact_candidates(scope)?;
        let allowed_paths = facts.keys().cloned().collect::<HashSet<_>>();
        let (embeddings_used, hits) = self.search_memory_facts(query, limit, &allowed_paths)?;
        let facts = hits
            .into_iter()
            .enumerate()
            .filter_map(|(idx, hit)| {
                let meta = facts.get(&hit.path)?;
                Some(MemoryRecallHit {
                    path: hit.path,
                    title: if meta.title.is_empty() {
                        hit.title
                    } else {
                        meta.title.clone()
                    },
                    claim: meta.claim.clone(),
                    scope: meta.scope.clone(),
                    certainty: meta.certainty.clone(),
                    source: meta.source.clone(),
                    snippet: hit.snippet,
                    score: hit.score,
                    rank: idx + 1,
                    lexical_rank: hit.lexical_rank,
                    semantic_rank: hit.semantic_rank,
                    char_start: hit.char_start,
                    char_end: hit.char_end,
                })
            })
            .collect::<Vec<_>>();

        Ok(serde_json::to_value(MemoryRecallResponse {
            query: query.to_string(),
            scope: scope.map(ToOwned::to_owned),
            limit,
            match_count: facts.len(),
            embeddings_used,
            facts,
        })?)
    }

    fn query_sql(&self, sql: &str) -> Result<Value> {
        Ok(serde_json::to_value(execute_sql(&self.cache, sql)?)?)
    }

    fn time_query(
        &self,
        when: &str,
        date_field: Option<&str>,
        query: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value> {
        let now = Local::now().naive_local();
        let (start, end) = time_query::parse_time_range(when, now)?;
        let field = match date_field.unwrap_or("mtime") {
            "mtime" => DateField::Mtime,
            "updated" => DateField::Updated,
            "created" => DateField::Created,
            other => anyhow::bail!(
                "invalid date_field '{other}': expected one of mtime, updated, created"
            ),
        };
        let limit = limit.unwrap_or(50);

        // Optional keyword restriction: map path -> lexical snippet.
        let text_filter: Option<HashMap<String, String>> = match query {
            Some(q) if !q.trim().is_empty() => {
                let hits = self.search_index.search(q, 500)?;
                Some(hits.into_iter().map(|h| (h.path, h.snippet)).collect())
            }
            _ => None,
        };

        let conn = self.cache.connection();
        let mut matches: Vec<Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // 1) Regular notes filtered on the chosen date field.
        let mut stmt = conn.prepare(
            "SELECT path, title, created_at, updated_at, mtime_unix, body_excerpt \
             FROM notes ORDER BY mtime_unix DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        for (path, title, created_at, updated_at, mtime_unix, excerpt) in rows {
            let matched = match field {
                DateField::Mtime => unix_to_naive(mtime_unix),
                DateField::Updated => updated_at
                    .as_deref()
                    .and_then(parse_flexible_datetime)
                    .or_else(|| unix_to_naive(mtime_unix)),
                DateField::Created => created_at.as_deref().and_then(parse_flexible_datetime),
            };
            let Some(matched_dt) = matched else {
                continue;
            };
            if matched_dt < start || matched_dt >= end {
                continue;
            }
            if let Some(ref tf) = text_filter {
                if !tf.contains_key(&path) {
                    continue;
                }
            }
            let snippet = text_filter
                .as_ref()
                .and_then(|tf| tf.get(&path).cloned())
                .unwrap_or(excerpt);
            seen.insert(path.clone());
            matches.push(json!({
                "path": path,
                "title": title,
                "source": "note",
                "date_field": field.as_str(),
                "matched_date": matched_dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "created_at": created_at,
                "updated_at": updated_at,
                "mtime_unix": mtime_unix,
                "snippet": snippet,
            }));
        }

        // 2) Periodic notes whose period overlaps the range, regardless of the
        // chosen date field (so "in May" surfaces May's daily/weekly notes even
        // if their file mtime is later).
        let query_end_inclusive = (end - chrono::Duration::seconds(1)).date();
        let query_start_date = start.date();
        let mut stmt = conn.prepare(
            "SELECT p.note_path, n.title, p.period_kind, p.period_key, p.period_start, \
                    p.period_end, n.body_excerpt \
             FROM periodic_notes p \
             JOIN notes n ON n.vault_name = p.vault_name AND n.path = p.note_path \
             WHERE p.vault_name = ?1 ORDER BY p.period_start DESC",
        )?;
        let periodic_rows = stmt
            .query_map([&self.vault_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        for (path, title, kind, key, period_start, period_end, excerpt) in periodic_rows {
            if seen.contains(&path) {
                continue;
            }
            let (Some(ps), Some(pe)) = (
                parse_flexible_date(&period_start),
                parse_flexible_date(&period_end),
            ) else {
                continue;
            };
            // Inclusive period [ps, pe] overlaps half-open query [start, end).
            if ps > query_end_inclusive || pe < query_start_date {
                continue;
            }
            if let Some(ref tf) = text_filter {
                if !tf.contains_key(&path) {
                    continue;
                }
            }
            let snippet = text_filter
                .as_ref()
                .and_then(|tf| tf.get(&path).cloned())
                .unwrap_or(excerpt);
            seen.insert(path.clone());
            matches.push(json!({
                "path": path,
                "title": title,
                "source": "periodic",
                "period_kind": kind,
                "period_key": key,
                "period_start": period_start,
                "period_end": period_end,
                "snippet": snippet,
            }));
        }

        let total = matches.len();
        matches.truncate(limit);
        Ok(json!({
            "expression": when,
            "date_field": field.as_str(),
            "range_start": start.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "range_end": end.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "match_count": total,
            "notes": matches,
        }))
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
    fn vault_name(&self) -> &str {
        self.inner.vault_name()
    }

    fn get_note(&self, path: &str) -> Result<Value> {
        self.inner.get_note(path)
    }

    fn search_notes(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        self.inner.search_notes(query, limit)
    }

    fn vault_search(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        self.inner.vault_search(query, limit)
    }

    fn memory_recall(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value> {
        self.inner.memory_recall(query, scope, limit)
    }

    fn time_query(
        &self,
        when: &str,
        date_field: Option<&str>,
        query: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value> {
        self.inner.time_query(when, date_field, query, limit)
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

fn validate_limit(
    limit: Option<usize>,
    default: usize,
    max: usize,
    operation: &str,
) -> Result<usize> {
    match limit.unwrap_or(default) {
        0 => anyhow::bail!("{operation} limit must be at least 1"),
        value if value > max => anyhow::bail!("{operation} limit must be at most {max}"),
        value => Ok(value),
    }
}

/// The filename stem of a note path: drop the directory and a trailing `.md`
/// (case-insensitive). Wikilink targets are stored as stems, so link-graph
/// comparisons operate on stems rather than full paths.
fn stem_of(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    if file.len() >= 3 && file[file.len() - 3..].eq_ignore_ascii_case(".md") {
        file[..file.len() - 3].to_string()
    } else {
        file.to_string()
    }
}

fn is_example_fact_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.starts_with("examples/")
        || normalized.starts_with("facts/examples/")
        || normalized.contains("/examples/")
}

/// Which indexed date a [`Ops::time_query`] filters notes on.
#[derive(Clone, Copy)]
enum DateField {
    /// `notes.mtime_unix` — the file modification time (always present).
    Mtime,
    /// Frontmatter `updated`, falling back to mtime when absent.
    Updated,
    /// Frontmatter `created`.
    Created,
}

impl DateField {
    fn as_str(self) -> &'static str {
        match self {
            DateField::Mtime => "mtime",
            DateField::Updated => "updated",
            DateField::Created => "created",
        }
    }
}

/// Convert a Unix timestamp (seconds) to a local naive datetime for range
/// comparison. Returns `None` for out-of-range timestamps rather than panicking
/// on untrusted index data.
fn unix_to_naive(secs: i64) -> Option<NaiveDateTime> {
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.with_timezone(&Local).naive_local())
}

/// Parse a frontmatter date/datetime string flexibly.
///
/// Frontmatter dates are untrusted free text and appear in several shapes:
/// RFC 3339 (`2026-07-08T09:30:00Z`), space-separated (`2026-07-08 09:30:00`),
/// or a bare date (`2026-07-08`). Unparseable values yield `None` so the note
/// is simply skipped rather than aborting the query.
fn parse_flexible_datetime(raw: &str) -> Option<NaiveDateTime> {
    let raw = raw.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Local).naive_local());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(dt);
        }
    }
    parse_flexible_date(raw).map(|d| d.and_hms_opt(0, 0, 0).unwrap())
}

/// Parse a bare date string (`YYYY-MM-DD` or `YYYY/MM/DD`), tolerating a
/// trailing time component.
fn parse_flexible_date(raw: &str) -> Option<NaiveDate> {
    let raw = raw.trim();
    let date_part = raw.split(['T', ' ']).next().unwrap_or(raw);
    for fmt in ["%Y-%m-%d", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(date_part, fmt) {
            return Some(d);
        }
    }
    None
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
    fn memory_recall_filters_to_active_non_example_facts() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "facts/Keep.md",
            "---\n\
             type: fact\n\
             description: Coffee helps focused debugging.\n\
             scope: user\n\
             certainty: explicit\n\
             source: User statement\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Coffee helps focused debugging.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/Missing Status.md",
            "---\n\
             type: fact\n\
             description: Coffee is still allowed without status.\n\
             scope: vault:test-vault\n\
             certainty: observed\n\
             source: Brew log\n\
             tags: [fact]\n\
             ---\n\
             Coffee is still allowed without status.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/Superseded.md",
            "---\n\
             type: fact\n\
             description: Old coffee preference.\n\
             scope: user\n\
             status: superseded\n\
             tags: [fact]\n\
             ---\n\
             Old coffee preference.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/Retracted.md",
            "---\n\
             type: fact\n\
             description: Wrong coffee claim.\n\
             scope: user\n\
             status: retracted\n\
             tags: [fact]\n\
             ---\n\
             Wrong coffee claim.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/Example Tagged.md",
            "---\n\
             type: fact\n\
             description: Example coffee fact.\n\
             scope: user\n\
             status: active\n\
             tags: [fact, example]\n\
             ---\n\
             Example coffee fact.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/examples/Seed.md",
            "---\n\
             type: fact\n\
             description: Seed coffee fact.\n\
             scope: user\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Seed coffee fact.\n",
        );
        write_note(
            temp_dir.path(),
            "Inbox/Not A Fact.md",
            "---\n\
             type: note\n\
             ---\n\
             Coffee in an ordinary note.\n",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops.memory_recall("coffee", None, Some(10)).unwrap();
        let facts = result["facts"].as_array().unwrap();
        let paths: Vec<&str> = facts
            .iter()
            .map(|fact| fact["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"facts/Keep.md"));
        assert!(paths.contains(&"facts/Missing Status.md"));
        assert!(!paths.contains(&"facts/Superseded.md"));
        assert!(!paths.contains(&"facts/Retracted.md"));
        assert!(!paths.contains(&"facts/Example Tagged.md"));
        assert!(!paths.contains(&"facts/examples/Seed.md"));
        assert!(!paths.contains(&"Inbox/Not A Fact.md"));
    }

    #[test]
    fn memory_recall_scope_filter_includes_user_and_exact_scope_only() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "facts/User.md",
            "---\n\
             type: fact\n\
             description: Coffee is a user preference.\n\
             scope: user\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Coffee is a user preference.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/This Vault.md",
            "---\n\
             type: fact\n\
             description: Coffee belongs to this vault.\n\
             scope: vault:test-vault\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Coffee belongs to this vault.\n",
        );
        write_note(
            temp_dir.path(),
            "facts/Other Vault.md",
            "---\n\
             type: fact\n\
             description: Coffee belongs elsewhere.\n\
             scope: vault:other\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Coffee belongs elsewhere.\n",
        );
        let ops = build_test_ops(temp_dir.path());

        let scoped = ops
            .memory_recall("coffee", Some("vault:test-vault"), Some(10))
            .unwrap();
        let scoped_paths: Vec<&str> = scoped["facts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fact| fact["path"].as_str().unwrap())
            .collect();
        assert!(scoped_paths.contains(&"facts/User.md"));
        assert!(scoped_paths.contains(&"facts/This Vault.md"));
        assert!(!scoped_paths.contains(&"facts/Other Vault.md"));

        let unscoped = ops.memory_recall("coffee", None, Some(10)).unwrap();
        let unscoped_paths: Vec<&str> = unscoped["facts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fact| fact["path"].as_str().unwrap())
            .collect();
        assert!(unscoped_paths.contains(&"facts/User.md"));
        assert!(unscoped_paths.contains(&"facts/This Vault.md"));
        assert!(unscoped_paths.contains(&"facts/Other Vault.md"));
    }

    #[test]
    fn memory_recall_lexical_fallback_returns_stable_fields() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "facts/Launch Preference.md",
            "---\n\
             type: fact\n\
             title: Launch Preference\n\
             description: Prefer coffee before launch reviews.\n\
             scope: user\n\
             certainty: explicit\n\
             source: User statement\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Prefer coffee before launch reviews.\n",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops.memory_recall("launch", None, Some(5)).unwrap();
        assert_eq!(result["embeddings_used"], false);
        assert_eq!(result["match_count"], 1);
        let fact = &result["facts"].as_array().unwrap()[0];
        assert_eq!(fact["path"], "facts/Launch Preference.md");
        assert_eq!(fact["title"], "Launch Preference");
        assert_eq!(fact["claim"], "Prefer coffee before launch reviews.");
        assert_eq!(fact["scope"], "user");
        assert_eq!(fact["certainty"], "explicit");
        assert_eq!(fact["source"], "User statement");
        assert!(fact["snippet"].is_string());
        assert!(fact["score"].is_number());
        assert_eq!(fact["rank"], 1);
        assert_eq!(fact["lexical_rank"], 1);
        assert!(fact["semantic_rank"].is_null());
        assert!(fact["char_start"].is_null());
        assert!(fact["char_end"].is_null());
    }

    #[test]
    fn memory_recall_rejects_blank_queries_and_invalid_limits() {
        let temp_dir = TempDir::new().unwrap();
        let ops = build_test_ops(temp_dir.path());

        let blank = ops.memory_recall("   ", None, None).unwrap_err();
        assert!(blank.to_string().contains("empty memory recall query"));

        let zero = ops.memory_recall("coffee", None, Some(0)).unwrap_err();
        assert!(zero.to_string().contains("limit"));

        let too_large = ops.memory_recall("coffee", None, Some(101)).unwrap_err();
        assert!(too_large.to_string().contains("limit"));
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
    fn time_query_filters_by_created_field() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Journal/May Note.md",
            "---\ntype: note\ncreated: 2020-05-15\n---\nSpring planning",
        );
        write_note(
            temp_dir.path(),
            "Journal/June Note.md",
            "---\ntype: note\ncreated: 2020-06-15\n---\nSummer planning",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops
            .time_query("May 2020", Some("created"), None, None)
            .unwrap();
        assert_eq!(result["date_field"], "created");
        assert_eq!(result["match_count"], 1);
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["path"], "Journal/May Note.md");
        assert_eq!(notes[0]["source"], "note");

        // A whole-year range captures both notes.
        let year = ops.time_query("2020", Some("created"), None, None).unwrap();
        assert_eq!(year["match_count"], 2);
    }

    #[test]
    fn time_query_combines_with_keyword() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "Journal/Launch.md",
            "---\ntype: note\ncreated: 2020-05-10\n---\nLaunch timeline discussion",
        );
        write_note(
            temp_dir.path(),
            "Journal/Groceries.md",
            "---\ntype: note\ncreated: 2020-05-11\n---\nBuy milk and eggs",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops
            .time_query("May 2020", Some("created"), Some("launch"), None)
            .unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["path"], "Journal/Launch.md");
    }

    #[test]
    fn time_query_includes_periodic_notes_by_period() {
        let temp_dir = TempDir::new().unwrap();
        // Daily note with a period-parseable filename but no `created` field.
        write_note(
            temp_dir.path(),
            "2020-05-10.md",
            "---\ntype: daily\n---\nDaily log entry",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops
            .time_query("May 2020", Some("created"), None, None)
            .unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["path"], "2020-05-10.md");
        assert_eq!(notes[0]["source"], "periodic");
        assert_eq!(notes[0]["period_kind"], "daily");
    }

    #[test]
    fn time_query_rejects_bad_date_field() {
        let temp_dir = TempDir::new().unwrap();
        let ops = build_test_ops(temp_dir.path());
        let err = ops
            .time_query("last week", Some("bogus"), None, None)
            .unwrap_err();
        assert!(err.to_string().contains("invalid date_field"));
    }

    #[test]
    fn time_query_rejects_unparseable_expression() {
        let temp_dir = TempDir::new().unwrap();
        let ops = build_test_ops(temp_dir.path());
        assert!(ops.time_query("not a time zzz", None, None, None).is_err());
    }

    #[test]
    fn related_notes_ranks_by_link_graph() {
        let temp_dir = TempDir::new().unwrap();
        // Hub links to two spokes; Spoke A links back to Hub; Cousin shares the
        // Spoke A neighbour with Hub (bibliographic coupling).
        write_note(
            temp_dir.path(),
            "Hub.md",
            "---\ntype: note\n---\nSee [[Spoke A]] and [[Spoke B]].",
        );
        write_note(
            temp_dir.path(),
            "Spoke A.md",
            "---\ntype: note\n---\nBack to [[Hub]].",
        );
        write_note(temp_dir.path(), "Spoke B.md", "---\ntype: note\n---\nLeaf.");
        write_note(
            temp_dir.path(),
            "Cousin.md",
            "---\ntype: note\n---\nAlso mentions [[Spoke A]].",
        );
        let ops = build_test_ops(temp_dir.path());

        let result = ops.related_notes("Hub.md", 10).unwrap();
        assert_eq!(result["path"], "Hub.md");
        assert_eq!(result["embeddings_used"], false);
        let related = result["related"].as_array().unwrap();
        let paths: Vec<&str> = related
            .iter()
            .map(|r| r["path"].as_str().unwrap())
            .collect();
        // Directly-linked spokes outrank the coupling-only cousin.
        assert_eq!(paths, vec!["Spoke A.md", "Spoke B.md", "Cousin.md"]);

        let cousin = related.iter().find(|r| r["path"] == "Cousin.md").unwrap();
        assert_eq!(cousin["directly_linked"], false);
        assert_eq!(cousin["shared_neighbors"], 1);
        assert_eq!(cousin["embedding_similarity"], Value::Null);

        let spoke_a = related.iter().find(|r| r["path"] == "Spoke A.md").unwrap();
        assert_eq!(spoke_a["directly_linked"], true);
    }

    #[test]
    fn related_notes_respects_limit_and_excludes_self() {
        let temp_dir = TempDir::new().unwrap();
        write_note(
            temp_dir.path(),
            "A.md",
            "---\ntype: note\n---\n[[B]] [[C]] [[D]]",
        );
        write_note(temp_dir.path(), "B.md", "---\ntype: note\n---\nb");
        write_note(temp_dir.path(), "C.md", "---\ntype: note\n---\nc");
        write_note(temp_dir.path(), "D.md", "---\ntype: note\n---\nd");
        let ops = build_test_ops(temp_dir.path());

        let result = ops.related_notes("A.md", 2).unwrap();
        let related = result["related"].as_array().unwrap();
        assert_eq!(related.len(), 2);
        assert!(related.iter().all(|r| r["path"] != "A.md"));
    }

    #[test]
    fn related_notes_errors_for_missing_note() {
        let temp_dir = TempDir::new().unwrap();
        let ops = build_test_ops(temp_dir.path());
        assert!(ops.related_notes("Nope.md", 10).is_err());
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
        assert!(ops.memory_recall("Visible", None, Some(5)).is_ok());
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

    #[test]
    fn memory_recall_uses_hybrid_search_when_embeddings_are_available() {
        let vault_name = "ops-memory-recall-hybrid";
        let data_root = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_root.path());
        }

        let vault = TempDir::new().unwrap();
        write_note(
            vault.path(),
            "facts/Coffee.md",
            "---\n\
             type: fact\n\
             description: Prefer coffee before launch reviews.\n\
             scope: user\n\
             certainty: explicit\n\
             source: User statement\n\
             status: active\n\
             tags: [fact]\n\
             ---\n\
             Prefer coffee before launch reviews.\n",
        );

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

        let cache_enabled = TempDir::new().unwrap();
        let ops = build_gated_ops(vault.path(), cache_enabled.path(), vault_name, true);
        let result = ops.memory_recall("coffee", None, Some(5)).unwrap();
        assert_eq!(result["embeddings_used"], true);
        let fact = &result["facts"].as_array().unwrap()[0];
        assert_eq!(fact["path"], "facts/Coffee.md");
        assert!(fact["semantic_rank"].as_u64().is_some());

        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }
    }
}
