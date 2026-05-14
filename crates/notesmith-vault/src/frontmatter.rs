pub fn extract_frontmatter(content: &str) -> (Option<String>, &str) {
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

#[cfg(test)]
mod tests {
    use super::extract_frontmatter;

    #[test]
    fn extract_frontmatter_basic() {
        let content = "---\ntype: note\ntags: [test]\n---\n# Hello\nBody text";
        let (fm, body) = extract_frontmatter(content);
        assert!(fm.is_some());
        assert!(body.contains("# Hello"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn extract_frontmatter_missing() {
        let content = "# No frontmatter\nJust body";
        let (fm, body) = extract_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }
}
