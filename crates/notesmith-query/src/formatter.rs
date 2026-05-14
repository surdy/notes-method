use serde_json::Value;

use crate::QueryResult;

pub fn format_query_as_markdown_table(result: &QueryResult) -> String {
    if result.row_count == 0 || result.rows.is_empty() {
        return "(no results)".to_string();
    }

    let header = format!("| {} |", result.columns.join(" | "));
    let separator = format!(
        "| {} |",
        result
            .columns
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let rows = result
        .rows
        .iter()
        .map(|row| {
            format!(
                "| {} |",
                row.iter()
                    .map(format_markdown_cell)
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("{header}\n{separator}\n{rows}")
}

pub fn format_markdown_cell(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    text.replace(['\n', '\r'], " ").replace('|', "\\|")
}
