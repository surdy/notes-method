//! notesmith-html: Comrak-based HTML rendering with OFM extensions

use comrak::{Options, markdown_to_html};
use regex::Regex;

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

fn convert_wikilinks(html: &str) -> String {
    let re = Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]").expect("valid wikilink regex");
    re.replace_all(html, |caps: &regex::Captures<'_>| {
        let target = &caps[1];
        let display = caps.get(2).map(|m| m.as_str()).unwrap_or(target);
        format!(r#"<a class="wikilink" data-target="{target}">{display}</a>"#)
    })
    .to_string()
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
    use super::render_to_html;

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
}
