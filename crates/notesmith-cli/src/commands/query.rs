use std::path::Path;

use anyhow::Context;
use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};
use notesmith_query::QueryResult;
use reqwest::Url;

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum QueryCommand {
    /// Execute read-only SQL against the daemon cache
    Sql {
        /// SQL statement to execute
        sql: String,
    },
}

impl QueryCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            QueryCommand::Sql { sql } => {
                cmd_sql(global_config, explicit_vault, cwd, sql, format).await
            }
        }
    }
}

async fn cmd_sql(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    sql: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let mut url = Url::parse(&format!("http://{}/", global_config.daemon.bind))
        .with_context(|| format!("invalid daemon bind address: {}", global_config.daemon.bind))?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
        .push("api")
        .push("v")
        .push(&detected.name)
        .push("query")
        .push("sql");

    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "sql": sql }))
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                anyhow::anyhow!(
                    "could not reach the Notesmith daemon at {}. Start it with `notesmith daemon start`",
                    global_config.daemon.bind
                )
            } else {
                anyhow::anyhow!("query request failed: {error}")
            }
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("query failed with {status}: {body}");
    }

    let result: QueryResult = response.json().await?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
        OutputFormat::Text => print_table(&result),
    }

    Ok(())
}

fn print_table(result: &QueryResult) {
    if result.columns.is_empty() {
        println!("(no columns)");
        return;
    }

    let mut widths = result
        .columns
        .iter()
        .map(|column| column.len())
        .collect::<Vec<_>>();
    let rows = result
        .rows
        .iter()
        .map(|row| row.iter().map(cell_to_string).collect::<Vec<_>>())
        .collect::<Vec<_>>();

    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    println!("{}", format_row(&result.columns, &widths));
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for row in rows {
        println!("{}", format_row(&row, &widths));
    }
}

fn cell_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn format_row(row: &[String], widths: &[usize]) -> String {
    row.iter()
        .zip(widths)
        .map(|(cell, width)| format!("{cell:width$}"))
        .collect::<Vec<_>>()
        .join(" | ")
}
