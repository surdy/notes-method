//! Per-domain clip templates ([ADR 0020](../../docs/adr/0020-web-clipper.md)).
//!
//! A template customizes the frontmatter and/or body produced for a clip based
//! on the source host. Templates are minijinja strings evaluated against the
//! extracted [`ClipDocument`]. Selection prefers the most specific matching host
//! suffix, falling back to a `*` (or empty) catch-all.
//!
//! Rendering is resilient: a template that fails to render (or produces invalid
//! YAML for a frontmatter value) degrades to the default behavior for that
//! piece rather than aborting the clip.

use std::collections::BTreeMap;

use minijinja::{Environment, context};
use serde_yaml::{Mapping, Value};

use crate::extract::ClipDocument;

/// A single per-domain clip template.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClipTemplate {
    /// Host suffix to match (e.g. `example.com`), or `*`/empty for the fallback.
    pub match_host: String,
    /// Extra frontmatter entries: key → minijinja template for the value.
    /// Rendered values are parsed as YAML scalars/collections when possible.
    pub frontmatter: BTreeMap<String, String>,
    /// Optional minijinja body template. When `None`, the extracted Markdown is
    /// used verbatim.
    pub body: Option<String>,
}

/// Select the best template for `host`: the longest matching host suffix wins;
/// a `*`/empty entry is the fallback.
pub fn select_template<'a>(templates: &'a [ClipTemplate], host: &str) -> Option<&'a ClipTemplate> {
    let mut best: Option<&ClipTemplate> = None;
    let mut best_len = 0usize;
    let mut fallback: Option<&ClipTemplate> = None;
    for t in templates {
        let m = t.match_host.trim();
        if m == "*" || m.is_empty() {
            fallback = Some(t);
            continue;
        }
        let matches = host == m || host.ends_with(&format!(".{m}"));
        if matches && m.len() >= best_len {
            best_len = m.len();
            best = Some(t);
        }
    }
    best.or(fallback)
}

/// Host component of `source_url`, for template context and selection.
fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// Build the minijinja context shared by frontmatter and body rendering.
fn template_context(doc: &ClipDocument, ingested_at: &str, tags: &[String]) -> minijinja::Value {
    context! {
        title => doc.title.clone(),
        source_url => doc.source_url.clone(),
        source_type => super::note::SOURCE_TYPE_ARTICLE,
        author => doc.author.clone(),
        published => doc.published.clone(),
        excerpt => doc.excerpt.clone(),
        site_name => doc.site_name.clone(),
        host => host_of(&doc.source_url),
        content => doc.markdown.clone(),
        ingested_at => ingested_at.to_string(),
        tags => tags.to_vec(),
    }
}

/// Render `template_value` against `ctx`, returning `None` on error so callers
/// can skip a broken frontmatter entry.
pub(crate) fn render_str(
    env: &Environment<'_>,
    template_value: &str,
    ctx: &minijinja::Value,
) -> Option<String> {
    env.render_str(template_value, ctx).ok()
}

/// Parse a rendered frontmatter value as YAML (so `[a, b]`, `true`, `42` keep
/// their types), falling back to a plain string.
fn parse_yaml_value(rendered: &str) -> Value {
    let trimmed = rendered.trim();
    serde_yaml::from_str::<Value>(trimmed).unwrap_or_else(|_| Value::from(trimmed.to_string()))
}

/// Apply `template`'s extra frontmatter entries onto `fm`, rendering each value.
pub(crate) fn apply_template_frontmatter(
    fm: &mut Mapping,
    template: &ClipTemplate,
    env: &Environment<'_>,
    ctx: &minijinja::Value,
) {
    for (key, value_tpl) in &template.frontmatter {
        if let Some(rendered) = render_str(env, value_tpl, ctx) {
            fm.insert(Value::from(key.clone()), parse_yaml_value(&rendered));
        }
    }
}

/// Compute the default provenance frontmatter mapping for `doc`.
pub(crate) fn default_frontmatter(
    doc: &ClipDocument,
    ingested_at: &str,
    tags: &[Value],
) -> Mapping {
    let mut fm = Mapping::new();
    fm.insert(Value::from("title"), Value::from(doc.title.clone()));
    fm.insert(
        Value::from("source_url"),
        Value::from(doc.source_url.clone()),
    );
    fm.insert(
        Value::from("source_type"),
        Value::from(super::note::SOURCE_TYPE_ARTICLE),
    );
    if let Some(author) = &doc.author {
        fm.insert(Value::from("author"), Value::from(author.clone()));
    }
    if let Some(published) = &doc.published {
        fm.insert(Value::from("published"), Value::from(published.clone()));
    }
    fm.insert(Value::from("ingested_at"), Value::from(ingested_at));
    fm.insert(Value::from("tags"), Value::Sequence(tags.to_vec()));
    fm
}

