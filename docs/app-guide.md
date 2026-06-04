# Notesmith Desktop App User Guide

Notesmith's desktop app is the main place to read, write, and organize notes by hand. It is a SvelteKit frontend served by the Notesmith daemon and wrapped in Tauri for macOS.

For automation and system-level details, see the [CLI Reference](cli.md), [HTTP API Reference](http-api.md), and [SQL Views Reference](sql-views.md). This guide stays focused on the day-to-day desktop experience.

## 1. Overview

Notesmith uses a three-pane layout:

- **Sidebar (left):** note navigation, file tree, and smart views
- **Editor area (center):** tabs, note toolbar, editing, and reading
- **Right rail (right):** context for the selected note

The desktop app connects to the live Notesmith daemon discovered from the Notesmith lockfile. By default that is `http://127.0.0.1:27183`, but the desktop shell follows the daemon's active port when it changes.

In practice, that means:

- the daemon provides notes, search, SQL results, and rendered HTML
- the desktop shell gives you a native macOS window around that app
- most note work happens in the center pane, with the sidebar and right rail supporting navigation and context

## 2. Sidebar

The sidebar provides two kinds of navigation:

- **Files tab** — always present; shows the full vault as a collapsible file tree
- **Custom views** — defined in `.notesmith/sidebar.yaml`; appear as additional tabs

When no `sidebar.yaml` exists, only the Files tab is shown (no tab bar at all).

### Configuring sidebar views

Create `.notesmith/sidebar.yaml` in your vault:

```yaml
views:
  - id: workflow
    name: Workflow
    icon: "⚡"
    badge_query: "SELECT count(*) FROM v_notes WHERE path LIKE 'Capture/%'"
    sections:
      - type: recently-viewed
        label: Recent
        mode: both      # 'viewed' | 'edited' | 'both'
        limit: 10

      - type: custom-folders
        label: Projects
        folders:
          - Projects/Active
          - Customers

      - type: custom-items
        label: Triage
        items:
          - name: Capture
            icon: "⚡"
            source:
              folder: Capture
              recursive: true
              sort: modified
              sort_dir: desc
          - name: Tasks
            icon: "✅"
            source:
              query: |
                SELECT note_path as path, text as title, status_group, line_number as line
                FROM v_tasks WHERE status_group = 'open'
              title_column: title
              subtitle_column: status
              badge_columns: [status]
```

### Section types

**`recently-viewed`** — shows notes you have recently opened or edited.
- `mode: viewed` — from localStorage (notes you clicked on)
- `mode: edited` — from the database (`v_notes` ordered by `updated_at`)
- `mode: both` — merge of both, deduplicated

**`custom-folders`** — renders subtrees of the vault file tree rooted at named folder paths.

**`custom-items`** — a list of named items (with icons) that each open a **middle pane** when clicked.

### Middle pane

Clicking a `custom-items` item opens a scrollable middle pane between the sidebar and the editor. The pane shows the item's contents:

- **Folder source** — lists notes from a folder (title + snippet)
- **Query source** — runs a SQL query against the vault database; supports title, subtitle, and badge columns

The middle pane is resizable by dragging its right edge. Width is persisted per vault + item name.

Click the ✕ button or click the same item again to close the middle pane.

## 3. File Tree / Note Navigation

Notes can be reached in two main ways:

- through the smart views in the sidebar
- through the file tree view

Sidebar views organize notes based on their data source queries rather than raw folders. That makes them ideal for workflows like:

- "show me captured notes waiting to be routed"
- "show me active tasks"
- "show me recent meeting notes"

When you want to browse the vault directly, use the file tree.

To open a note:

1. Find it in a sidebar view or the file tree.
2. Click the note.
3. Notesmith opens it in the editor area as a tab.

## 4. Tab System

Notesmith supports multiple open notes at the same time.

Key points:

- the tab bar sits above the editor area
- each open note gets its own tab
- switching tabs does not close or discard the others
- tabs persist across sessions using `localStorage`

Keyboard shortcuts:

- **⌘W** closes the current tab
- **⌘⇧T** reopens the last closed tab

Right-clicking a tab opens tab-specific context actions.

This makes it easy to keep a few working notes open at once, such as:

- today's daily note
- a project or account note
- a meeting note
- a dashboard note

## 5. Note Toolbar

The note toolbar sits between the tab bar and the editor.

It has two main jobs:

- show the current note path as a centered breadcrumb
- provide the current tab's view mode toggle on the right

The view mode icon changes with the active mode:

- **Code brackets** = Source mode
- **Pencil** = Live Preview mode
- **Book** = Reading View mode

You can switch modes in either of these ways:

- click the icon
- press **⌘E**

The breadcrumb helps you stay oriented when similarly named notes live in different folders.

## 6. View Modes

Each tab has its own view mode, and Notesmith remembers that mode per tab.

Notesmith cycles through modes in this order:

`source → live-preview → reading → source`

Mode summary:

- **Source:** raw markdown editing in CodeMirror
- **Live Preview:** inline markdown rendering while you edit
- **Reading View:** fully rendered HTML for focused reading

Live Preview renders structures such as headings, emphasis, links, and code, while still showing raw markdown syntax on the cursor line.

This guide only summarizes the modes. Deeper rendering behavior can be documented separately without repeating it here.

## 7. Editor (Source & Live Preview)

The editor in Source and Live Preview modes is powered by **CodeMirror 6** with Obsidian Flavored Markdown support.

