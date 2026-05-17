# Notesmith View Modes

Notesmith gives every note tab its own view mode, following Obsidian's familiar editing model. You can switch between raw markdown editing, inline rendered editing, and fully rendered reading without losing your place.

Mode is stored per tab and persists across sessions, so each note opens the way you left it.

---

## Overview

Notesmith offers three view modes for every note tab:

| Mode | What it shows | Best for |
|------|---------------|----------|
| **Source mode** | Raw markdown in the full editor | Precise editing, frontmatter, bulk changes |
| **Live Preview** | Inline rendered markdown with editable cursor line | Everyday writing with visual feedback |
| **Reading View** | Fully rendered read-only HTML | Reviewing notes, checking tasks, presenting content |

Each tab remembers its own mode independently. If one tab is in Source mode and another is in Reading View, both stay that way across refreshes and app restarts.

---

## Switching Modes

You can change a note tab's mode in three ways:

- **Keyboard:** `⌘E` cycles through `source → live-preview → reading → source`
- **Toolbar icon:** click the mode icon on the right side of the note toolbar
- **Command palette:** `⌘K` → **Toggle View Mode**

### Toolbar icons

| Icon | Mode |
|------|------|
| `</>` | Source |
| `✏️` | Live Preview |
| `📖` | Reading View |

---

## Source Mode

Source mode is the full CodeMirror 6 editor showing raw markdown exactly as written.

### What you see

- Full syntax highlighting in the editor
- Raw markdown characters and structures
- Frontmatter exactly as stored
- Every markdown construct in its literal form

Examples of visible syntax:

- `#` for headings
- `**bold**`
- `[[wikilinks]]`
- `- [ ]` task lists
- Fenced code blocks such as ```` ```sql ````

### Best for

- Precise editing
- Complex frontmatter changes
- Large structural edits
- Bulk markdown cleanup

### SQL blocks

SQL fenced blocks (` ```sql `) still execute and render results inline while you edit.

---

## Live Preview Mode

Live Preview hides markdown syntax on lines you are not actively editing and renders the content inline. When your cursor enters a line, the raw markdown for that line appears so you can edit it directly.

### How it behaves

- Non-cursor lines render like formatted content
- The active cursor line always shows raw markdown
- Moving the cursor reveals markdown only where you are editing
- The rest of the note stays visually rendered

### Inline rendering behavior

| Markdown | Live Preview result |
|----------|---------------------|
| `# Heading` | `#` marker is hidden and the text is styled as a heading |
| `**bold**` | `**` markers are hidden and the text renders bold |
| `*italic*` or `_italic_` | Markers are hidden and the text renders italic |
| `~~done~~` | `~~` markers are hidden and the text renders with strikethrough |
| `[text](url)` | Markdown link syntax is hidden and the link text is styled |
| `` `code` `` | Backticks are hidden and the text renders with inline code styling |
| Markdown table | Rendered as an editable table; edit cell text directly, or use `+ Row` / `+ Column` to extend the table |
| `---` | Replaced with a rendered horizontal rule |

### Best for

- Everyday writing
- Fast note-taking
- Editing while keeping visual feedback
- Staying close to final rendered output

### SQL blocks

SQL fenced blocks still execute and render results inline in Live Preview.

---

## Reading View

Reading View shows fully rendered HTML generated server-side by the Notesmith daemon.

### What to expect

- Fully rendered note content
- Read-only display
- No direct text editing
- Support for Obsidian Flavored Markdown constructs such as wikilinks, callouts, tags, and inline fields

### Interactive task checkboxes

Task checkboxes remain interactive in Reading View. Clicking a checkbox sends an update request to the daemon, then re-renders the note.

| Markdown marker | Status |
|-----------------|--------|
| `- [ ]` | todo |
| `- [/]` | in progress |
| `- [x]` | done |
| `- [b]` | blocked |
| `- [w]` | waiting |
| `- [h]` | on hold |
| `- [-]` | cancelled |

### Best for

- Reviewing notes
- Checking off tasks
- Reading long-form content
- Presenting polished note output

---

## Auto-Save Behavior

Notesmith saves editor content before switching into Reading View.

- **Live Preview → Reading View:** current edits are auto-saved first
- **Source → Reading View:** current edits are preserved before rendering
- Toggling modes does not drop unsaved work

You can move between editing and reading without worrying about losing changes.

---

## Per-Tab Persistence

View mode is stored per note tab, not globally.

- Each tab keeps its own mode
- Different tabs can stay in different modes at the same time
- Modes survive browser refresh
- Modes survive app restart
- Persistence uses `localStorage`
- New tabs default to **Source mode**

This makes it easy to keep an editing tab open next to a read-only reference tab.
