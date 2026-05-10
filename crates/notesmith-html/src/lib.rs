//! notesmith-html: Comrak-based HTML rendering with OFM extensions

use comrak::{Options, markdown_to_html};
use regex::Regex;

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
    margin: 1em 0;
    padding: 0.9em 1em;
    border: 1px solid #bfdbfe;
    border-radius: 8px;
    background: #eff6ff;
}
.callout-title {
    margin-bottom: 0.35em;
    font-weight: 600;
    color: #1d4ed8;
}
.callout-body > :first-child {
    margin-top: 0;
}
.callout-body > :last-child {
    margin-bottom: 0;
}
"#;

/// Render markdown content to HTML.
pub fn render_to_html(markdown: &str) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.render.unsafe_ = true;

    let html = markdown_to_html(markdown, &options);
    let html = convert_wikilinks(&html);
    convert_callouts(&html)
}

/// Render markdown content to a complete HTML document with embedded styles.
pub fn render_to_html_with_inline_styles(markdown: &str) -> String {
    let html = render_to_html(strip_frontmatter(markdown));
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
    let re = Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]").expect("valid wikilink regex");
    re.replace_all(html, |caps: &regex::Captures<'_>| {
        let target = &caps[1];
        let display = caps.get(2).map(|m| m.as_str()).unwrap_or(target);
        format!(r#"<a class="wikilink" data-target="{target}">{display}</a>"#)
    })
    .to_string()
}

fn convert_styled_wikilinks(html: &str) -> String {
    let re = Regex::new(r#"<a class="wikilink" data-target="([^"]+)">"#)
        .expect("valid styled wikilink regex");
    re.replace_all(html, r#"<a href="$1">"#).to_string()
}

fn convert_callouts(html: &str) -> String {
    let re = Regex::new(r"(?s)<blockquote>\s*<p>\[!(\w+)\]\s*(.*?)</p>\s*</blockquote>")
        .expect("valid callout regex");
    re.replace_all(html, |caps: &regex::Captures<'_>| {
        let callout_type = caps[1].to_lowercase();
        let content = &caps[2];
        let (title, body) = content.split_once('\n').unwrap_or((content, ""));
        let title = title.trim();
        let body = body.trim();
        let body_html = if body.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="callout-body">{body}</div>"#)
        };
        format!(
            r#"<div class="callout callout-{callout_type}"><div class="callout-title">{title}</div>{body_html}</div>"#
        )
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{render_to_html, render_to_html_with_inline_styles, strip_frontmatter};

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
            html.contains(r#"<div class="callout callout-info">"#),
            "html was: {html}"
        );
        assert!(
            html.contains(r#"<div class="callout-title">Title</div>"#),
            "html was: {html}"
        );
        assert!(
            html.contains(r#"<div class="callout-body">body</div>"#),
            "html was: {html}"
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
