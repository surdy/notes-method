---
type: note
tags:
  - test
  - edge-cases
created: 2025-01-15 20:00
updated: 2025-01-15 20:30
---

# OFM Edge Cases

This note exercises every OFM element that the parser must handle, including edge cases and less common syntax.

## Same-Document Anchors

Jump to [[#Nested Tags]] or [[#Footnotes]] below.

Internal link to a heading: [[#Foldable Callouts]]

## Nested Tags

#project/notesmith #status/active #priority/high #area/work/engineering

Plain text with a hash that isn't a tag: issue #42, C# language, channel #general.

## External Links

- Markdown link: [Obsidian Help](https://help.obsidian.md/)
- Another: [Rust Book](https://doc.rust-lang.org/book/)
- Bare URL: https://github.com/surdy/notes-method
- Link with title: [MDN Web Docs](https://developer.mozilla.org "MDN")

## Markdown Links to Notes

- Relative link: [Migration Notes](../Customers/Acme/Streams/Migration%20to%20v2.md)
- Link with heading: [Acme Overview](../Customers/Acme/Acme%20Corp.md#Overview)

## Foldable Callouts

> [!tip]+ Expanded by Default
> This callout starts expanded.
> It has multiple lines of content.
> And even a third line.

> [!danger]- Collapsed by Default
> This callout starts collapsed.
> Users must click to expand it.

> [!example] Standard (No Fold Marker)
> This callout has no fold behavior.
> It simply displays as-is.

> [!success]
> A callout with no title — type only.

## Highlights and Formatting

This is ==highlighted text== within a sentence.

This has ~~strikethrough~~ text and **bold** and *italic* and ***bold italic***.

Inline `code` and a ==multi-word highlight==.

## Comments

This paragraph is visible.

%%
This is a hidden comment block.
It should not appear in rendered output.
Multiple lines of hidden content here.
%%

Visible again. With an inline hidden comment: %%secret note%% and then text continues.

## Footnotes

This sentence has a footnote[^1] and another[^longnote].

[^1]: This is a simple footnote.

[^longnote]: This is a longer footnote with multiple paragraphs.

    The second paragraph of the footnote.

## Complex Task Lines

- [ ] Task with recurrence 🔁 every weekday 📅 2025-01-20
- [ ] Task with all metadata ⏫ 📅 2025-02-01 ⏳ 2025-01-25 🛫 2025-01-20 ➕ 2025-01-10
- [x] Completed with done date ✅ 2025-01-14
- [-] Cancelled with cancel date ❌ 2025-01-13
- [b] Blocked task with due date 📅 2025-01-30 🔽
- [w] Waiting on response ⏳ 2025-02-05
- [h] On hold — revisit in Q2

Indented/nested tasks:

- [ ] Parent task 📅 2025-02-01
    - [ ] Subtask one 🔼
    - [x] Subtask two ✅ 2025-01-10
    - [/] Subtask three in progress

## Block References

This paragraph has an ID. ^edge-case-block

Reference it elsewhere: [[OFM Edge Cases#^edge-case-block]]

Another block with an ID at the end of a list item:

- Important list item ^list-block-id

## Tables

### Simple Table

| Feature | Supported |
|---------|-----------|
| Wikilinks | Yes |
| Embeds | Yes |
| Tasks | Yes |

### Aligned Table

| Left | Center | Right |
|:-----|:------:|------:|
| a | b | c |
| longer text | centered | 123 |

## Code Blocks

```rust
fn main() {
    // Wikilinks inside code should NOT be parsed: [[NotALink]]
    let task = "- [ ] Not a task";
    println!("Hello, Notesmith!");
}
```

```notesmith sql
SELECT path, title, type
FROM v_notes
WHERE type = 'stream'
ORDER BY updated DESC
```

Inline code with OFM syntax that should be ignored: `[[not a link]]` and `#not-a-tag` and `[not:: a field]`.

## Consecutive Wikilinks

[[Acme Corp]] and [[Globex]] are both customers. See [[Migration to v2]] or [[Platform Rollout]] for active streams.

## Emoji in Content

Regular emoji in text: 🎉 🚀 💡 — these are not task metadata.

## Empty Frontmatter Edge Case

This note has frontmatter, so this section just tests content after frontmatter.
