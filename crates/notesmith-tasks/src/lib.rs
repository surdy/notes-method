//! notesmith-tasks: content-hash anchored toggling and generic task insertion.

use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ToggleError {
    #[error("no task with hash '{hash}' found in note")]
    TaskNotFound { hash: String },
    #[error("hash '{hash}' matches {count} tasks; cannot toggle unambiguously")]
    HashCollision { hash: String, count: usize },
}

#[derive(Debug, Default, Clone)]
pub struct AddTaskOptions {
    pub status_char: Option<char>,
    pub fields: HashMap<String, String>,
}

pub fn toggle_task(
    content: &str,
    task_hash: &str,
    new_status_char: char,
) -> Result<String, ToggleError> {
    let re = task_line_regex();
    let lines = split_preserving_endings(content);

    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let stripped = strip_line_ending(line);
            let hash = blake3::hash(stripped.as_bytes()).to_hex().to_string();
            hash == task_hash && re.is_match(stripped)
        })
        .map(|(idx, _)| idx)
        .collect();

    match matches.len() {
        0 => Err(ToggleError::TaskNotFound {
            hash: task_hash.to_string(),
        }),
        n if n > 1 => Err(ToggleError::HashCollision {
            hash: task_hash.to_string(),
            count: n,
        }),
        _ => {
            let idx = matches[0];
            let line = lines[idx];
            let stripped = strip_line_ending(line);
            let ending = &line[stripped.len()..];

            let Some(caps) = re.captures(stripped) else {
                return Err(ToggleError::TaskNotFound {
                    hash: task_hash.to_string(),
                });
            };
            let indent = caps.name("indent").map_or("", |m| m.as_str());
            let task_content = caps.name("content").map_or("", |m| m.as_str());
            let new_line = format!(
                "{indent}- [{}] {task_content}{ending}",
                normalize_status_char(new_status_char)
            );

            let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            result[idx] = new_line;
            Ok(result.concat())
        }
    }
}

pub fn task_content_hash(raw_line: &str) -> String {
    let stripped = strip_line_ending(raw_line);
    blake3::hash(stripped.as_bytes()).to_hex().to_string()
}

pub fn add_task(content: &str, description: &str, opts: &AddTaskOptions) -> String {
    let mut task = format!(
        "- [{}] {description}",
        normalize_status_char(opts.status_char.unwrap_or(' '))
    );

    let mut keys: Vec<&String> = opts.fields.keys().collect();
    keys.sort();
    for key in keys {
        append_inline_field(&mut task, key, opts.fields.get(key).map(String::as_str));
    }
    task.push('\n');

    let separator = if content.ends_with('\n') || content.is_empty() {
        ""
    } else {
        "\n"
    };
    format!("{content}{separator}{task}")
}

fn append_inline_field(task: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        task.push_str(&format!(" [{key}:: {value}]"));
    }
}

fn normalize_status_char(status_char: char) -> char {
    if status_char == 'X' { 'x' } else { status_char }
}

fn task_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<indent>\s*)- \[(?P<marker>.)\] (?P<content>.*)$")
            .expect("valid task line regex")
    })
}

fn strip_line_ending(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn split_preserving_endings(content: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (idx, ch) in content.char_indices() {
        if ch == '\n' {
            out.push(&content[start..idx + 1]);
            start = idx + 1;
        }
    }
    if start < content.len() {
        out.push(&content[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_hash(line: &str) -> String {
        task_content_hash(line)
    }

    #[test]
    fn toggle_rewrites_the_matching_task_line() {
        let line = "- [ ] Fix the bug";
        let content = format!("{line}\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, '/').unwrap();
        assert_eq!(result, "- [/] Fix the bug\n");
    }

    #[test]
    fn toggle_preserves_other_lines() {
        let line = "- [ ] Task A";
        let content = format!("Some intro text\n{line}\n- [/] Task B\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, 'x').unwrap();
        assert_eq!(result, "Some intro text\n- [x] Task A\n- [/] Task B\n");
    }

    #[test]
    fn toggle_supports_custom_status_chars() {
        let line = "- [ ] Review task";
        let content = format!("{line}\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, '!').unwrap();
        assert_eq!(result, "- [!] Review task\n");
    }

    #[test]
    fn toggle_returns_not_found_when_hash_missing() {
        let content = "- [ ] Some task\n";
        let err = toggle_task(content, "deadbeef", '/').unwrap_err();
        assert!(matches!(err, ToggleError::TaskNotFound { .. }));
    }

    #[test]
    fn toggle_returns_collision_for_duplicate_lines() {
        let line = "- [ ] Duplicate task";
        let content = format!("{line}\n{line}\n");
        let hash = line_hash(line);

        let err = toggle_task(&content, &hash, 'x').unwrap_err();
        assert!(matches!(err, ToggleError::HashCollision { count: 2, .. }));
    }

    #[test]
    fn toggle_works_without_trailing_newline() {
        let line = "- [ ] No trailing newline";
        let hash = line_hash(line);

        let result = toggle_task(line, &hash, 'x').unwrap();
        assert_eq!(result, "- [x] No trailing newline");
    }

    #[test]
    fn toggle_preserves_crlf_endings() {
        let line = "- [ ] Windows line ending";
        let content = format!("{line}\r\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, '/').unwrap();
        assert_eq!(result, "- [/] Windows line ending\r\n");
    }

    #[test]
    fn add_task_appends_simple_todo() {
        let result = add_task("", "Fix the bug", &AddTaskOptions::default());
        assert_eq!(result, "- [ ] Fix the bug\n");
    }

    #[test]
    fn add_task_appends_after_existing_content_with_newline() {
        let content = "# Heading\n\nSome text\n";
        let result = add_task(content, "New task", &AddTaskOptions::default());
        assert_eq!(result, "# Heading\n\nSome text\n- [ ] New task\n");
    }

    #[test]
    fn add_task_inserts_separator_when_no_trailing_newline() {
        let content = "Some text";
        let result = add_task(content, "New task", &AddTaskOptions::default());
        assert_eq!(result, "Some text\n- [ ] New task\n");
    }

    #[test]
    fn add_task_writes_generic_inline_fields() {
        let opts = AddTaskOptions {
            fields: HashMap::from([
                ("customer".to_string(), "Acme".to_string()),
                ("stream".to_string(), "Migration to v2".to_string()),
                ("due".to_string(), "2025-03-15".to_string()),
                ("priority".to_string(), "high".to_string()),
            ]),
            ..Default::default()
        };
        let result = add_task("", "Plan migration", &opts);
        assert_eq!(
            result,
            "- [ ] Plan migration [customer:: Acme] [due:: 2025-03-15] [priority:: high] [stream:: Migration to v2]\n"
        );
    }

    #[test]
    fn add_task_supports_custom_status_char() {
        let opts = AddTaskOptions {
            status_char: Some('!'),
            ..Default::default()
        };
        let result = add_task("", "Needs review", &opts);
        assert_eq!(result, "- [!] Needs review\n");
    }

    #[test]
    fn task_content_hash_strips_line_ending_before_hashing() {
        let with_newline = "- [ ] Task A\n";
        let without = "- [ ] Task A";
        assert_eq!(task_content_hash(with_newline), task_content_hash(without));
    }
}
