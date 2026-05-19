//! Structural assertion tests for OFM features.
//!
//! These verify that specific OFM syntax produces the correct HTML elements,
//! independent of exact whitespace or attribute ordering. More stable than
//! full-HTML snapshots for catching semantic regressions.

use notesmith_html::render_to_html;

// --- Wikilinks ---

#[test]
fn wikilink_produces_anchor_with_class() {
    let html = render_to_html("Link to [[My Page]].");
    assert!(html.contains(r#"<a class="wikilink" data-target="My Page">My Page</a>"#));
}

#[test]
fn wikilink_alias_shows_display_text() {
    let html = render_to_html("See [[Target Page|display text]].");
    assert!(html.contains(r#"data-target="Target Page">display text</a>"#));
}

#[test]
fn wikilink_with_heading_ref() {
    let html = render_to_html("See [[Page#Heading]].");
    assert!(html.contains(r#"data-target="Page#Heading""#));
}

// --- Highlights ---

#[test]
fn highlight_produces_mark_element() {
    let html = render_to_html("This is ==important== text.");
    assert!(html.contains("<mark>important</mark>"));
}

#[test]
fn highlight_does_not_match_inside_fenced_code_block() {
    // Fenced code blocks: comrak wraps in <pre><code>, and the == are HTML-escaped
    // or preserved as literal text. Our regex runs on HTML so it won't match inside
    // <code> blocks because comrak escapes the content.
    let html = render_to_html("```\n==not highlighted==\n```");
    assert!(!html.contains("<mark>"));
}

// --- Comments ---

#[test]
fn inline_comment_is_stripped() {
    let html = render_to_html("visible %%hidden%% text");
    assert!(!html.contains("hidden"));
    assert!(html.contains("visible"));
}

#[test]
fn block_comment_is_stripped() {
    let html = render_to_html("before\n%%\nmulti\nline\n%%\nafter");
    assert!(!html.contains("multi"));
    assert!(!html.contains("line"));
    assert!(html.contains("before"));
    assert!(html.contains("after"));
}

// --- Embeds ---

#[test]
fn note_embed_produces_div_with_data_target() {
    let html = render_to_html("![[Some Note]]");
    assert!(html.contains(r#"class="embed""#));
    assert!(html.contains(r#"data-target="Some Note""#));
}

#[test]
fn image_embed_produces_img_tag() {
    for ext in &["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp"] {
        let html = render_to_html(&format!("![[photo.{ext}]]"));
        assert!(
            html.contains("<img"),
            "expected <img> for .{ext}, got: {html}"
        );
        assert!(html.contains(&format!(r#"src="photo.{ext}""#)));
    }
}

#[test]
fn embed_with_section_ref_shows_note_name() {
    let html = render_to_html("![[Note#Section]]");
    assert!(html.contains(r#"data-target="Note#Section""#));
    assert!(html.contains(">Note</a>"));
}

// --- Callouts ---

#[test]
fn callout_produces_div_with_type_class() {
    let html = render_to_html("> [!warning] Watch out\n> Danger ahead");
    assert!(html.contains(r#"class="callout callout-warning""#));
    assert!(html.contains(r#"data-callout="warning""#));
    assert!(html.contains("Watch out"));
}

#[test]
fn callout_type_is_case_insensitive() {
    let html = render_to_html("> [!WARNING] Title");
    assert!(html.contains("callout-warning"));
}

#[test]
fn callout_without_title_uses_type_name() {
    let html = render_to_html("> [!tip]\n> Content");
    assert!(html.contains(r#"<div class="callout-title">Tip</div>"#));
}

#[test]
fn foldable_callout_has_data_fold() {
    let collapsed = render_to_html("> [!faq]- Question\n> Answer");
    assert!(collapsed.contains(r#"class="callout callout-question""#));
    assert!(collapsed.contains(r#"data-callout="faq""#));
    assert!(collapsed.contains(r#"data-callout-fold="-""#));
    assert!(collapsed.contains(r#"data-fold="closed""#));

    let expanded = render_to_html("> [!info]+ Details\n> Content");
    assert!(expanded.contains(r#"data-callout-fold="+""#));
    assert!(expanded.contains(r#"data-fold="open""#));
}

#[test]
fn all_builtin_callout_types_recognized() {
    let types = [
        "note", "abstract", "info", "tip", "success", "question", "warning", "failure", "danger",
        "bug", "example", "quote",
    ];
    for ty in types {
        let html = render_to_html(&format!("> [!{ty}] Title"));
        assert!(
            html.contains(&format!("callout-{ty}")),
            "callout type {ty} not recognized in: {html}"
        );
    }
}

#[test]
fn builtin_callout_aliases_use_canonical_styles() {
    let aliases = [
        ("summary", "abstract"),
        ("tldr", "abstract"),
        ("hint", "tip"),
        ("important", "tip"),
        ("check", "success"),
        ("done", "success"),
        ("help", "question"),
        ("faq", "question"),
        ("caution", "warning"),
        ("attention", "warning"),
        ("fail", "failure"),
        ("missing", "failure"),
        ("error", "danger"),
        ("cite", "quote"),
    ];

    for (alias, canonical) in aliases {
        let html = render_to_html(&format!("> [!{alias}]"));
        assert!(
            html.contains(&format!(r#"class="callout callout-{canonical}""#)),
            "callout alias {alias} should use {canonical} style: {html}"
        );
        assert!(
            html.contains(&format!(r#"data-callout="{alias}""#)),
            "callout alias should preserve identifier: {html}"
        );
    }
}

#[test]
fn unsupported_callout_type_defaults_to_note_style() {
    let html = render_to_html("> [!custom-type] Custom title\n> Body");

    assert!(html.contains(r#"class="callout callout-note""#));
    assert!(html.contains(r#"data-callout="custom-type""#));
    assert!(html.contains("Custom title"));
}

#[test]
fn nested_callouts_render_at_multiple_levels() {
    let html = render_to_html(
        "> [!question] Can callouts be nested?\n> > [!todo] Yes, they can.\n> > > [!example] Multiple layers.",
    );

    assert!(html.contains(r#"class="callout callout-question""#));
    assert!(html.contains(r#"class="callout callout-todo""#));
    assert!(html.contains(r#"class="callout callout-example""#));
}

// --- Extended Tasks ---

#[test]
fn standard_tasks_render_checkboxes() {
    let html = render_to_html("- [ ] unchecked\n- [x] checked");
    assert!(html.contains(r#"type="checkbox""#));
    let checkbox_count = html.matches(r#"type="checkbox""#).count();
    assert_eq!(checkbox_count, 2, "expected 2 checkboxes, html: {html}");
}

#[test]
fn extended_task_markers_render_as_checked() {
    let markers = ['?', '!', '-', '/', 'b', 'w', 'h'];
    for m in markers {
        let html = render_to_html(&format!("- [{m}] task with {m}"));
        assert!(
            html.contains(&format!(r#"data-task="{m}""#)),
            "marker '{m}' missing data-task attribute: {html}"
        );
        assert!(
            html.contains("checked"),
            "marker '{m}' should be checked: {html}"
        );
    }
}

// --- Highlights ---

#[test]
fn highlight_within_paragraph() {
    let html = render_to_html("Start ==middle== end.");
    assert!(html.contains("<mark>middle</mark>"));
    assert!(html.contains("Start"));
    assert!(html.contains("end."));
}

// --- Strikethrough ---

#[test]
fn strikethrough_produces_del_element() {
    let html = render_to_html("This is ~~deleted~~ text.");
    assert!(html.contains("<del>deleted</del>"));
}

// --- Footnotes ---

#[test]
fn footnote_produces_link_and_definition() {
    let html = render_to_html("Text[^1] here.\n\n[^1]: The footnote.");
    assert!(html.contains("footnote"));
    assert!(html.contains("The footnote."));
}

// --- Tables ---

#[test]
fn table_produces_thead_and_tbody() {
    let html = render_to_html("| A | B |\n|---|---|\n| 1 | 2 |");
    assert!(html.contains("<table"));
    assert!(html.contains("<thead>"));
    assert!(html.contains("<tbody>"));
}

// --- Horizontal Rule ---

#[test]
fn horizontal_rule_produces_hr() {
    let html = render_to_html("Above\n\n---\n\nBelow");
    assert!(html.contains("<hr"));
}

// --- Frontmatter ---

#[test]
fn frontmatter_not_stripped_in_render_to_html() {
    // render_to_html does NOT strip frontmatter (that's render_to_html_with_inline_styles)
    // This test documents the current behavior — frontmatter appears as rendered content
    let html = render_to_html("---\ntitle: Test\n---\n# Heading");
    assert!(html.contains("<h1>Heading</h1>"));
}