Highlights include:

- markdown-aware editing
- syntax highlighting
- YAML frontmatter support
- wikilinks
- tags
- inline fields

Auto-save is built in. After you stop typing for a brief moment, Notesmith saves your changes automatically.

What that means for daily use:

- you usually do not need to save manually
- the current note stays up to date as you work
- the tab can show unsaved state while edits are still pending

Conflict handling is designed for a live vault:

- if a file changes externally through the CLI or another tool, the editor detects the change through SSE events
- if the note is clean, the editor refreshes automatically
- if the note has local unsaved edits, the app surfaces the conflict and lets you reload or keep your in-memory changes

SQL code blocks are also supported in the editing surface. Fenced `sql` blocks render live query results against the vault's SQLite cache, which is especially useful for dashboard notes.

Example:

````markdown
```sql
SELECT status, COUNT(*) AS count
FROM v_tasks
GROUP BY status
ORDER BY status;
```
````

That result can render inline as a live table inside the note.

## 8. Command Palette

Open the command palette with **⌘K** or **⌘P**.

As you type, Notesmith filters the available commands. Commands are organized into these categories:

- **Notes**
- **Tasks**
- **Templates**
- **Navigation**
- **Vault**

Available commands include:

- **New Note** (**⌘N**) — create a new note with a title and optional folder
- **Quick Capture** (**⌘⇧N**) — quickly capture text into the default capture folder
- **Copy as HTML** — copy the current note as styled HTML to the clipboard
- **Archive Current Note** (**⌘⇧A**) — route the current note to its archive destination
- **Open Today's Daily Note** (**⌘D**) — create or open today's daily note
- **New Note from Template** — create a note from a template
- **Global Search** (**⌘⇧F**) — open the quick switcher for search
- **Reload Vault** — refresh the note list from the daemon
- **Toggle View Mode** (**⌘E**) — cycle through source, live preview, and reading

Tips:

- type a few letters from the command name instead of browsing the full list
- use it as the fastest way to create notes without leaving the keyboard
- for command-line equivalents, see the [CLI Reference](cli.md)

## 9. Quick Switcher

Open the quick switcher with **⌘O** or **⌘⇧F**.

The quick switcher is the fastest way to jump to a note by title.

How it works:

- start typing a note name
- Notesmith searches across your notes using Tantivy-backed full-text search
- results update quickly as you type
- select a result to open it in a tab

The matching is optimized for fast fuzzy navigation, so partial terms are often enough.

Example searches:

- `acme kickoff`
- `daily 2026-05`
- `blocked tasks`

## 10. Right Rail (Context Panel)

The right rail is a collapsible context panel on the right side of the app.

Toggle it with **⌘\\**.

It follows the currently selected note and shows contextual information such as:

- **Backlinks** — notes that link to the current note
- **Metadata** — frontmatter and note-level fields
- **Heading outline / table of contents** — a quick map of the current note

Use the right rail when you want to answer questions like:

- "What links here?"
- "What metadata is on this note?"
- "Where is the section I want inside this long note?"

## 11. Reading View

Reading View shows the note as fully rendered HTML from the server-side markdown renderer.

This is the best mode for:

- focused reading
- sharing or reviewing formatted notes
- working with rendered task lists

Reading View supports interactive task checkboxes. Clicking a checkbox updates task status directly from the rendered note.

Notesmith supports these OFM task statuses:

- `- [ ]` = `todo`
- `- [/]` = `in_progress`
- `- [x]` = `done`
- `- [b]` = `blocked`
- `- [w]` = `waiting`
- `- [h]` = `on_hold`
- `- [-]` = `cancelled`

Reading View also supports the main OFM constructs you expect, including:

- wikilinks
- callouts
- tags
- inline fields
- task statuses

## 12. SQL Dashboard Blocks

Dashboard notes can include live SQL blocks.

In Source or Live Preview mode, fenced `sql` blocks execute live queries and render their results inline in the editor.

This is useful for notes that act like dashboards, such as:

- a Home note with task summaries
- a notes-by-tag list
- a capture triage page
- a project status review

Queries run against the vault's SQLite cache and can use public views such as:

- `v_notes`
- `v_tasks`
- `v_backlinks`
- `v_fields`
- `v_task_fields`
- `v_periodic`

Example:

````markdown
```sql
SELECT n.title, state.value AS state
FROM v_notes n
JOIN v_fields state ON state.vault_name = n.vault_name AND state.note_path = n.path AND state.key = 'state'
JOIN tags t ON t.vault_name = n.vault_name AND t.note_path = n.path AND t.tag = 'stream'
ORDER BY n.title;
```
````

For the full view schema and column list, see the [SQL Views Reference](sql-views.md).

## 13. Dark Theme

Notesmith uses a dark theme by default.

The theme is tuned for long editing sessions, with:

- muted surrounding chrome
- strong text contrast
- readable code and markdown syntax colors
- a layout that keeps focus on the note content

## 14. URL Scheme

Notesmith supports deep links with the `notesmith://` URL scheme.

To open a specific note:

`notesmith://app/vault/path/to/note.md`

Example:

`notesmith://app/work/Customers/Acme%20Corp/Account%20Info.md`

This is useful for:

- cross-app linking
- launcher shortcuts
- scripts and automations
- opening a note directly from another tool

When you need broader automation or daemon-level integration, use the [CLI Reference](cli.md) or [HTTP API Reference](http-api.md) alongside these links.
