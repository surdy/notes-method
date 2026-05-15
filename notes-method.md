# Notes Method

A method for organizing my notes. Some details are figured out; others I want to get ideas on.

## Context

I work with customers, and I want to organize each customer's notes in their own folder.

This method is now intended to be implemented by **Notesmith**, a custom markdown notes app. The definitive application blueprint is `plans/notesmith-plan.md`.
The repository root is also the Notesmith Cargo workspace root, with Rust crates living under `crates/` alongside the planning, vault, and spike directories.
- Notesmith should also expose the vault to local AI clients through an MCP server (`notesmith mcp start`) so agents can create, read, search, route, and template notes over stdio without going through the HTTP daemon.

## Per-Customer Folder Structure

For each customer there would be a folder containing:

- **Internal meetings**
- **External meetings**
- **Account information**
  - Account information (note)
  - Glossary
  - Dates or Milestones
- **Projects or streams of work** for that customer

## High-Level Structure

- Capture
- Tasks (aggregated)
- Dashboards
- Customer 1
- Customer 2
- Customer N
- General
- Assets
  - templates
  - data

## Sidebar Views

- By default the sidebar shows only a **Files** tab (standard file/folder tree). No tab bar is rendered unless custom views are configured.
- Custom views are defined in `.notesmith/sidebar.yaml`. When ≥1 custom view exists, a tab bar appears with Files always first.
- Tabs are laid out in a **fixed 2-column grid** (icon + name per tab). Overflow wraps to additional rows.
- Each custom view contains **sections** stacked vertically with horizontal separators. Sections are **collapsible** and show **item count badges** on their headers. Three section types:

  1. **`recently-viewed`** — Shows recently viewed or edited notes. Mode (`viewed | edited | both`), tracked by the frontend (localStorage). Default limit: 10.
  2. **`custom-folders`** — Lists configured vault folders. Each folder renders its tree using the same FileTree component as the Files tab, rooted at that folder (leaf name displayed, full path as tooltip).
  3. **`custom-items`** — Each item has a name and icon (emoji). Clicking an item opens a **middle pane** between the sidebar and the reading pane. Two source variants:
     - `folder` source: lists notes in a folder (optionally recursive) with title + 2-line preview snippet.
     - `query` source: runs a SQL query and renders results using column mapping (`title_column`, `subtitle_column`, `badge_columns`).

- The **middle pane** is resizable (drag handle, default 300px, width persisted in localStorage). Only one custom item is active at a time. Clicking another replaces the pane content. An explicit close button dismisses the pane. Switching tabs closes it.
- Clicking a note in the middle pane opens it in the reading pane. For query-backed items (e.g., tasks), the reading pane scrolls to the relevant line.
- Views support an optional `badge_query` for tab-level badge counts (SQL-driven).
- Folder-backed items in the middle pane default to `modified_at DESC` sort, configurable via YAML.
- When `.notesmith/sidebar.yaml` does not exist, the app behaves as a plain Files-only notes app.
- The main note workspace should also include a contextual, collapsible **right rail** with tabbed **Metadata**, **Links**, and **TOC** modes for the active note.
- Notes can set `_icon:` in frontmatter to override their emoji in file trees, quick switchers, and editor tabs. Frontmatter keys prefixed with `_` are reserved for system/UI use and should stay hidden from metadata panels.
- The **TOC** tab should be driven from live editor headings so it can highlight the current section and jump the editor to a selected heading.

## Reactive Configuration

- Changes to vault-local config files under `.notesmith/` should be reflected without restarting the daemon or reloading the app window.
- The daemon should watch `.notesmith/sidebar.yaml` and `.notesmith/vault.toml` alongside note files, debounce rapid file-system events, and publish vault-scoped SSE events.
- Sidebar config is already loaded from disk on each `GET /api/v/{vault}/sidebar-config`; a sidebar config change only needs a `config.changed` SSE event so the frontend can refetch it.
- Vault config is cached in daemon state and must be reloaded in place after `.notesmith/vault.toml` changes. Invalid TOML should leave the last valid config active and publish a config error event.
- `vault.toml` should include a top-level `schema_version`; daemon loads must reject newer unknown schemas and automatically migrate older supported schemas before hot-swapping the in-memory config.
- Config events should include the config key (`sidebar` or `vault`), vault-relative path, status (`changed`, `removed`, or `error`), and an optional parse error message.
- The frontend should handle config events from the existing `/api/v/{vault}/events` stream: refetch sidebar config for `sidebar`, refresh app state derived from vault config for `vault`, and show non-blocking feedback for invalid config.

## Editor Experience

- The primary note surface should be an editable **CodeMirror 6** OFM editor, not a read-only HTML viewer.
- Notes should auto-save shortly after edits and support explicit save with the platform save shortcut.
- If a file changes on disk while the editor is clean, the editor should silently reload; if it is dirty, the editor should warn and let me reload or keep my in-memory edits.
- OFM affordances should stay visible while editing, especially wikilinks, tags, task checkboxes, callout markers, inline fields, and YAML frontmatter.
- Dashboard notes should stay as normal markdown files in the editor; fenced `notesmith` and `notesmith sql` blocks should execute read-only SQL against the cache and render inline result tables.

## Desktop App UX

- The primary desktop experience should be a three-pane app: sidebar on the left, tabbed editor workspace in the center, and a collapsible contextual right rail on the right.
- The note workspace should use tabs that persist across launches and remember each tab's current view mode.
- Each open tab should support three modes: **Source**, **Live Preview**, and **Reading View**, with a breadcrumb toolbar and a simple mode toggle in the header.
- The desktop shell should provide a command palette, quick switcher, and keyboard-first navigation for note creation, search, daily notes, capture, archiving, and view toggling.
- The app should use a dark theme by default, with five available themes: **Dark**, **Light**, **System** (follows OS preference), **Manuscript** (dark chrome + light editor), and **High Contrast** (black background, vivid colors). Theme preference should persist and apply without flash on load.
- Note creation, capture, and template workflows should use a **sequential input palette** (VS Code/Raycast style) instead of native browser prompts, which are broken in Tauri's WKWebView. Alerts and success messages should use non-blocking **toast notifications**.
- The app should use a `notesmith://app/...` URL scheme for deep links.

