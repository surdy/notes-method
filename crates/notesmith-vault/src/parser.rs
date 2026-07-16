use crate::frontmatter::extract_frontmatter;
use notesmith_core::{
    Block, InlineField, Link, LinkType, Note, SourcePosition, StatusGroup, Task, TaskStatusMap,
    VaultName, VaultPath,
};
use regex::Regex;
use std::{collections::HashMap, ops::Range, sync::OnceLock};

#[derive(Debug, Default, Clone)]
struct TaskMetadata {
    inline_fields: HashMap<String, String>,
    cleaned_content: String,
}

pub fn parse_note(vault_name: &VaultName, path: &VaultPath, content: &str) -> Note {
    let (raw_frontmatter, body) = extract_frontmatter(content);
    let frontmatter = raw_frontmatter
        .as_deref()
        .and_then(|yaml| serde_yaml::from_str(yaml).ok());
    let mut links = parse_links(body);
    links.extend(parse_embeds(body));
    links.extend(parse_markdown_links(body));
    links.sort_by_key(|link| link.position.offset);

    Note {
        vault: vault_name.clone(),
        path: path.clone(),
        frontmatter,
        raw_frontmatter,
        body: body.to_string(),
        links,
        inline_fields: parse_inline_fields(body),
        tasks: parse_tasks(body),
        blocks: parse_blocks(body),
        hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
    }
}

pub(crate) fn find_code_block_ranges(body: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut active_fence: Option<(usize, char, usize)> = None;

    for segment in body.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + segment.len();
        let trimmed = trim_line_ending(segment).trim_start_matches([' ', '\t']);

        if let Some((fence_start, marker, len)) = active_fence {
            if is_fence_delimiter(trimmed, marker, len) {
                ranges.push(fence_start..line_end);
                active_fence = None;
            }
        } else if let Some((marker, len)) = parse_fence_start(trimmed) {
            active_fence = Some((line_start, marker, len));
        }

        offset = line_end;
    }

    if let Some((fence_start, _, _)) = active_fence {
        ranges.push(fence_start..body.len());
    }

    let mut inline_ranges = find_inline_code_ranges(body, &ranges);
    ranges.append(&mut inline_ranges);
    ranges.sort_by_key(|range| range.start);
    merge_ranges(&ranges)
}

fn parse_links(body: &str) -> Vec<Link> {
    let excluded = find_code_block_ranges(body);
    parse_links_excluding(body, &excluded)
}

fn parse_links_excluding(body: &str, excluded: &[Range<usize>]) -> Vec<Link> {
    let line_starts = line_starts(body);
    wikilink_regex()
        .captures_iter(body)
        .filter_map(|captures| {
            let full = captures.get(0)?;
            if overlaps_excluded(full.start(), full.end(), excluded) {
                return None;
            }
            if full.start() > 0 && body.as_bytes()[full.start() - 1] == b'!' {
                return None;
            }

            let inner = captures.get(1)?.as_str();
            let (target, display_text) = inner
                .split_once('|')
                .map_or((inner.trim(), None), |(target, alias)| {
                    (target.trim(), Some(alias.trim().to_string()))
                });
            let (link_type, parsed_target) = classify_wikilink(target);

            Some(Link {
                link_type,
                target: parsed_target,
                display_text,
                position: source_position(
                    body,
                    &line_starts,
                    full.start(),
                    full.end() - full.start(),
                ),
            })
        })
        .collect()
}

fn parse_embeds(body: &str) -> Vec<Link> {
    let excluded = find_code_block_ranges(body);
    let line_starts = line_starts(body);

    embed_regex()
        .captures_iter(body)
        .filter_map(|captures| {
            let full = captures.get(0)?;
            if overlaps_excluded(full.start(), full.end(), &excluded) {
                return None;
            }

            let inner = captures.get(1)?.as_str().trim();
            let (target, display_text) = inner
                .split_once('|')
                .map_or((inner.to_string(), None), |(target, alias)| {
                    (target.trim().to_string(), Some(alias.trim().to_string()))
                });

            Some(Link {
                link_type: LinkType::Embed,
                target,
                display_text,
                position: source_position(
                    body,
                    &line_starts,
                    full.start(),
                    full.end() - full.start(),
                ),
            })
        })
        .collect()
}

