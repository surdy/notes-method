//! Golden comparison tests: compare Notesmith's HTML rendering against
//! Obsidian's ground-truth output from the export plugin.
//!
//! These tests extract semantic elements (tags, classes, text content) from
//! both outputs and compare them. Full HTML equality isn't the goal — Obsidian
//! wraps content in extra divs, adds SVG icons, copy buttons, etc. Instead we
//! compare the *semantic structure*: which elements are produced, which classes
//! are set, and which text content appears.

use notesmith_html::render_to_html;
use regex::Regex;
use std::path::Path;

fn fixture_md(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures/obsidian-sandbox/Formatting")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

fn golden_html(name: &str) -> String {
    let stem = name.trim_end_matches(".md");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures/obsidian-golden")
        .join(format!("{stem}.html"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read golden {}: {e}", path.display()))
}

/// Extract a simplified list of semantic tags from HTML for comparison.
/// Returns tags like: "h1", "h2", "p", "mark", "strong", "em", "del",
/// "a.internal-link", "a.wikilink", "div.callout", "li.task-list-item", etc.
fn extract_semantic_tags(html: &str) -> Vec<String> {
    let tag_re = Regex::new(r#"<([\w]+)([^>]*)>"#).unwrap();
    let class_re = Regex::new(r#"class="([^"]*)""#).unwrap();

    let mut tags = Vec::new();
    for cap in tag_re.captures_iter(html) {
        let tag = cap[1].to_lowercase();
        // Skip non-semantic tags
        if matches!(
            tag.as_str(),
            "svg" | "path" | "rect" | "button" | "span" | "br" | "hr" | "img"
        ) {
            continue;
        }
        let attrs = &cap[2];
        if let Some(class_cap) = class_re.captures(attrs) {
            let classes = &class_cap[1];
            // Extract meaningful classes
            for class in classes.split_whitespace() {
                match class {
                    "callout" | "callout-title" | "callout-content" | "callout-body"
                    | "internal-link" | "wikilink" | "external-link" | "tag" | "task-list-item"
                    | "is-checked" | "contains-task-list" | "embed" | "embed-image"
                    | "embed-link" => {
                        tags.push(format!("{tag}.{class}"));
                    }
                    c if c.starts_with("callout-") => {
                        // skip callout-icon, callout-title-inner, etc. unless it's the type
                    }
                    _ => {}
                }
            }
        }
        // Always record the base tag for structural elements
        if matches!(
            tag.as_str(),
            "h1" | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "mark"
                | "strong"
                | "em"
                | "del"
                | "table"
                | "thead"
                | "tbody"
                | "blockquote"
                | "pre"
                | "code"
                | "sup"
        ) {
            tags.push(tag);
        }
    }
    tags
}

/// Check that a Notesmith-rendered feature matches the Obsidian golden output
/// for a specific semantic pattern.
fn assert_feature_parity(fixture: &str, feature: &str, check: impl Fn(&str, &str)) {
    let md = fixture_md(fixture);
    let notesmith = render_to_html(&md);
    let obsidian = golden_html(fixture);
    check(&notesmith, &obsidian);
    eprintln!("  ✓ {fixture}: {feature}");
}

// --- Highlighting ---

#[test]
fn golden_highlighting_uses_mark() {
    assert_feature_parity("Highlighting.md", "<mark> element", |ns, obs| {
        assert!(
            obs.contains("<mark>"),
            "Obsidian golden should contain <mark>"
        );
        assert!(ns.contains("<mark>"), "Notesmith should contain <mark>");
        // Both should highlight the same text
        assert!(obs.contains("<mark>highlight text</mark>"));
        assert!(ns.contains("<mark>highlight text</mark>"));
    });
}

// --- Callouts ---

#[test]
fn golden_callout_structure() {
    assert_feature_parity("Callout.md", "callout divs", |ns, obs| {
        // Obsidian uses data-callout="info", Notesmith uses class="callout-info"
        assert!(
            obs.contains(r#"data-callout="info""#),
            "Obsidian should have data-callout: {obs}"
        );
        assert!(
            ns.contains("callout-info"),
            "Notesmith should have callout-info class"
        );

        // Both should render the callout title text
        assert!(obs.contains("Info"), "Obsidian title missing");
        assert!(
            ns.contains("Info") || ns.contains("Here's a callout block"),
            "Notesmith callout content missing"
        );
    });
}

#[test]
fn golden_callout_foldable() {
    assert_feature_parity("Callout.md", "foldable callout", |ns, obs| {
        // Obsidian marks foldable callouts with data-callout-fold="-"
        assert!(
            obs.contains(r#"data-callout-fold="-""#),
            "Obsidian should have fold marker"
        );
        // Notesmith uses data-fold="closed"
        assert!(
            ns.contains(r#"data-fold="closed""#),
            "Notesmith should have fold marker"
        );
    });
}

// --- Tasks ---

#[test]
fn golden_task_extended_markers() {
    assert_feature_parity("Task.md", "extended task markers", |ns, obs| {
        // Obsidian: data-task="?" on li, checkbox checked
        assert!(
            obs.contains(r#"data-task="?""#),
            "Obsidian should have data-task='?'"
        );
        // Notesmith: data-task="?" on input, checkbox checked
        assert!(
            ns.contains(r#"data-task="?""#),
            "Notesmith should have data-task='?'"
        );

        // Both should have task-list-item class
        assert!(obs.contains("task-list-item"));
        assert!(ns.contains("task-list-item"));
    });
}

#[test]
fn golden_task_standard_checkboxes() {
    assert_feature_parity("Task.md", "standard checkboxes", |ns, obs| {
        // Both should render checkboxes
        assert!(obs.contains(r#"type="checkbox""#));
        assert!(ns.contains(r#"type="checkbox""#));

        // Both should have checked items
        assert!(obs.contains("checked"));
        assert!(ns.contains("checked"));
    });
}

// --- Internal Links / Wikilinks ---

#[test]
fn golden_internal_links() {
    assert_feature_parity("Internal link.md", "wikilink rendering", |ns, obs| {
        // Obsidian renders as <a class="internal-link" ...>
        assert!(
            obs.contains("internal-link"),
            "Obsidian should use internal-link class"
        );
        // Notesmith renders as <a class="wikilink" ...>
        assert!(
            ns.contains("wikilink"),
            "Notesmith should use wikilink class"
        );

        // Both should link to "Embeds"
        assert!(obs.contains("Embeds"), "Obsidian link target missing");
        assert!(ns.contains("Embeds"), "Notesmith link target missing");
    });
}

// --- Emphasis ---

#[test]
fn golden_emphasis() {
    assert_feature_parity("Emphasis.md", "bold and italic", |ns, obs| {
        let ns_tags = extract_semantic_tags(ns);
        let obs_tags = extract_semantic_tags(obs);

        assert!(
            obs_tags.contains(&"em".to_string()),
            "Obsidian should have <em>"
        );
        assert!(
            ns_tags.contains(&"em".to_string()),
            "Notesmith should have <em>"
        );
        assert!(
            obs_tags.contains(&"strong".to_string()),
            "Obsidian should have <strong>"
        );
        assert!(
            ns_tags.contains(&"strong".to_string()),
            "Notesmith should have <strong>"
        );
    });
}

// --- Strikethrough ---

#[test]
fn golden_strikethrough() {
    assert_feature_parity("Strikethrough.md", "<del> element", |ns, obs| {
        // Obsidian uses <s>, comrak uses <del> — both are valid
        let obs_has = obs.contains("<s>") || obs.contains("<del>");
        let ns_has = ns.contains("<del>");
        assert!(obs_has, "Obsidian should have strikethrough element");
        assert!(ns_has, "Notesmith should have <del>");
    });
}

// --- Headings ---

#[test]
fn golden_headings() {
    assert_feature_parity("Heading.md", "heading levels", |ns, obs| {
        for level in 1..=6 {
            let tag = format!("<h{level}");
            assert!(obs.contains(&tag), "Obsidian missing h{level}");
            assert!(ns.contains(&tag), "Notesmith missing h{level}");
        }
    });
}

// --- Tables ---

#[test]
fn golden_tables() {
    assert_feature_parity("Table.md", "table structure", |ns, obs| {
        for tag in &["<table", "<thead", "<tbody", "<th", "<td"] {
            assert!(obs.contains(tag), "Obsidian missing {tag}");
            assert!(ns.contains(tag), "Notesmith missing {tag}");
        }
    });
}

// --- Code blocks ---

#[test]
fn golden_code_blocks() {
    assert_feature_parity("Code block.md", "pre/code elements", |ns, obs| {
        assert!(obs.contains("<pre"), "Obsidian missing <pre>");
        assert!(ns.contains("<pre"), "Notesmith missing <pre>");
        assert!(obs.contains("<code"), "Obsidian missing <code>");
        assert!(ns.contains("<code"), "Notesmith missing <code>");
    });
}

// --- Footnotes ---

#[test]
fn golden_footnotes() {
    assert_feature_parity("Footnote.md", "footnote rendering", |ns, obs| {
        // Both should have footnote references (superscript links)
        assert!(
            obs.contains("footnote") || obs.contains("fn-"),
            "Obsidian missing footnote markers"
        );
        assert!(
            ns.contains("footnote"),
            "Notesmith missing footnote markers"
        );
    });
}

// --- Comments ---

#[test]
fn golden_comments_stripped() {
    assert_feature_parity("Comment.md", "comment stripping", |ns, obs| {
        // Strip <pre>...</pre> blocks — both outputs include a code-block
        // example showing the raw markdown syntax (which contains %%...%%).
        let pre_re = Regex::new(r"(?s)<pre[^>]*>.*?</pre>").unwrap();
        let obs_no_pre = pre_re.replace_all(obs, "");
        let ns_no_pre = pre_re.replace_all(ns, "");

        assert!(
            !obs_no_pre.contains("You can't see this text"),
            "Obsidian should strip inline comments (outside code blocks)"
        );
        assert!(
            !ns_no_pre.contains("You can't see this text"),
            "Notesmith should strip inline comments (outside code blocks)"
        );
    });
}

// --- Horizontal divider ---

#[test]
fn golden_horizontal_divider() {
    assert_feature_parity("Horizontal divider.md", "<hr> element", |ns, obs| {
        assert!(obs.contains("<hr"), "Obsidian missing <hr>");
        assert!(ns.contains("<hr"), "Notesmith missing <hr>");
    });
}

// --- Lists ---

#[test]
fn golden_lists() {
    assert_feature_parity("Lists.md", "list elements", |ns, obs| {
        assert!(
            obs.contains("<ul>") || obs.contains("<ul"),
            "Obsidian missing <ul>"
        );
        assert!(
            ns.contains("<ul>") || ns.contains("<ul"),
            "Notesmith missing <ul>"
        );
        assert!(
            obs.contains("<ol>") || obs.contains("<ol"),
            "Obsidian missing <ol>"
        );
        assert!(
            ns.contains("<ol>") || ns.contains("<ol"),
            "Notesmith missing <ol>"
        );
    });
}

// --- Summary: tag parity across all fixtures ---

#[test]
fn golden_structural_tag_coverage() {
    let fixtures = [
        "Heading.md",
        "Emphasis.md",
        "Highlighting.md",
        "Table.md",
        "Code block.md",
        "Horizontal divider.md",
    ];

    for fixture in fixtures {
        let md = fixture_md(fixture);
        let ns = render_to_html(&md);
        let obs = golden_html(fixture);

        let ns_tags = extract_semantic_tags(&ns);
        let obs_tags = extract_semantic_tags(&obs);

        // Check that every structural tag Obsidian produces, Notesmith also produces
        let structural = [
            "h1", "h2", "h3", "p", "strong", "em", "mark", "del", "table", "pre", "code",
        ];
        for tag in structural {
            if obs_tags.contains(&tag.to_string()) {
                assert!(
                    ns_tags.contains(&tag.to_string()),
                    "{fixture}: Obsidian has <{tag}> but Notesmith doesn't.\n  Notesmith tags: {ns_tags:?}\n  Obsidian tags: {obs_tags:?}"
                );
            }
        }
    }
}
