use std::str::FromStr;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Datelike;
use notesmith_core::{PeriodKind, VaultEngine, VaultName, VaultPath};
use notesmith_vault::parse_note;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::{SharedAppState, VaultState};

use super::helpers::{internal_error, note_error};

#[derive(Debug, Default, Deserialize)]
pub struct CurrentPeriodicQuery {
    #[serde(default)]
    pub offset: i32,
}

#[derive(Debug, Default, Deserialize)]
pub struct PeriodicListQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn get_current_periodic_note(
    State(state): State<SharedAppState>,
    Path((vault_name, kind)): Path<(String, String)>,
    Query(query): Query<CurrentPeriodicQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let kind = parse_period_kind(&kind)?;
    let target_date = shift_period_date(kind, chrono::Local::now().date_naive(), query.offset);
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let config = vault.vault_config.load();
    let ensured = crate::scheduler::ensure_periodic_note(
        &vault.root,
        &config.periodic,
        kind,
        target_date,
        &vault.template_engine,
        &vault.engine,
    )
    .map_err(internal_error)?;

    if let Some(path) = ensured.created_path.as_deref() {
        refresh_indexes(vault, &vault_name, path).map_err(internal_error)?;
        events::emit(
            &state.event_tx,
            &state.event_buffer,
            VaultEvent::new(
                &vault_name,
                if kind == PeriodKind::Daily {
                    EventType::DailyCreated
                } else {
                    EventType::PeriodicCreated
                },
                path,
            ),
        );
    }

    let note_path = VaultPath::new(ensured.note.path.clone());
    let content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;
    let parsed = parse_note(&VaultName::new(vault_name.clone()), &note_path, &content);

    Ok(Json(json!({
        "created": ensured.created_path.is_some(),
        "path": ensured.note.path,
        "content": content,
        "frontmatter": parsed.frontmatter,
        "period_kind": ensured.note.kind.to_string(),
        "period_key": ensured.note.key,
        "period_start": ensured.note.period_start.to_string(),
        "period_end": ensured.note.period_end.to_string(),
    })))
}

pub async fn list_periodic_notes(
    State(state): State<SharedAppState>,
    Path((vault_name, kind)): Path<(String, String)>,
    Query(query): Query<PeriodicListQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let kind = parse_period_kind(&kind)?;
    let from = parse_optional_date(query.from.as_deref())?;
    let to = parse_optional_date(query.to.as_deref())?;

    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let rows: Vec<Value> = {
        let conn = vault.cache.connection();
        let mut stmt = conn
            .prepare(
                "SELECT note_path, period_kind, period_key, period_start, period_end
                 FROM v_periodic
                 WHERE period_kind = ?1
                   AND (?2 IS NULL OR period_end >= ?2)
                   AND (?3 IS NULL OR period_start <= ?3)
                 ORDER BY period_start, note_path",
            )
            .map_err(internal_error)?;
        stmt.query_map(
            rusqlite::params![kind.to_string(), from.as_deref(), to.as_deref()],
            |row| {
                Ok(json!({
                    "path": row.get::<_, String>(0)?,
                    "period_kind": row.get::<_, String>(1)?,
                    "period_key": row.get::<_, String>(2)?,
                    "period_start": row.get::<_, String>(3)?,
                    "period_end": row.get::<_, String>(4)?,
                }))
            },
        )
        .map_err(internal_error)?
        .collect::<Result<_, _>>()
        .map_err(internal_error)?
    };

    Ok(Json(json!(rows)))
}

fn parse_period_kind(kind: &str) -> Result<PeriodKind, (StatusCode, Json<Value>)> {
    PeriodKind::from_str(kind)
        .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))))
}

fn parse_optional_date(date: Option<&str>) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    date.map(|value| {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|parsed| parsed.to_string())
            .map_err(|error| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("invalid date: {error}") })),
                )
            })
    })
    .transpose()
}

fn shift_period_date(kind: PeriodKind, date: chrono::NaiveDate, offset: i32) -> chrono::NaiveDate {
    match kind {
        PeriodKind::Daily => date + chrono::Duration::days(offset as i64),
        PeriodKind::Weekly => date + chrono::Duration::weeks(offset as i64),
        PeriodKind::Monthly => shift_months(date, offset),
        PeriodKind::Quarterly => shift_months(date, offset * 3),
        PeriodKind::Yearly => shift_months(date, offset * 12),
    }
}

fn shift_months(date: chrono::NaiveDate, offset_months: i32) -> chrono::NaiveDate {
    let month_index = date.month0() as i32 + offset_months;
    let year = date.year() + month_index.div_euclid(12);
    let month0 = month_index.rem_euclid(12) as u32;
    let month = month0 + 1;
    let last_day = last_day_of_month(year, month);
    chrono::NaiveDate::from_ymd_opt(year, month, date.day().min(last_day)).unwrap()
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    (chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap() - chrono::Duration::days(1))
        .day()
}

fn refresh_indexes(vault: &VaultState, vault_name: &str, path: &str) -> anyhow::Result<()> {
    let note_path = VaultPath::new(path.to_string());
    let content = vault.engine.read(&vault.root, &note_path)?;
    let note = parse_note(
        &VaultName::new(vault_name.to_string()),
        &note_path,
        &content,
    );
    let config = vault.vault_config.load();
    vault
        .cache
        .update_note_with_periodic(vault_name, &note, &config.periodic)?;
    vault.search_index.update_note(vault_name, &note)?;
    Ok(())
}
