# Notesmith Domain Glossary

This file defines the domain vocabulary used throughout the Notesmith codebase. Use these terms consistently in code, comments, issues, and architectural discussions.

---

## Vault & Notes

- **Vault** — A rooted directory of markdown notes with a `.notesmith/` config folder. A user may have multiple vaults (e.g. `work`, `personal`). Each vault is independently configured and indexed.
- **Note** — A single markdown file in a vault. The canonical parsed representation including frontmatter, body, tasks, links, inline fields, and blocks. (`notesmith-core::Note`)
- **VaultPath** — A vault-relative path to a note, e.g. `Daily/2025-01-15.md`. Never absolute. (`notesmith-core::VaultPath`)
- **VaultName** — A short identifier for a vault, e.g. `work`. Used in API paths and config. (`notesmith-core::VaultName`)
- **VaultEngine** — The filesystem abstraction trait for scanning, reading, writing, deleting, and moving notes. (`notesmith-core::VaultEngine`)

## Frontmatter & Note Types

- **Frontmatter** — YAML metadata at the top of a note, delimited by `---`. Typed by a `type` field that determines the struct variant. (`notesmith-core::Frontmatter`)
- **NoteType** — The discriminator for frontmatter: `note`, `daily`, `meeting`, `stream`, `customer`, `account-info`, `glossary`, `milestones`, `dashboard`, `contact`.
- **CommonMeta** — Shared frontmatter fields across all note types: `tags`, `created`, `updated`, `archived`, `archived-at`.

## Customer Domain

- **Customer** — An external entity with its own folder containing meetings, account info, and streams. Customer state (Active, On Hold, Temp, Inactive) lives in the Customer Index note's frontmatter.
- **Stream** — A stream of work for a customer. Has status (In Progress, Blocked, Done, Awaiting Customer, On Hold) and priority (P0–P3). Tasks may be associated with a stream.
- **Meeting** — A customer interaction note, classified as internal or external (`MeetingKind`).

## Tasks

- **Task** — A checkbox item extracted from note content. Has status, priority, content, and a source position linking back to the note.
- **TaskStatus** — Seven states: Todo (`[ ]`), InProgress (`[/]`), Blocked (`[!]`), Waiting (`[>]`), OnHold (`[-]`), Done (`[x]`), Cancelled (`[~]`).
- **TaskPriority** — Lowest to Highest, parsed from task metadata.

## Links & Blocks

- **Link** — A parsed reference from one note to another. Types: wiki (`[[target]]`), embed (`![[target]]`), heading ref, block ref, anchor, markdown link, external.
- **InlineField** — A `key:: value` pair embedded in note text, used for metadata that doesn't belong in frontmatter.
- **Block** — A content block with an optional block ID (`^block-id`) for block-level references.
- **SourcePosition** — Line/column/offset/length anchor for any parsed syntax element back to its source location.
- **Backlink** — An inverse link: "which notes link to this note?"

## Indexing & Search

- **NoteIndex** — The in-memory SQLite cache of all parsed notes, tasks, links, and frontmatter. Rebuilt on startup, incrementally updated on file changes. (`notesmith-index`)
- **SearchIndex** — The Tantivy full-text search index alongside the NoteIndex. (`notesmith-index`)

## Configuration

- **VaultConfig** — Per-vault settings in `.notesmith/vault.toml`. Sections: schema version, capture, daily, editor, git, hooks.
- **GlobalConfig** — App-wide settings in `~/.config/notesmith/config.toml`. Contains daemon bind address, CLI auto-start policy, and the vault registry.
- **SidebarConfig** — Per-vault sidebar view definitions in `.notesmith/sidebar.yaml`. Defines custom views with sections (recently-viewed, custom-folders, custom-items).

## Capture & Routing

- **Capture** — The quick-capture workflow that writes timestamped notes to the configured capture folder. When `capture.folder = ""`, captures land in the vault root.
- **Routing** — Rule-based note filing from captured or draft notes to destination folders. Rules match on frontmatter fields (type, customer, meeting-kind, stream). Defined in `.notesmith/routing.yaml`.
- **Archive** — The act of routing a note: stamping `archived: true` and `archived-at` in frontmatter, then moving to the destination folder.
- **Daemon-backed CLI commands** — `capture`, `query`, `note`, `search`, `template`, `route`, `daily`, `task`, `reindex`, and daemon-backed `notesmith://` handlers. They probe `/api/status` and auto-start the HTTP daemon when `[daemon].auto_start = true`.

## Templates

- **Template** — A Minijinja-based file in `.notesmith/templates/` with metadata (name, description, output path pattern) and prompt specs. (`notesmith-templates::TemplateEngine`)
- **PromptSpec** — A named parameter a template requires at instantiation time (e.g. "customer name", "meeting date").
- **RenderedTemplate** — The output of template instantiation: a resolved path and rendered content.