fn parse_inline_fields(body: &str) -> Vec<InlineField> {
    let excluded = find_code_block_ranges(body);
    let line_starts = line_starts(body);
    let mut fields = Vec::new();
    let mut index = 0;

    while index < body.len() {
        if let Some(range) = excluded
            .iter()
            .find(|range| range.start <= index && index < range.end)
        {
            index = range.end;
            continue;
        }

        let current = body[index..].chars().next().unwrap_or_default();
        if current == '['
            && !body[index..].starts_with("[[")
            && !(index > 0 && body.as_bytes()[index - 1] == b'!')
        {
            if let Some((end, key, value)) = parse_inline_field_at(body, index) {
                fields.push(InlineField {
                    key,
                    value,
                    position: source_position(body, &line_starts, index, end - index),
                });
                index = end;
                continue;
            }
        }

        index += current.len_utf8().max(1);
    }

    fields
}

fn parse_tasks(body: &str) -> Vec<Task> {
    let excluded = find_code_block_ranges(body);
    let line_starts = line_starts(body);
    let mut tasks = Vec::new();
    let mut offset = 0;
    let status_map = TaskStatusMap::default();

    for segment in body.split_inclusive('\n') {
        let line_end = offset + segment.len();
        if overlaps_excluded(offset, line_end, &excluded) {
            offset = line_end;
            continue;
        }

        let line = trim_line_ending(segment);
        if let Some(captures) = task_regex().captures(line) {
            // Capture group 0 (the whole match) is guaranteed to be present
            // whenever `captures()` returns `Some` — a regex-crate invariant,
            // not a value derived from (untrusted) note content.
            let full = captures
                .get(0)
                .expect("capture group 0 always exists on a match");
            let status_char = captures
                .name("marker")
                .and_then(|marker| marker.as_str().chars().next())
                .unwrap_or(' ');
            let content = captures
                .name("content")
                .map(|content| content.as_str())
                .unwrap_or_default();
            let metadata = parse_task_metadata(content);

            tasks.push(Task {
                status_char,
                status_group: resolve_status_group(&status_map, status_char),
                content: metadata.cleaned_content,
                position: source_position(
                    body,
                    &line_starts,
                    offset + full.start(),
                    full.end() - full.start(),
                ),
                inline_fields: metadata.inline_fields,
                content_hash: Some(blake3::hash(line.as_bytes()).to_hex().to_string()),
            });
        }

        offset = line_end;
    }

    tasks
}

fn parse_task_metadata(content: &str) -> TaskMetadata {
    let mut inline_fields = HashMap::new();
    let mut cleaned = String::new();
    let mut index = 0;

    while index < content.len() {
        let current = content[index..].chars().next().unwrap_or_default();
        if current == '[' && !content[index..].starts_with("[[") {
            if let Some((end, key, value)) = parse_inline_field_at(content, index) {
                inline_fields.insert(key, value);
                if !cleaned.ends_with(' ') && !cleaned.is_empty() {
                    cleaned.push(' ');
                }
                index = end;
                continue;
            }
        }

        cleaned.push(current);
        index += current.len_utf8().max(1);
    }

    TaskMetadata {
        inline_fields,
        cleaned_content: normalize_whitespace(&cleaned),
    }
}

fn resolve_status_group(statuses: &TaskStatusMap, status_char: char) -> StatusGroup {
    if statuses.statuses.contains_key(&status_char) {
        return statuses.resolve_group(status_char);
    }

    let normalized = status_char.to_ascii_lowercase();
    statuses.resolve_group(normalized)
}

fn parse_blocks(body: &str) -> Vec<Block> {
    let excluded = find_code_block_ranges(body);
    let line_starts = line_starts(body);
    let mut blocks = Vec::new();
    let mut offset = 0;

    for segment in body.split_inclusive('\n') {
        let line_end = offset + segment.len();
        if overlaps_excluded(offset, line_end, &excluded) {
            offset = line_end;
            continue;
        }

        let line = trim_line_ending(segment);
        if line.trim().is_empty() {
            offset = line_end;
            continue;
        }

        let (block_id, content) = block_id_regex()
            .captures(line)
            .and_then(|captures| {
                let full = captures.get(0)?;
                let block_id = captures.get(1)?.as_str().to_string();
                let content = line[..full.start()].trim_end().to_string();
                Some((Some(block_id), content))
            })
            .unwrap_or_else(|| (None, line.to_string()));

        blocks.push(Block {
            content,
            block_id,
            position: source_position(body, &line_starts, offset, line.len()),
        });

        offset = line_end;
    }

    blocks
}

