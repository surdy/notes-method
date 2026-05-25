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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{format_markdown_cell, format_query_as_markdown_table};
    use crate::QueryResult;

    #[test]
    fn format_markdown_cell_returns_empty_string_for_null() {
        assert_eq!(format_markdown_cell(&Value::Null), "");
    }

    #[test]
    fn format_markdown_cell_returns_plain_string_text() {
        assert_eq!(
            format_markdown_cell(&Value::String("hello".to_string())),
            "hello"
        );
    }

    #[test]
    fn format_markdown_cell_formats_numbers() {
        assert_eq!(format_markdown_cell(&json!(42)), "42");
    }

    #[test]
    fn format_markdown_cell_formats_booleans() {
        assert_eq!(format_markdown_cell(&json!(true)), "true");
    }

    #[test]
    fn format_markdown_cell_escapes_pipes() {
        assert_eq!(format_markdown_cell(&json!("left|right")), "left\\|right");
    }

    #[test]
    fn format_markdown_cell_replaces_newlines_with_spaces() {
        assert_eq!(format_markdown_cell(&json!("left\nright")), "left right");
    }

    #[test]
    fn format_markdown_cell_escapes_pipes_and_replaces_newlines() {
        assert_eq!(
            format_markdown_cell(&json!("left|\nright")),
            "left\\| right"
        );
    }

    #[test]
    fn format_query_as_markdown_table_returns_placeholder_for_empty_results() {
        let result = QueryResult {
            columns: vec!["title".to_string()],
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
        };

        assert_eq!(format_query_as_markdown_table(&result), "(no results)");
    }

    #[test]
    fn format_query_as_markdown_table_formats_single_row_single_column() {
        let result = QueryResult {
            columns: vec!["title".to_string()],
            rows: vec![vec![json!("Inbox")]],
            row_count: 1,
            truncated: false,
        };

        assert_eq!(
            format_query_as_markdown_table(&result),
            "| title |\n| --- |\n| Inbox |"
        );
    }

    #[test]
    fn format_query_as_markdown_table_formats_multiple_rows_and_columns() {
        let result = QueryResult {
            columns: vec!["title".to_string(), "done".to_string()],
            rows: vec![
                vec![json!("Inbox"), json!(false)],
                vec![json!("Today"), json!(true)],
            ],
            row_count: 2,
            truncated: false,
        };

        assert_eq!(
            format_query_as_markdown_table(&result),
            "| title | done |\n| --- | --- |\n| Inbox | false |\n| Today | true |"
        );
    }
}