## Daily Notes

- **Daily Note** — A date-stamped note generated into the configured daily folder (default: vault root when `daily.folder = ""`). Can be created by the scheduler, CLI, API, or an external agent.
- **Catch-up** — Backfilling missing daily notes for recent days when `catch_up: true` in DailyConfig.
- **DailyScheduler** — Background task that auto-generates daily notes at a configured time.
- **MCP server** — The `notesmith mcp start` stdio server. It builds its own in-memory indexes for local MCP clients rather than proxying through the HTTP daemon.

## Runtime & Events

- **Daemon** — The HTTP server process (`notesmith daemon start`) that serves the API, SSE events, and static frontend.
- **VaultState** — Per-vault runtime state held by the daemon: cache, search index, engine, root path, config (ArcSwap), template engine.
- **AppState** — Global daemon state containing all VaultStates and shared config.
- **VaultEvent** — An SSE event broadcast when something changes in a vault: note CRUD, task updates, config changes, cache rebuilds.
- **VaultWatcher** — A filesystem watcher (notify crate) that detects file changes, classifies them (note vs config), debounces, and emits VaultEvents.

## Save Pipeline

- **SavePipeline** — Pre-write normalization applied to note content: frontmatter stamping (created/updated timestamps), YAML key sorting, trailing whitespace cleanup. (`notesmith-vault::apply_save_pipeline`)

## Security & Conflict Detection

- **WriteGuard** — An Axum extractor that checks the `Origin` header on write requests. Allows localhost and Tauri origins; rejects foreign origins.
- **ETag** — A BLAKE3 hash of config file content used for optimistic concurrency. GET returns it; PUT requires `If-Match`.
- **Capabilities** — A server-driven feature flags endpoint (`GET /api/capabilities`) that tells the frontend what the deployment supports (desktop vs hosted, config editing, local path opening).

## Frontend

- **Design Tokens** — Current components still consume legacy `--ns-*` tokens from `ui/app/src/app.css`, but the new theme engine contract now lives in `ui/app/src/styles/`: ramp primitives (`--neutral-*`, `--blue-*`, etc.) map into bare semantic tokens such as `--bg-default` and `--text-default`, with a separate high-contrast overlay file. Components should migrate toward the semantic layer instead of defining ad-hoc colors.
- **Theme System** — Five themes: Dark (default), Light, System (follows OS), Manuscript (dark chrome + light editor), High Contrast (pure black, cyan borders). Theme choice stored in `localStorage` under `notesmith:theme`. A blocking inline script in `app.html` applies the theme class before paint to prevent flash. Managed by `theme.svelte.ts` (Svelte 5 runes store). The incoming theme engine also layers `data-theme`, `data-tone`, and optional `data-mode` attributes on `<html>` so generated ramp CSS can feed the semantic tokens.
- **Settings Page** — A dedicated `/settings` route with left sidebar navigation and right content area for editing vault config in-app. Sections: General (name, homepage, capture folder/template), Daily Notes, Editor, Git, Hooks, Appearance (theme picker). Per-section Save/Revert with ETag-based conflict detection.
- **Right Rail** — A collapsible tabbed panel (`⌘\`) with three modes: **Metadata** (frontmatter key/values, `_`-prefixed keys hidden), **Links** (backlinks + outgoing), **TOC** (live table of contents from editor headings with click-to-scroll and active heading highlight). Tab selection persists in `localStorage`.
- **Middle Pane** — A resizable panel between sidebar and editor, opened by custom sidebar items to show folder listings or query results.
- **Folder Notes** — Same-name markdown notes (`Folder/Folder.md`) represented by their folder row in FileTree. Folder rows open the hidden folder note from the name control, expand from the chevron, and support creation/rename actions from the context menu.
- **Command Palette** — A fuzzy-searchable overlay for executing commands (⌘K / ⌘P). Shows footer hints for keyboard navigation.
- **Input Palette** — A sequential multi-step input overlay (same chrome as Command Palette) used for note creation, capture, and template instantiation. Supports text input and fuzzy list picker modes. Replaces `window.prompt()` which is broken in Tauri's WKWebView.
- **Toast Stack** — Non-blocking corner notifications (success/error/warning) with auto-dismiss. Replaces `window.alert()`. Managed by `toast-store.svelte.ts`.
- **Quick Switcher** — A fuzzy note search overlay for rapid navigation (⌘O).
- **Status Bar** — A 28px bar at the bottom of the app showing: connection status with diagnostic popover (left), vault name (center), cursor position, word count, and save indicator (right). Editor state shared via `editor-status.svelte.ts`.
- **Note Icons** — Notes can set `_icon:` (emoji) in frontmatter to override their icon in file trees, tabs, and quick switcher. Falls back to type-based defaults. `_`-prefixed frontmatter keys are reserved for system/UI use and hidden from metadata panels.