fn parse_markdown_links(body: &str) -> Vec<Link> {
    let excluded = find_code_block_ranges(body);
    let line_starts = line_starts(body);

    markdown_link_regex()
        .captures_iter(body)
        .filter_map(|captures| {
            let full = captures.get(0)?;
            if overlaps_excluded(full.start(), full.end(), &excluded) {
                return None;
            }
            if full.start() > 0 && body.as_bytes()[full.start() - 1] == b'!' {
                return None;
            }

            let text = captures.name("text")?.as_str().to_string();
            let target = captures.name("target")?.as_str().to_string();
            let link_type = if target.starts_with("http://") || target.starts_with("https://") {
                LinkType::ExternalLink
            } else {
                LinkType::MarkdownLink
            };

            Some(Link {
                link_type,
                target,
                display_text: Some(text),
                position: source_position(
                    body,
                    &line_starts,
                    full.start(),
                    full.end() - full.start(),
                ),
            })
        })
        .collect()
}

fn parse_inline_field_at(body: &str, start: usize) -> Option<(usize, String, String)> {
    let mut index = start + 1;
    let mut wikilink_depth = 0usize;

    while index < body.len() {
        if body[index..].starts_with("[[") {
            wikilink_depth += 1;
            index += 2;
            continue;
        }
        if wikilink_depth > 0 && body[index..].starts_with("]]") {
            wikilink_depth -= 1;
            index += 2;
            continue;
        }

        let ch = body[index..].chars().next()?;
        if ch == '\n' {
            return None;
        }
        if ch == ']' && wikilink_depth == 0 {
            let inner = &body[start + 1..index];
            let (key, value) = inner.split_once("::")?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            return Some((index + 1, key.to_string(), value.trim().to_string()));
        }

        index += ch.len_utf8();
    }

    None
}

fn classify_wikilink(target: &str) -> (LinkType, String) {
    if let Some(anchor) = target.strip_prefix('#') {
        return (LinkType::Anchor, anchor.to_string());
    }
    if let Some((base, fragment)) = target.split_once('#') {
        if fragment.starts_with('^') {
            return (LinkType::BlockRef, base.trim().to_string());
        }
        return (LinkType::HeadingRef, base.trim().to_string());
    }
    (LinkType::WikiLink, target.to_string())
}

fn source_position(
    body: &str,
    line_starts: &[usize],
    offset: usize,
    length: usize,
) -> SourcePosition {
    let line_index = line_starts.partition_point(|line_start| *line_start <= offset) - 1;
    let line_start = line_starts[line_index];
    let column = body[line_start..offset].chars().count() + 1;
    SourcePosition::new(line_index + 1, column, offset, length)
}

fn line_starts(body: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        body.char_indices()
            .filter_map(|(index, ch)| (ch == '\n' && index + 1 < body.len()).then_some(index + 1)),
    );
    starts
}

fn parse_fence_start(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = line.chars().take_while(|ch| *ch == marker).count();
    (len >= 3).then_some((marker, len))
}

fn is_fence_delimiter(line: &str, marker: char, len: usize) -> bool {
    let run = line.chars().take_while(|ch| *ch == marker).count();
    run >= len && line[run..].trim().is_empty()
}

fn find_inline_code_ranges(body: &str, excluded: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut active: Option<(usize, usize)> = None;
    let mut index = 0;

    while index < body.len() {
        if let Some(range) = excluded
            .iter()
            .find(|range| range.start <= index && index < range.end)
        {
            index = range.end;
            continue;
        }

        if body.as_bytes()[index] == b'`' {
            let run = body[index..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if let Some((start, delimiter_len)) = active {
                if delimiter_len == run {
                    ranges.push(start..index + run);
                    active = None;
                }
            } else {
                active = Some((index, run));
            }
            index += run;
            continue;
        }

        index += body[index..].chars().next().map_or(1, char::len_utf8);
    }

    ranges
}

fn merge_ranges(ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range.clone());
    }
    merged
}

fn overlaps_excluded(start: usize, end: usize, excluded: &[Range<usize>]) -> bool {
    excluded
        .iter()
        .any(|range| start < range.end && end > range.start)
}

fn trim_line_ending(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn wikilink_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\[\[([^\]]+)\]\]").expect("valid wikilink regex"))
}

fn embed_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"!\[\[([^\]]+)\]\]").expect("valid embed regex"))
}

fn markdown_link_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"\[(?P<text>[^\]\n]+)\]\((?P<target>[^)\s]+)(?:\s+"[^"]*")?\)"#)
            .expect("valid markdown link regex")
    })
}

fn task_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^(?P<indent>\s*)- \[(?P<marker>.)\] (?P<content>.*)$")
            .expect("valid task regex")
    })
}