## Capture Workflow

- All captured notes start in the configured capture location (for example `Inbox/` if I want a dedicated capture folder).
- Once I am done working on a note, I move it to the appropriate folder for long-term storage.
- **Routing engine** (`.notesmith/routing.yaml`) automatically determines each note's destination based on frontmatter fields (`type`, `customer`, `meeting-kind`, `stream`) and moves notes with `notesmith route apply <path>`.
- Routed notes are stamped with `archived: true` and `archived-at` in frontmatter before moving.
- The goal is to keep the capture backlog at zero.

## Daily Notes

- Every morning I want a note for that day generated into the configured daily location (for example `Inbox/Daily/` if I want a dedicated folder). Primary creation should come from an external agent using a saved prompt template, with a daemon scheduler available as a fallback.
- Vault-specific agent instructions should live in `.notesmith/skill.md`, and saved prompt templates should live in `.notesmith/prompts/` so agents can assemble daily-note context consistently.
- Vault hooks can trigger external automation on note creation (`on_note_create`) and daily note creation (`on_daily_create`) without blocking the underlying Notesmith action.

## Tasks

- I want an aggregation of tasks from all notes to appear in a single place.
- The aggregated list should also show the associated project.
- Aggregated tasks should link to the stream note.
- The primary aggregated task list should show only **active** tasks (**To Do** and **In Progress**).
- There should be separate aggregated views for **Blocked**, **Awaiting Customer**, and **On Hold** tasks.
- Each meeting note optionally has tasks associated with it.
- A task can be associated with a stream of work (note).
- Tasks can have a status independent of the stream of work.
- Task statuses: **To Do**, **In Progress**, **Blocked**, **Awaiting Customer**, **On Hold**, **Done**, **Cancelled**.

## Streams of Work

- A stream of work has a status: **In Progress**, **Blocked**, **Done**, **Awaiting Customer**, **On Hold**.
- Tasks can be added to a stream of work regardless of its state.

## Assets / Resources

- Separate folder for assets/resources that are not notes but might be referenced in notes.

## Customer Folders

- Each customer folder would have the same structure, but can optionally add more folders or notes as needed.

## Customer State

- Customers have a state associated with them based on my relationship with them: **Active**, **On Hold**, **Temp**, **Inactive**.
- I will use these states to filter customer folders and smart views.
- Customer state should live in the Customer Index note frontmatter (`state:`), not in the Account Info note.

## Open for Ideas

- What additional customer metadata should live alongside `state:` on the Customer Index note.

## Git Integration

- Opt-in per-vault git sync via `[git]` in `vault.toml` (`enabled`, `auto_commit_every`, `auto_pull_every`, `auto_push_every`, `commit_message`).
- Auto-commit stages only note-relevant file types (`.md`, `.yaml`, `.yml`, `.toml`, `.json`, images, `.pdf`).
- Pull uses fast-forward only — conflicts abort and log a warning instead of attempting resolution.
- Auto-push always pulls first to minimize conflicts.
- CLI: `notesmith git {status, pull, push, sync, log}`.
- HTTP: `GET /api/v/{vault}/git/status`, `POST /api/v/{vault}/git/sync`.
- Non-git vaults are completely unaffected.

## Daemon Diagnostics

- The daemon should expose `GET /api/status` with version, API schema, uptime, vault note counts, watcher/index health placeholders, and resource diagnostics (RSS, open FDs, SSE connection count, cache size).
- On daemon start, Notesmith should run SQLite cache and Tantivy integrity checks, automatically move aside corrupt artifacts, and rebuild them from markdown files before serving the vault.
- Vaults reported by `GET /api/status` should surface a temporary `rebuilding` state while a manual reindex is in progress so the UI can show a rebuild banner.
- `GET /ping` remains as a lightweight compatibility alias for scripts, but richer clients should rely on `/api/status`.
- API and admin responses should include daemon version and schema headers so the frontend can detect incompatible client/daemon pairs, show a blue compatibility banner, and mark the sidebar status pill as restart-required until versions align.
- The daemon should write daily-rotated logs to the platform log directory, retain 7 days of history, and expose `GET /admin/logs?tail=` for local diagnostics.
- The desktop shell should show a bottom status bar with connection status, active vault, cursor position, word count, and save state. Clicking the connection section should open a popover with daemon health details and local controls for restart, reindex, and log tail viewing.
- CLI reindexing should be available as `notesmith reindex` with `--cache-only` and `--search-only` flags, defaulting to all registered vaults unless `--vault` is supplied.
- The daemon should write a JSON lockfile at the platform-specific Notesmith data/runtime location containing PID, port, version, start time, and binary path so desktop and other local clients can discover the live daemon and clean up stale entries.
- The daemon should watch the global config file and hot-reload vault registrations (add, remove, rename/path changes) without requiring a restart, emitting SSE `vaults.changed` so clients can refresh the vault list.
- Daemon-backed CLI commands should auto-start the HTTP daemon on first use when `[daemon].auto_start = true`, so workflows like capture, query, note CRUD, search, routing, templates, reindex, dailies, tasks, and `notesmith://` deep links do not require a manual `notesmith daemon start`.
- `notesmith mcp start` remains a standalone stdio server with its own in-memory indexes rather than proxying through the HTTP daemon.