/// Render `doc` into a full note applying an optional per-domain `template`.
///
/// `tag_values`/`tag_strings` are the resolved tag list (`inbox` first). When
/// `template` is `None`, this produces the default note (default frontmatter +
/// extracted Markdown body).
pub(crate) fn render_with_template(
    doc: &ClipDocument,
    ingested_at: &str,
    tag_values: &[Value],
    tag_strings: &[String],
    template: Option<&ClipTemplate>,
) -> String {
    let mut fm = default_frontmatter(doc, ingested_at, tag_values);

    let body = if let Some(template) = template {
        let env = Environment::new();
        let ctx = template_context(doc, ingested_at, tag_strings);
        apply_template_frontmatter(&mut fm, template, &env, &ctx);
        match &template.body {
            Some(body_tpl) => render_str(&env, body_tpl, &ctx)
                .unwrap_or_else(|| doc.markdown.clone())
                .trim()
                .to_string(),
            None => doc.markdown.trim().to_string(),
        }
    } else {
        doc.markdown.trim().to_string()
    };

    let yaml = serde_yaml::to_string(&Value::Mapping(fm))
        .unwrap_or_default()
        .trim_end()
        .to_string();
    format!("---\n{yaml}\n---\n\n{body}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tpl(host: &str) -> ClipTemplate {
        ClipTemplate {
            match_host: host.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn selects_most_specific_host() {
        let templates = vec![tpl("*"), tpl("example.com"), tpl("blog.example.com")];
        let sel = select_template(&templates, "blog.example.com").unwrap();
        assert_eq!(sel.match_host, "blog.example.com");
    }

    #[test]
    fn matches_subdomain_via_suffix() {
        let templates = vec![tpl("example.com")];
        let sel = select_template(&templates, "news.example.com").unwrap();
        assert_eq!(sel.match_host, "example.com");
    }

    #[test]
    fn falls_back_to_wildcard() {
        let templates = vec![tpl("*"), tpl("other.com")];
        let sel = select_template(&templates, "example.com").unwrap();
        assert_eq!(sel.match_host, "*");
    }

    #[test]
    fn no_match_and_no_fallback_is_none() {
        let templates = vec![tpl("other.com")];
        assert!(select_template(&templates, "example.com").is_none());
    }

    fn sample() -> ClipDocument {
        ClipDocument {
            source_url: "https://example.com/post".to_string(),
            title: "Title".to_string(),
            author: Some("Jane".to_string()),
            published: None,
            excerpt: None,
            site_name: Some("Example".to_string()),
            markdown: "Body.".to_string(),
        }
    }

    #[test]
    fn template_adds_frontmatter_and_body() {
        let mut fm = BTreeMap::new();
        fm.insert("category".to_string(), "{{ host }}".to_string());
        fm.insert("stars".to_string(), "3".to_string());
        let template = ClipTemplate {
            match_host: "example.com".to_string(),
            frontmatter: fm,
            body: Some("# {{ title }}\n\n> from {{ site_name }}\n\n{{ content }}".to_string()),
        };
        let tag_values = vec![Value::from("inbox")];
        let tag_strings = vec!["inbox".to_string()];
        let note = render_with_template(
            &sample(),
            "2026-07-09T00:00:00Z",
            &tag_values,
            &tag_strings,
            Some(&template),
        );
        assert!(note.contains("category: example.com"));
        // Numeric YAML scalar, not quoted string.
        assert!(note.contains("stars: 3"));
        assert!(note.contains("# Title"));
        assert!(note.contains("> from Example"));
        assert!(note.contains("Body."));
    }

    #[test]
    fn broken_body_template_falls_back_to_markdown() {
        let template = ClipTemplate {
            match_host: "example.com".to_string(),
            frontmatter: BTreeMap::new(),
            body: Some("{{ unclosed".to_string()),
        };
        let tag_values = vec![Value::from("inbox")];
        let tag_strings = vec!["inbox".to_string()];
        let note = render_with_template(
            &sample(),
            "2026-07-09T00:00:00Z",
            &tag_values,
            &tag_strings,
            Some(&template),
        );
        assert!(note.contains("Body."));
    }

    #[test]
    fn none_template_matches_default_frontmatter() {
        let tag_values = vec![Value::from("inbox")];
        let tag_strings = vec!["inbox".to_string()];
        let note = render_with_template(
            &sample(),
            "2026-07-09T00:00:00Z",
            &tag_values,
            &tag_strings,
            None,
        );
        assert!(note.contains("source_type: article"));
        assert!(note.contains("author: Jane"));
        assert!(note.trim_end().ends_with("Body."));
    }
}