fn block_id_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\^([a-zA-Z0-9-]+)\s*$").expect("valid block id regex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_note_populates_note_metadata() {
        let vault = VaultName::new("test");
        let path = VaultPath::new("Inbox/note.md");
        let note = parse_note(&vault, &path, "# Hello");

        assert_eq!(note.vault, vault);
        assert_eq!(note.path, path);
        assert_eq!(
            note.hash,
            blake3::hash("# Hello".as_bytes()).to_hex().to_string()
        );
    }

    #[test]
    fn parse_wikilink_simple() {
        let links = parse_links("See [[Acme Corp]] for details");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Acme Corp");
        assert_eq!(links[0].link_type, LinkType::WikiLink);
        assert!(links[0].display_text.is_none());
    }

    #[test]
    fn parse_wikilink_with_alias() {
        let links = parse_links("See [[Acme Corp|Acme]] for details");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Acme Corp");
        assert_eq!(links[0].display_text, Some("Acme".to_string()));
    }

    #[test]
    fn parse_wikilink_with_heading() {
        let links = parse_links("See [[Note#Section]] for details");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Note");
        assert_eq!(links[0].link_type, LinkType::HeadingRef);
    }

    #[test]
    fn parse_wikilink_with_block_ref() {
        let links = parse_links("See [[Note#^abc123]] for details");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Note");
        assert_eq!(links[0].link_type, LinkType::BlockRef);
    }

    #[test]
    fn parse_wikilink_same_doc_anchor() {
        let links = parse_links("See [[#Heading]] above");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, LinkType::Anchor);
    }

    #[test]
    fn parse_embed() {
        let links = parse_embeds("![[image.png]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, LinkType::Embed);
        assert_eq!(links[0].target, "image.png");
    }

    #[test]
    fn parse_inline_field_basic() {
        let fields = parse_inline_fields("Some text [customer:: [[Acme Corp]]] more text");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "customer");
        assert_eq!(fields[0].value, "[[Acme Corp]]");
    }

    #[test]
    fn parse_task_all_statuses() {
        let body = "- [ ] Todo\n- [/] In progress\n- [b] Blocked\n- [w] Waiting\n- [h] On hold\n- [x] Done\n- [-] Cancelled";
        let tasks = parse_tasks(body);
        assert_eq!(tasks.len(), 7);
        assert_eq!(tasks[0].status_char, ' ');
        assert_eq!(tasks[1].status_char, '/');
        assert_eq!(tasks[2].status_char, 'b');
        assert_eq!(tasks[3].status_char, 'w');
        assert_eq!(tasks[4].status_char, 'h');
        assert_eq!(tasks[5].status_char, 'x');
        assert_eq!(tasks[6].status_char, '-');
        assert_eq!(tasks[0].status_group, StatusGroup::Open);
        assert_eq!(tasks[5].status_group, StatusGroup::Done);
        assert_eq!(tasks[6].status_group, StatusGroup::Done);
    }

    #[test]
    fn parse_task_extracts_inline_fields() {
        let body = "- [ ] Plan rollout [customer:: [[Acme Corp]]] [due:: 2025-03-15]";
        let tasks = parse_tasks(body);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].content, "Plan rollout");
        assert_eq!(
            tasks[0].inline_fields.get("customer"),
            Some(&"[[Acme Corp]]".to_string())
        );
        assert_eq!(
            tasks[0].inline_fields.get("due"),
            Some(&"2025-03-15".to_string())
        );
    }

    #[test]
    fn parse_task_keeps_non_field_metadata_in_content() {
        let body = "- [ ] Something 📅 2025-03-15 🔼";
        let tasks = parse_tasks(body);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].content, "Something 📅 2025-03-15 🔼");
    }

    #[test]
    fn parse_block_reference() {
        let body = "Some paragraph text ^abc123\n\nAnother paragraph";
        let blocks = parse_blocks(body);
        let with_id: Vec<_> = blocks.iter().filter(|b| b.block_id.is_some()).collect();
        assert_eq!(with_id.len(), 1);
        assert_eq!(with_id[0].block_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn parse_external_link() {
        let links =
            parse_markdown_links("Check [Google](https://google.com) and [Docs](https://docs.rs)");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "https://google.com");
        assert_eq!(links[0].display_text, Some("Google".to_string()));
        assert_eq!(links[0].link_type, LinkType::ExternalLink);
    }

    #[test]
    fn code_block_exclusion() {
        let body = "[[real link]]\n```\n[[fake link]]\n```\n[[another real]]";
        let excluded = find_code_block_ranges(body);
        assert!(!excluded.is_empty());
        let links = parse_links_excluding(body, &excluded);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert!(targets.contains(&"real link"));
        assert!(targets.contains(&"another real"));
        assert!(!targets.contains(&"fake link"));
    }
}
