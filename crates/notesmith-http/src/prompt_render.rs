//! Rendering of vault agent-prompt templates (issues #282; extracted from the
//! daily `agent-create` route, which pioneered the format).
//!
//! An agent prompt is a markdown file in the vault's `.notesmith/prompts/`
//! folder whose YAML frontmatter may declare `context_queries` — named SQL
//! queries executed against the vault index. Rendering replaces each
//! `{{ name }}` placeholder with the query's result as a markdown table and
//! `{{ today }}` with the target date. The body is otherwise verbatim.
//!
//! Distinct from `notesmith-prompts` (`_prompts/` static custom prompts sent
//! verbatim to the chat UI): agent prompts are *rendered* with live vault
//! context for headless agent runs.
//!
//! Malformed templates degrade per ADR 0009: errors are returned as typed
//! values the callers map to HTTP statuses (or CLI failures) — never a panic.

use notesmith_query::{QueryError, execute_sql, format_query_as_markdown_table};
use notesmith_vault::extract_frontmatter;
use serde::Deserialize;

use crate::server::VaultState;

/// One named SQL context query declared in a prompt template's frontmatter.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ContextQuery {
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Default, Deserialize)]
struct PromptTemplateFrontmatter {
    #[serde(default)]
    context_queries: Vec<ContextQuery>,
}

/// Why a prompt could not be rendered. Callers map these to HTTP statuses
/// (404 / 500 / 422) or CLI exit failures.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("prompt template not found: {path}")]
    NotFound { path: String },
    #[error("failed to read prompt template {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid prompt template frontmatter: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("context query failed: {0}")]
    Query(#[from] QueryError),
}

/// Whether `name` is safe to resolve under `.notesmith/prompts/` — a bare
/// file stem, no path traversal.
pub fn is_valid_prompt_name(name: &str) -> bool {
    !name.trim().is_empty() && !name.contains(['/', '\\']) && !name.contains("..") && name != "."
}

/// Split a prompt template into its declared context queries and body.
pub fn parse_prompt_template(content: &str) -> Result<(Vec<ContextQuery>, String), RenderError> {
    let (raw_frontmatter, body) = extract_frontmatter(content);
    let frontmatter = match raw_frontmatter {
        Some(raw) => serde_yaml::from_str::<PromptTemplateFrontmatter>(&raw)?,
        None => PromptTemplateFrontmatter::default(),
    };

    Ok((
        frontmatter.context_queries,
        body.trim_start_matches(['\r', '\n']).to_string(),
    ))
}

/// Render the vault's `.notesmith/prompts/<name>.md` for `date_str`
/// (`YYYY-MM-DD`, substituted for `{{ today }}`): execute every context query
/// against the vault index and splice the markdown tables in.
pub fn render_prompt(
    vault: &VaultState,
    name: &str,
    date_str: &str,
) -> Result<String, RenderError> {
    let prompt_path = vault
        .root
        .join(".notesmith")
        .join("prompts")
        .join(format!("{name}.md"));
    let template = std::fs::read_to_string(&prompt_path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            RenderError::NotFound {
                path: prompt_path.display().to_string(),
            }
        } else {
            RenderError::Io {
                path: prompt_path.display().to_string(),
                source,
            }
        }
    })?;

    let (queries, body) = parse_prompt_template(&template)?;
    let mut prompt = body;
    for query in queries {
        let result = execute_sql(&vault.cache, &query.sql)?;
        let table = format_query_as_markdown_table(&result);
        prompt = prompt
            .replace(&format!("{{{{ {} }}}}", query.name), &table)
            .replace(&format!("{{{{{}}}}}", query.name), &table);
    }
    prompt = prompt
        .replace("{{ today }}", date_str)
        .replace("{{today}}", date_str);
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prompt_template_extracts_queries() {
        let template = r#"---
context_queries:
  - name: open_tasks
    sql: "SELECT text FROM v_tasks"
  - name: inbox_count
    sql: "SELECT COUNT(*) as count FROM v_notes"
---

# Daily Note Prompt

{{ open_tasks }}
"#;

        let (queries, body) = parse_prompt_template(template).unwrap();

        assert_eq!(
            queries,
            vec![
                ContextQuery {
                    name: "open_tasks".to_string(),
                    sql: "SELECT text FROM v_tasks".to_string(),
                },
                ContextQuery {
                    name: "inbox_count".to_string(),
                    sql: "SELECT COUNT(*) as count FROM v_notes".to_string(),
                },
            ]
        );
        assert!(body.contains("# Daily Note Prompt"));
        assert!(body.contains("{{ open_tasks }}"));
    }

    #[test]
    fn parse_prompt_template_without_frontmatter_is_plain_body() {
        let (queries, body) = parse_prompt_template("Just instructions.\n").unwrap();
        assert!(queries.is_empty());
        assert_eq!(body, "Just instructions.\n");
    }

    #[test]
    fn parse_prompt_template_bad_yaml_is_an_error_not_a_panic() {
        let template = "---\ncontext_queries: {not: [valid\n---\nBody\n";
        assert!(matches!(
            parse_prompt_template(template),
            Err(RenderError::Parse(_))
        ));
    }

    #[test]
    fn prompt_names_reject_path_traversal() {
        assert!(is_valid_prompt_name("daily-note"));
        assert!(is_valid_prompt_name("weekly_review"));
        assert!(!is_valid_prompt_name(""));
        assert!(!is_valid_prompt_name("  "));
        assert!(!is_valid_prompt_name("../secrets"));
        assert!(!is_valid_prompt_name("a/b"));
        assert!(!is_valid_prompt_name("a\\b"));
        assert!(!is_valid_prompt_name("."));
    }
}
