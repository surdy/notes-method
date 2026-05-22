//! Snapshot tests for OFM rendering against Obsidian's sandbox corpus.
//!
//! Each test renders a fixture from `test-fixtures/obsidian-sandbox/Formatting/`
//! through `render_to_html()` and snapshots the output with `insta`.

use notesmith_html::render_to_html;
use std::path::Path;

fn fixture(name: &str) -> String {
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

macro_rules! snapshot_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let md = fixture($file);
            let html = render_to_html(&md);
            insta::assert_snapshot!(html);
        }
    };
}

snapshot_test!(snapshot_blockquote, "Blockquote.md");
snapshot_test!(snapshot_callout, "Callout.md");
snapshot_test!(snapshot_code_block, "Code block.md");
snapshot_test!(snapshot_comment, "Comment.md");
snapshot_test!(snapshot_diagram, "Diagram.md");
snapshot_test!(snapshot_embeds, "Embeds.md");
snapshot_test!(snapshot_emphasis, "Emphasis.md");
snapshot_test!(snapshot_footnote, "Footnote.md");
snapshot_test!(snapshot_heading, "Heading.md");
snapshot_test!(snapshot_highlighting, "Highlighting.md");
snapshot_test!(snapshot_horizontal_divider, "Horizontal divider.md");
snapshot_test!(snapshot_images, "Images.md");
snapshot_test!(snapshot_inline_code, "Inline code.md");
snapshot_test!(snapshot_internal_link, "Internal link.md");
snapshot_test!(snapshot_links, "Links.md");
snapshot_test!(snapshot_lists, "Lists.md");
snapshot_test!(snapshot_math, "Math.md");
snapshot_test!(snapshot_strikethrough, "Strikethrough.md");
snapshot_test!(snapshot_table, "Table.md");
snapshot_test!(snapshot_task, "Task.md");
