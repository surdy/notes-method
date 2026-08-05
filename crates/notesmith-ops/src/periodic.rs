//! Shared periodic-note path resolution and creation (issue #279).
//!
//! Single source of truth for where a periodic (daily/weekly/monthly/...)
//! note lives and how it is created. Every daily-note entry point — the MCP
//! `create_daily_note` tool (via [`crate::Ops`]), the HTTP daily routes, the
//! CLI (which calls the HTTP routes), and the daemon scheduler — resolves the
//! note path through [`resolve_periodic_note`], so a custom
//! `[periodic.daily] filename` pattern yields the identical path everywhere.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, anyhow};
use chrono::NaiveDate;
use notesmith_config::PeriodicConfig;
use notesmith_core::{NotesmithError, PeriodKind, VaultEngine, VaultPath};
use notesmith_templates::TemplateEngine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPeriodicNote {
    pub kind: PeriodKind,
    pub key: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsurePeriodicResult {
    pub note: ResolvedPeriodicNote,
    pub created_path: Option<String>,
}

/// Resolve the vault-relative path (and period bounds) of a periodic note
/// from the configured folder and filename pattern.
pub fn resolve_periodic_note(
    periodic: &PeriodicConfig,
    kind: PeriodKind,
    date: NaiveDate,
    template_engine: &TemplateEngine,
) -> anyhow::Result<ResolvedPeriodicNote> {
    let config = periodic
        .kind_config(kind)
        .ok_or_else(|| anyhow!("periodic {kind} is not configured"))?;
    let prompts = periodic_template_context(kind, date);
    let rendered_name = template_engine
        .render_text(&config.filename, &prompts)
        .with_context(|| format!("failed to render {} filename", kind.as_str()))?;

    let path = if config.folder.is_empty() {
        format!("{rendered_name}.md")
    } else {
        format!("{}/{rendered_name}.md", config.folder)
    };
    let (period_start, period_end) = kind.period_bounds(date);

    Ok(ResolvedPeriodicNote {
        kind,
        key: kind.current_key(date),
        period_start,
        period_end,
        path,
    })
}

/// Ensure the periodic note for `date` exists at its resolved path, creating
/// it from the configured template when missing. The exists/created outcome
/// always refers to the resolved path: the template renders *to* that path,
/// its own `output_path` never decides where the note lands.
pub fn ensure_periodic_note(
    vault_root: &Path,
    periodic: &PeriodicConfig,
    kind: PeriodKind,
    date: NaiveDate,
    template_engine: &TemplateEngine,
    engine: &dyn VaultEngine,
) -> anyhow::Result<EnsurePeriodicResult> {
    let note = resolve_periodic_note(periodic, kind, date, template_engine)?;
    let vault_path = VaultPath::new(note.path.clone());
    match engine.read(vault_root, &vault_path) {
        Ok(_) => {
            return Ok(EnsurePeriodicResult {
                note,
                created_path: None,
            });
        }
        Err(NotesmithError::NoteNotFound { .. }) => {}
        Err(error) => return Err(error.into()),
    }

    let config = periodic
        .kind_config(kind)
        .ok_or_else(|| anyhow!("periodic {kind} is not configured"))?;
    let prompts = periodic_template_context(kind, date);
    let content = match config
        .template
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(template_name) => {
            template_engine
                .render_to_path(template_name, &prompts, &note.path)
                .with_context(|| {
                    format!(
                        "failed to render {} template {template_name}",
                        kind.as_str()
                    )
                })?
                .content
        }
        None => String::new(),
    };
    let content = notesmith_vault::apply_save_pipeline(&content);
    engine.write(vault_root, &vault_path, None, &content)?;

    Ok(EnsurePeriodicResult {
        created_path: Some(note.path.clone()),
        note,
    })
}

fn periodic_template_context(kind: PeriodKind, date: NaiveDate) -> HashMap<String, String> {
    let (period_start, period_end) = kind.period_bounds(date);
    let mut prompts = HashMap::new();
    prompts.insert("today".to_string(), date.format("%Y-%m-%d").to_string());
    prompts.insert("date".to_string(), date.format("%Y-%m-%d").to_string());
    prompts.insert("week".to_string(), PeriodKind::Weekly.current_key(date));
    prompts.insert("month".to_string(), PeriodKind::Monthly.current_key(date));
    prompts.insert(
        "quarter".to_string(),
        PeriodKind::Quarterly.current_key(date),
    );
    prompts.insert("year".to_string(), PeriodKind::Yearly.current_key(date));
    prompts.insert("day_name".to_string(), date.format("%A").to_string());
    prompts.insert("period_kind".to_string(), kind.to_string());
    prompts.insert("period_key".to_string(), kind.current_key(date));
    prompts.insert("period_start".to_string(), period_start.to_string());
    prompts.insert("period_end".to_string(), period_end.to_string());
    prompts
}
