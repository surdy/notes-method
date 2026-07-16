//! notesmith-html: Comrak-based HTML rendering with OFM extensions

use comrak::{Options, markdown_to_html};
use regex::Regex;
use std::sync::LazyLock;

// Regexes used by the per-render OFM post-processing passes. Compiled once at
// module init (the ADR 0009-sanctioned form) rather than on every render — the
// patterns are compile-time literals, so `expect` here can never fire on
// (untrusted) note content.
static WIKILINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]").expect("valid wikilink regex")
});
static STYLED_WIKILINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a class="wikilink" data-target="([^"]+)">"#)
        .expect("valid styled wikilink regex")
});
static COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)(```[^\n]*\n.*?```)|(`[^`\n]+`)|%%.*?%%").expect("valid comment-aware regex")
});
static HIGHLIGHT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"==([^=]+)==").expect("valid highlight regex"));
static CODE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)(<code>.*?</code>|<pre>.*?</pre>)").expect("valid code tag regex")
});
static EMBED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[\[([^\]]+?)\]\]").expect("valid embed regex"));
static EXTENDED_TASK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<li>\[([^\]\s])\]\s*").expect("valid extended task regex"));
static CALLOUT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^<blockquote>\s*<p>\[!([\w-]+)\]([+-])? ?([^\n]*)(.*?)</p>(.*?)</blockquote>$")
        .expect("valid callout regex")
});

const INLINE_STYLE_SHEET: &str = r#"html {
    background: #ffffff;
}
body {
    margin: 0;
    padding: 24px;
    color: #1f2937;
    background: #ffffff;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 16px;
    line-height: 1.6;
}
h1, h2, h3, h4, h5, h6 {
    margin: 1.4em 0 0.6em;
    color: #111827;
    line-height: 1.25;
}
h1 { font-size: 2em; }
h2 { font-size: 1.6em; }
h3 { font-size: 1.35em; }
h4 { font-size: 1.15em; }
h5, h6 { font-size: 1em; }
p, ul, ol, pre, table, blockquote {
    margin: 0 0 1em;
}
ul, ol {
    padding-left: 1.5em;
}
li {
    margin: 0.25em 0;
}
a {
    color: #2563eb;
    text-decoration: none;
}
a:hover {
    text-decoration: underline;
}
a[href^="http://"]:not(.wikilink)::after,
a[href^="https://"]:not(.wikilink)::after,
a[href^="//"]:not(.wikilink)::after,
a[href^="mailto:"]:not(.wikilink)::after,
a[href^="tel:"]:not(.wikilink)::after,
a[href^="ftp://"]:not(.wikilink)::after,
a[href^="obsidian://"]:not(.wikilink)::after,
a[href^="notesmith://"]:not(.wikilink)::after {
    content: "↗";
    display: inline-block;
    margin-left: 0.15em;
    font-size: 0.85em;
    vertical-align: baseline;
    opacity: 0.7;
}
strong {
    font-weight: 600;
}
em {
    font-style: italic;
}
blockquote {
    padding: 0.75em 1em;
    border-left: 4px solid #cbd5e1;
    background: #f8fafc;
    color: #475569;
}
pre {
    padding: 1em;
    border: 1px solid #d0d7de;
    border-radius: 8px;
    background: #f6f8fa;
    overflow-x: auto;
}
code {
    padding: 0.15em 0.35em;
    border-radius: 4px;
    background: #f3f4f6;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.9em;
}
pre code {
    padding: 0;
    background: transparent;
}
table {
    width: 100%;
    border-collapse: collapse;
}
th, td {
    padding: 0.5em 0.75em;
    border: 1px solid #d0d7de;
    text-align: left;
    vertical-align: top;
}
th {
    background: #f8fafc;
}
input[type="checkbox"] {
    margin-right: 0.5em;
}
.callout {
    --callout-color: #448aff;
    margin: 1em 0;
    padding: 0.9em 1em;
    border: 1px solid color-mix(in srgb, var(--callout-color) 42%, transparent);
    border-left: 4px solid var(--callout-color);
    border-radius: 8px;
    background: color-mix(in srgb, var(--callout-color) 13%, white);
}
.callout-title {
    margin-bottom: 0.35em;
    font-weight: 600;
    color: var(--callout-color);
}
.callout-title::before {
    content: var(--callout-icon, "✎");
    display: inline-block;
    margin-right: 0.45em;
}
.callout-body > :first-child {
    margin-top: 0;
}
.callout-body > :last-child {
    margin-bottom: 0;
}
.callout[data-fold="closed"] .callout-body {
    display: none;
}
.callout-note { --callout-color: #448aff; --callout-icon: "✎"; }
.callout-abstract { --callout-color: #00b0ff; --callout-icon: "☷"; }
.callout-info { --callout-color: #00b8d4; --callout-icon: "ⓘ"; }
.callout-todo { --callout-color: #00b8d4; --callout-icon: "☑"; }
.callout-tip { --callout-color: #00bfa5; --callout-icon: "🔥"; }
.callout-success { --callout-color: #00c853; --callout-icon: "✓"; }
.callout-question { --callout-color: #64dd17; --callout-icon: "?"; }
.callout-warning { --callout-color: #ff9100; --callout-icon: "⚠"; }
.callout-failure { --callout-color: #ff5252; --callout-icon: "✕"; }
.callout-danger { --callout-color: #ff1744; --callout-icon: "⚡"; }
.callout-bug { --callout-color: #f50057; --callout-icon: "◉"; }
.callout-example { --callout-color: #7c4dff; --callout-icon: "▦"; }
.callout-quote { --callout-color: #9e9e9e; --callout-icon: "❝"; }
"#;

/// Render markdown content to HTML.
pub fn render_to_html(markdown: &str) -> String {
    render_to_html_opts(markdown, true)
}

/// Render markdown content to HTML with configurable hardbreak handling.
pub fn render_to_html_opts(markdown: &str, hardbreaks: bool) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.render.unsafe_ = true;
    options.render.hardbreaks = hardbreaks;

    let preprocessed = strip_comments(markdown);
    let html = markdown_to_html(&preprocessed, &options);
    let html = convert_embeds(&html);
    let html = convert_wikilinks(&html);
    let html = convert_highlights(&html);
    let html = convert_extended_tasks(&html);
    convert_callouts(&html)
}

/// Render markdown content to a complete HTML document with embedded styles.
pub fn render_to_html_with_inline_styles(markdown: &str) -> String {
    render_to_html_with_inline_styles_opts(markdown, true)
}

/// Render markdown content to a complete HTML document with embedded styles and configurable
/// hardbreak handling.
pub fn render_to_html_with_inline_styles_opts(markdown: &str, hardbreaks: bool) -> String {
    let html = render_to_html_opts(strip_frontmatter(markdown), hardbreaks);
    let html = convert_styled_wikilinks(&html);
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>{INLINE_STYLE_SHEET}</style>
</head>
<body>{html}</body>
</html>"#
    )
}

/// Strip leading YAML frontmatter from markdown content.
pub fn strip_frontmatter(content: &str) -> &str {
    let mut lines = content.split_inclusive('\n');
    let Some(first_line) = lines.next() else {
        return content;
    };

    if first_line.trim_end_matches(['\r', '\n']) != "---" {
        return content;
    }

    let mut offset = first_line.len();
    for line in lines {
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return content[offset..].trim_start_matches(['\r', '\n']);
        }
    }

    content
}

fn convert_wikilinks(html: &str) -> String {
    WIKILINK_RE
        .replace_all(html, |caps: &regex::Captures<'_>| {
            let target = &caps[1];
            let display = caps.get(2).map(|m| m.as_str()).unwrap_or(target);
            format!(r#"<a class="wikilink" data-target="{target}">{display}</a>"#)
        })
        .to_string()
}

fn convert_styled_wikilinks(html: &str) -> String {
    STYLED_WIKILINK_RE
        .replace_all(html, r#"<a href="$1">"#)
        .to_string()
}

/// Strip `%%...%%` comments from markdown before rendering.
/// Handles both inline (`%%text%%`) and block comments.
/// Respects fenced code blocks and inline code — comments inside code are preserved.
fn strip_comments(markdown: &str) -> String {
    // Regex that matches (in priority order):
    // 1. Fenced code blocks (``` ... ```) — captured in group 1 to preserve
    // 2. Inline code (`...`) — captured in group 2 to preserve
    // 3. %%...%% comments — matched without a group to strip
    let re = &COMMENT_RE;

    re.replace_all(markdown, |caps: &regex::Captures| {
        // If group 1 (fenced code) or group 2 (inline code) matched, preserve it
        if caps.get(1).is_some() || caps.get(2).is_some() {
            caps[0].to_string()
        } else {
            // It's a %%...%% comment — strip it
            String::new()
        }
    })
    .to_string()
}

/// Convert `==text==` highlights to `<mark>text</mark>`.
/// Skips content inside `<code>` and `<pre>` elements.
fn convert_highlights(html: &str) -> String {
    let highlight_re = &HIGHLIGHT_RE;
    // Split by code/pre tags to avoid converting highlights inside them
    let tag_re = &CODE_TAG_RE;

    let mut result = String::with_capacity(html.len());
    let mut last_end = 0;

    for m in tag_re.find_iter(html) {
        // Process text before this code/pre block
        let before = &html[last_end..m.start()];
        result.push_str(&highlight_re.replace_all(before, r#"<mark>$1</mark>"#));
        // Preserve the code/pre block as-is
        result.push_str(m.as_str());
        last_end = m.end();
    }

    // Process remaining text after last code/pre block
    let remaining = &html[last_end..];
    result.push_str(&highlight_re.replace_all(remaining, r#"<mark>$1</mark>"#));

    result
}

/// Convert `![[target]]` embeds to appropriate HTML elements.
/// Image embeds produce `<img>`, note embeds produce a placeholder div.
fn convert_embeds(html: &str) -> String {
    let re = &EMBED_RE;
    re.replace_all(html, |caps: &regex::Captures<'_>| {
        let target = &caps[1];
        let lower = target.to_lowercase();
        if lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".gif")
            || lower.ends_with(".svg")
            || lower.ends_with(".webp")
            || lower.ends_with(".bmp")
        {
            format!(r#"<img src="{target}" alt="{target}" class="embed-image">"#)
        } else {
            let display = target.split('#').next().unwrap_or(target);
            format!(
                r#"<div class="embed" data-target="{target}"><a class="embed-link" href="{target}">{display}</a></div>"#
            )
        }
    })
    .to_string()
}

/// Convert extended task markers that comrak doesn't handle.
/// Obsidian treats any character in `- [x]` brackets as a completed task
/// (e.g., `[?]`, `[!]`, `[b]`, `[-]`, `[/]`).
/// comrak only handles `[ ]` and `[x]`/`[X]`, so we post-process the HTML
/// for extended markers that comrak rendered as plain list items.
fn convert_extended_tasks(html: &str) -> String {
    let re = &EXTENDED_TASK_RE;
    re.replace_all(html, |caps: &regex::Captures<'_>| {
        let marker = &caps[1];
        format!(
            r#"<li class="task-list-item"><input type="checkbox" checked="" disabled="" data-task="{marker}"> "#
        )
    })
    .to_string()
}

fn canonical_callout_type(identifier: &str) -> &'static str {
    match identifier {
        "note" => "note",
        "abstract" | "summary" | "tldr" => "abstract",
        "info" => "info",
        "todo" => "todo",
        "tip" | "hint" | "important" => "tip",
        "success" | "check" | "done" => "success",
        "question" | "help" | "faq" => "question",
        "warning" | "caution" | "attention" => "warning",
        "failure" | "fail" | "missing" => "failure",
        "danger" | "error" => "danger",
        "bug" => "bug",
        "example" => "example",
        "quote" | "cite" => "quote",
        _ => "note",
    }
}

fn convert_callouts(html: &str) -> String {
    let mut converted = html.to_string();
    while let Some(next) = convert_one_innermost_callout(&converted) {
        converted = next;
    }
    converted
}

fn convert_one_innermost_callout(html: &str) -> Option<String> {
    let mut search_end = html.len();
    while let Some(start) = html[..search_end].rfind("<blockquote>") {
        let after_start = &html[start..];
        let Some(close_start) = after_start.find("</blockquote>") else {
            search_end = start;
            continue;
        };
        let end = start + close_start + "</blockquote>".len();
        let candidate = &html[start..end];
        if let Some(replacement) = convert_callout_block(candidate) {
            return Some(format!("{}{}{}", &html[..start], replacement, &html[end..]));
        }
        search_end = start;
    }
    None
}

fn convert_callout_block(html: &str) -> Option<String> {
    let re = &CALLOUT_RE;
    let caps = re.captures(html)?;
    Some({
        let callout_identifier = caps[1].to_lowercase();
        let callout_type = canonical_callout_type(&callout_identifier);
        let fold_marker = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let mut title_text = caps[3].trim();
        let mut first_body = caps[4].trim().to_string();
        if let Some((before_break, after_break)) = split_once_break_tag(title_text) {
            title_text = before_break.trim();
            let after_break = after_break.trim();
            if !after_break.is_empty() {
                first_body = if first_body.is_empty() {
                    after_break.to_string()
                } else {
                    format!("{after_break}\n{first_body}")
                };
            }
        }
        title_text = trim_trailing_break_tags(title_text);
        let first_body = trim_leading_break_tags(&first_body);
        let rest = caps.get(5).map(|m| m.as_str().trim()).unwrap_or("");

        let title = if title_text.is_empty() {
            capitalize(&callout_identifier)
        } else {
            title_text.to_string()
        };

        let fold_attr = match fold_marker {
            "+" => r#" data-callout-fold="+" data-fold="open""#,
            "-" => r#" data-callout-fold="-" data-fold="closed""#,
            _ => "",
        };

        // Combine body from first paragraph remainder and subsequent blockquote content
        let mut body_parts = Vec::new();
        if !first_body.is_empty() {
            body_parts.push(first_body.to_string());
        }
        if !rest.is_empty() {
            body_parts.push(rest.to_string());
        }
        let body = body_parts.join("\n");

        let body_html = if body.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="callout-body">{body}</div>"#)
        };

        format!(
            r#"<div class="callout callout-{callout_type}" data-callout="{callout_identifier}"{fold_attr}><div class="callout-title">{title}</div>{body_html}</div>"#
        )
    })
}

fn split_once_break_tag(s: &str) -> Option<(&str, &str)> {
    s.split_once("<br />").or_else(|| s.split_once("<br>"))
}

fn trim_leading_break_tags(s: &str) -> &str {
    let mut trimmed = s.trim();
    loop {
        if let Some(rest) = trimmed.strip_prefix("<br />") {
            trimmed = rest.trim_start();
        } else if let Some(rest) = trimmed.strip_prefix("<br>") {
            trimmed = rest.trim_start();
        } else {
            return trimmed;
        }
    }
}

fn trim_trailing_break_tags(s: &str) -> &str {
    let mut trimmed = s.trim();
    loop {
        if let Some(rest) = trimmed.strip_suffix("<br />") {
            trimmed = rest.trim_end();
        } else if let Some(rest) = trimmed.strip_suffix("<br>") {
            trimmed = rest.trim_end();
        } else {
            return trimmed;
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        render_to_html, render_to_html_opts, render_to_html_with_inline_styles, strip_frontmatter,
    };

    #[test]
    fn renders_basic_markdown() {
        let html = render_to_html("# Heading\n\nParagraph with **bold** and *italic*.");

        assert!(html.contains("<h1>Heading</h1>"), "html was: {html}");
        assert!(
            html.contains("<p>Paragraph with <strong>bold</strong> and <em>italic</em>.</p>"),
            "html was: {html}"
        );
    }

    #[test]
    fn renders_wikilinks() {
        let html = render_to_html("[[target]] and [[target|alias]]");

        assert!(
            html.contains(r#"<a class="wikilink" data-target="target">target</a>"#),
            "html was: {html}"
        );
        assert!(
            html.contains(r#"<a class="wikilink" data-target="target">alias</a>"#),
            "html was: {html}"
        );
    }

    #[test]
    fn renders_tables() {
        let html = render_to_html("| Name | Value |\n| --- | --- |\n| One | 1 |");

        assert!(html.contains("<table>"), "html was: {html}");
        assert!(html.contains("<thead>"), "html was: {html}");
        assert!(html.contains("<td>One</td>"), "html was: {html}");
    }

    #[test]
    fn hardbreaks_converts_single_newline_to_br() {
        let html = render_to_html("Line one\nLine two");
        assert!(html.contains("<br"), "expected <br> in: {html}");
    }

    #[test]
    fn no_hardbreaks_keeps_soft_break() {
        let html = render_to_html_opts("Line one\nLine two", false);
        assert!(!html.contains("<br"), "expected no <br> in: {html}");
    }

    #[test]
    fn renders_task_lists() {
        let html = render_to_html("- [ ] todo\n- [x] done");

        assert!(
            html.contains(r#"type="checkbox""#),
            "expected checkbox input in html: {html}"
        );
        assert!(
            html.contains(r#"checked=""#) || html.contains("checked>"),
            "expected checked checkbox in html: {html}"
        );
    }

    #[test]
    fn renders_callouts() {
        let html = render_to_html("> [!info] Title\n> body");

        assert!(
            html.contains(r#"<div class="callout callout-info" data-callout="info">"#),
            "html was: {html}"
        );
        assert!(
            html.contains(r#"<div class="callout-title">Title</div>"#),
            "html was: {html}"
        );
        assert!(html.contains("callout-body"), "html was: {html}");
        assert!(html.contains("body"), "html was: {html}");
    }

    #[test]
    fn renders_callout_without_title() {
        let html = render_to_html("> [!note]\n> Some content here");
        assert!(
            html.contains(r#"<div class="callout-title">Note</div>"#),
            "expected default title: {html}"
        );
    }

    #[test]
    fn renders_foldable_callout_collapsed() {
        let html = render_to_html("> [!faq]- Are callouts foldable?\n> Yes!");
        assert!(
            html.contains(r#"data-fold="closed""#),
            "expected fold=closed: {html}"
        );
    }

    #[test]
    fn renders_foldable_callout_expanded() {
        let html = render_to_html("> [!tip]+ Expanded\n> Content");
        assert!(
            html.contains(r#"data-fold="open""#),
            "expected fold=open: {html}"
        );
    }

    #[test]
    fn renders_highlights() {
        let html = render_to_html("Use ==highlight text== here.");
        assert!(
            html.contains("<mark>highlight text</mark>"),
            "html was: {html}"
        );
    }

    #[test]
    fn strips_inline_comments() {
        let html = render_to_html("visible %%hidden%% text");
        assert!(!html.contains("hidden"), "comment not stripped: {html}");
        assert!(html.contains("visible"), "visible text missing: {html}");
        assert!(html.contains("text"), "text missing: {html}");
    }

    #[test]
    fn strips_block_comments() {
        let html = render_to_html("before\n%%\nblock comment\n%%\nafter");
        assert!(
            !html.contains("block comment"),
            "block comment not stripped: {html}"
        );
        assert!(html.contains("before"), "html was: {html}");
        assert!(html.contains("after"), "html was: {html}");
    }

    #[test]
    fn renders_note_embeds() {
        let html = render_to_html("![[My Note]]");
        assert!(
            html.contains(r#"class="embed""#),
            "embed not rendered: {html}"
        );
        assert!(
            html.contains(r#"data-target="My Note""#),
            "embed target missing: {html}"
        );
    }

    #[test]
    fn renders_image_embeds() {
        let html = render_to_html("![[photo.png]]");
        assert!(
            html.contains(r#"<img src="photo.png""#),
            "image embed not rendered: {html}"
        );
    }

    #[test]
    fn renders_section_embeds() {
        let html = render_to_html("![[Note#Section]]");
        assert!(
            html.contains(r#"data-target="Note#Section""#),
            "section embed missing: {html}"
        );
        assert!(
            html.contains(">Note</a>"),
            "display should show note name without section: {html}"
        );
    }

    #[test]
    fn renders_extended_task_markers() {
        let html = render_to_html("- [?] custom marker\n- [!] another\n- [-] cancelled");
        assert!(
            html.contains(r#"data-task="?""#),
            "? marker missing: {html}"
        );
        assert!(
            html.contains(r#"data-task="!""#),
            "! marker missing: {html}"
        );
        assert!(
            html.contains(r#"data-task="-""#),
            "- marker missing: {html}"
        );
        // All extended markers should be checked
        assert!(
            html.matches(r#"checked="""#).count() == 3,
            "all extended markers should be checked: {html}"
        );
    }

    #[test]
    fn render_to_html_with_inline_styles_wraps_document() {
        let html = render_to_html_with_inline_styles("# Heading");

        assert!(html.contains("<html"), "html was: {html}");
        assert!(html.contains("<style>"), "html was: {html}");
        assert!(html.contains("<body>"), "html was: {html}");
        assert!(html.contains("<h1>Heading</h1>"), "html was: {html}");
    }

    #[test]
    fn strip_frontmatter_removes_leading_yaml_block() {
        let body = strip_frontmatter("---\nstatus: draft\nowner: me\n---\n# Heading\n");

        assert_eq!(body, "# Heading\n");
    }

    #[test]
    fn render_to_html_with_inline_styles_strips_frontmatter_and_plainifies_wikilinks() {
        let html = render_to_html_with_inline_styles("---\nstatus: draft\n---\n[[Target|Alias]]");

        assert!(!html.contains("status: draft"), "html was: {html}");
        assert!(
            html.contains(r#"<a href="Target">Alias</a>"#),
            "html was: {html}"
        );
        assert!(!html.contains("class=\"wikilink\""), "html was: {html}");
    }
}
