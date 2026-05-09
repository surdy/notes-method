use chrono::Local;
use serde_yaml::{Mapping, Value};

pub fn apply_save_pipeline(content: &str) -> String {
    apply_save_pipeline_with_timestamp(content, &now_timestamp())
}

pub fn apply_save_pipeline_with_timestamp(content: &str, timestamp: &str) -> String {
    let (raw_frontmatter, body) = extract_frontmatter(content);

    let Some(raw_frontmatter) = raw_frontmatter else {
        return finalize_document(normalize_lines(content));
    };

    let Some(mut frontmatter) = parse_frontmatter_mapping(&raw_frontmatter) else {
        return finalize_document(normalize_lines(content));
    };

    let created_key = Value::String("created".to_string());
    if !frontmatter.contains_key(&created_key) {
        frontmatter.insert(created_key, Value::String(timestamp.to_string()));
    }
    frontmatter.insert(
        Value::String("updated".to_string()),
        Value::String(timestamp.to_string()),
    );

    let sorted_frontmatter = sort_mapping(frontmatter);
    let yaml = serialize_frontmatter(&sorted_frontmatter);
    let normalized_body = normalize_lines(body);
    let rebuilt = if normalized_body.is_empty() {
        format!("---\n{yaml}\n---")
    } else {
        format!("---\n{yaml}\n---\n{normalized_body}")
    };

    finalize_document(rebuilt)
}

fn now_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}

fn extract_frontmatter(content: &str) -> (Option<String>, &str) {
    let mut lines = content.split_inclusive('\n');
    let Some(first_line) = lines.next() else {
        return (None, content);
    };
    if trim_line_ending(first_line) != "---" {
        return (None, content);
    }

    let mut offset = first_line.len();
    for line in lines {
        let line_start = offset;
        offset += line.len();
        if trim_line_ending(line) == "---" {
            let raw = content[first_line.len()..line_start]
                .trim_end_matches(['\r', '\n'])
                .to_string();
            return (Some(raw), &content[offset..]);
        }
    }

    (None, content)
}

fn trim_line_ending(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn parse_frontmatter_mapping(raw_frontmatter: &str) -> Option<Mapping> {
    if raw_frontmatter.trim().is_empty() {
        return Some(Mapping::new());
    }

    match serde_yaml::from_str::<Value>(raw_frontmatter).ok()? {
        Value::Mapping(mapping) => Some(mapping),
        Value::Null => Some(Mapping::new()),
        _ => None,
    }
}

fn sort_mapping(mapping: Mapping) -> Mapping {
    let mut entries = mapping.into_iter().collect::<Vec<_>>();
    entries.sort_by(|(left_key, _), (right_key, _)| {
        yaml_key_sort_key(left_key).cmp(&yaml_key_sort_key(right_key))
    });

    let mut sorted = Mapping::new();
    for (key, value) in entries {
        sorted.insert(key, value);
    }
    sorted
}

fn yaml_key_sort_key(key: &Value) -> String {
    match key {
        Value::String(text) => text.clone(),
        other => serde_yaml::to_string(other).unwrap_or_default(),
    }
}

fn serialize_frontmatter(frontmatter: &Mapping) -> String {
    let serialized =
        serde_yaml::to_string(&Value::Mapping(frontmatter.clone())).unwrap_or_default();
    serialized
        .strip_prefix("---\n")
        .unwrap_or(&serialized)
        .trim_end_matches('\n')
        .to_string()
}

fn normalize_lines(content: &str) -> String {
    content
        .split('\n')
        .map(|line| line.trim_end_matches([' ', '\t', '\r']))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches('\n')
        .to_string()
}

fn finalize_document(content: String) -> String {
    format!("{}\n", content.trim_end_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::apply_save_pipeline_with_timestamp;

    const TIMESTAMP: &str = "2026-05-09 14:30";

    #[test]
    fn test_stamps_created_when_missing() {
        let output =
            apply_save_pipeline_with_timestamp("---\ntitle: Example\n---\nBody", TIMESTAMP);

        assert!(output.contains("created: 2026-05-09 14:30"));
        assert!(output.contains("updated: 2026-05-09 14:30"));
    }

    #[test]
    fn test_preserves_existing_created() {
        let output = apply_save_pipeline_with_timestamp(
            "---\ncreated: 2024-01-01 09:00\ntitle: Example\n---\nBody",
            TIMESTAMP,
        );

        assert!(output.contains("created: 2024-01-01 09:00"));
        assert!(!output.contains("created: 2026-05-09 14:30"));
    }

    #[test]
    fn test_stamps_updated_on_every_write() {
        let output = apply_save_pipeline_with_timestamp(
            "---\ncreated: 2024-01-01 09:00\nupdated: 2024-01-02 10:00\n---\nBody",
            TIMESTAMP,
        );

        assert!(output.contains("updated: 2026-05-09 14:30"));
        assert!(!output.contains("updated: 2024-01-02 10:00"));
    }

    #[test]
    fn test_sorts_yaml_keys_alphabetically() {
        let output = apply_save_pipeline_with_timestamp(
            "---\nzebra: 1\napple: 2\nmiddle: 3\n---\nBody",
            TIMESTAMP,
        );

        let apple = output.find("apple: 2").unwrap();
        let created = output.find("created: 2026-05-09 14:30").unwrap();
        let middle = output.find("middle: 3").unwrap();
        let updated = output.find("updated: 2026-05-09 14:30").unwrap();
        let zebra = output.find("zebra: 1").unwrap();

        assert!(apple < created);
        assert!(created < middle);
        assert!(middle < updated);
        assert!(updated < zebra);
    }

    #[test]
    fn test_trims_trailing_whitespace() {
        let output = apply_save_pipeline_with_timestamp(
            "---\ntitle: Example   \n---\nBody with space   \nAnother line\t\t",
            TIMESTAMP,
        );

        assert!(output.contains("title: Example\n"));
        assert!(output.contains("Body with space\nAnother line\n"));
        assert!(!output.contains("   \n"));
        assert!(!output.contains("\t"));
    }

    #[test]
    fn test_ensures_single_trailing_newline() {
        let no_newline = apply_save_pipeline_with_timestamp("---\n---\nBody", TIMESTAMP);
        let many_newlines = apply_save_pipeline_with_timestamp("---\n---\nBody\n\n\n", TIMESTAMP);

        assert!(no_newline.ends_with('\n'));
        assert!(!no_newline.ends_with("\n\n"));
        assert!(many_newlines.ends_with('\n'));
        assert!(!many_newlines.ends_with("\n\n"));
    }

    #[test]
    fn test_no_frontmatter_pass_through() {
        let output =
            apply_save_pipeline_with_timestamp("Hello world   \nSecond line\t\t", TIMESTAMP);

        assert_eq!(output, "Hello world\nSecond line\n");
    }

    #[test]
    fn test_empty_frontmatter() {
        let output = apply_save_pipeline_with_timestamp("---\n---\n", TIMESTAMP);

        assert!(output.contains("created: 2026-05-09 14:30"));
        assert!(output.contains("updated: 2026-05-09 14:30"));
        assert!(output.ends_with("---\n"));
    }

    #[test]
    fn test_roundtrip_preserves_body() {
        let input = "---\ntitle: Example\n---\n# Heading\n\n- item one\n- item two";
        let output = apply_save_pipeline_with_timestamp(input, TIMESTAMP);

        assert!(output.ends_with("---\n# Heading\n\n- item one\n- item two\n"));
    }
}
