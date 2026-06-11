# Notesmith Domain Glossary

This file defines the domain vocabulary used throughout the Notesmith codebase. Use these terms consistently in code, comments, issues, and architectural discussions.

---

## Vault & Notes

- **Vault** — A rooted directory of markdown notes with a `.notesmith/` config folder. A user may have multiple vaults (e.g. `work`, `personal`). Each vault is independently configured and indexed.
- **Note** — A single markdown file in a vault. The canonical parsed representation including frontmatter, body, tasks, links, inline fields, and blocks. (`notesmith-core::Note`)
- **VaultPath** — A vault-relative path to a note, e.g. `Daily/2025-01-15.md`. Never absolute. (`notesmith-core::VaultPath`)
- **VaultName** — A short identifier for a vault, e.g. `work`. Used in API paths and config. (`notesmith-core::VaultName`)
- **VaultEngine** — The filesystem abstraction trait for scanning, reading, writing, deleting, and moving notes. (`notesmith-core::VaultEngine`)

## Frontmatter & Fields

- **Frontmatter** — YAML metadata at the top of a note, delimited by `---`. Parsed generically as key-value pairs (no typed variants). (`notesmith-core::Frontmatter` → `HashMap<String, Value>`)
- **Field** — A key-value pair associated with a note. Sources: frontmatter YAML or inline `[key:: value]` syntax. All fields are stored uniformly in the `fields` table with no source distinction in queries.
- **Tag** — A label associated with a note. Sources: `tags:` frontmatter array or inline `#hashtag` syntax. Stored in a dedicated `tags` table for ergonomic multi-value queries.
- **Field Registry** — Advisory field definitions in `.notesmith/fields.toml`. Specifies type, allowed values, and autocomplete sources for each field key. Not enforced — invalid values produce warnings.

## Tasks

- **Task** — A checkbox item extracted from note content. Has status character, status group, text, and a source position linking back to the note.
- **TaskStatus** — Configurable and extensible. Each status is a single character (e.g. `x`, `/`, `!`) mapped to a label, group (`open` or `done`), and icon via `[task_statuses]` in `vault.toml`. Default ships the standard OFM set: `[ ]`=Todo, `[x]`=Done, `[/]`=InProgress, etc.
- **Task Fields** — Inline fields associated with individual tasks (e.g. `[due:: 2026-06-01]` on a task line). Stored in a `task_fields` table.

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

- **VaultConfig** — Per-vault settings in `.notesmith/vault.toml`. Sections: schema version, capture, periodic notes, editor, git, hooks, task statuses.
- **GlobalConfig** — App-wide settings in `~/.config/notesmith/config.toml`. Contains daemon bind address, CLI auto-start policy, and the vault registry.
- **SidebarConfig** — Per-vault sidebar view definitions in `.notesmith/sidebar.yaml`. Defines custom views with sections (recently-viewed, custom-folders, custom-items).
- **RoutingConfig** — Per-vault routing rules in `.notesmith/routing.yaml`. Expressive YAML DSL with boolean combinators (all/any/not), field/tag predicates, and full mutations (move, set/remove fields, add/remove tags).
- **FieldRegistry** — Per-vault field definitions in `.notesmith/fields.toml`. Advisory type/value constraints for autocomplete and validation.
- **UserViews** — Per-vault SQL view definitions in `.notesmith/views.sql`. Creates persistent views in the cache database for dashboard blocks and queries.

## Capture & Routing

- **Capture** — A first-class command that writes timestamped notes to the configured capture folder. Delegates to the template system internally. When `capture.folder = ""`, captures land in the vault root.
- **Routing** — Rule-based note filing using an expressive YAML DSL (`.notesmith/routing.yaml`). Rules match on field values, tag presence/absence, path globs, and boolean combinators (all/any/not). Mutations: move_to, set_fields, remove_fields, add_tags, remove_tags. Supports both manual trigger (`notesmith route apply`) and auto-routing (opt-in per rule).
- **Route Log** — An append-only audit table recording every routing operation (from_path, to_path, rule_id, mutations). Enables undo via `notesmith route undo`.
- **Daemon-backed CLI commands** — `capture`, `query`, `note`, `search`, `template`, `route`, `periodic`, `task`, `reindex`, and daemon-backed `notesmith://` handlers. They probe `/api/status` and auto-start the HTTP daemon when `[daemon].auto_start = true`.

## Templates

- **Template** — A Minijinja-backed markdown file in `.notesmith/templates/` (legacy `Assets/templates/` is still supported) with metadata (name, description, output path pattern), prompt specs, optional context_queries (SQL), and optional pre_render_hook (script). (`notesmith-templates::TemplateEngine`)
- **PromptSpec** — A named parameter a template requires at instantiation time (e.g. "customer name", "meeting date"). Types: text, field-picker, date.
- **RenderedTemplate** — The output of template instantiation: a resolved path and rendered content.
- **Context Layers** — Three layers of template context: (1) static variables (date, vault, filename), (2) SQL context_queries against the cache, (3) pre_render_hook script enrichment.

## Periodic Notes

- **Periodic Note** — A note tied to a time period (daily, weekly, monthly, quarterly, yearly). Generated into configured folders from templates.
- **Period Kinds** — Five types: daily (`YYYY-MM-DD`), weekly (`YYYY-Www`), monthly (`YYYY-MM`), quarterly (`YYYY-Qq`), yearly (`YYYY`). Each configured independently in `vault.toml` with folder, template, and filename pattern.
- **DailyScheduler** — Background task that auto-generates daily notes at a configured time (also available for other period kinds).

## Runtime & Events

- **Daemon** — The HTTP server process (`notesmith daemon start`) that serves the API, SSE events, and static frontend when frontend assets are available.
- **Container image flavors** — GHCR publishes an `app` flavor (`latest`, `sha-*`, date tags) with the SvelteKit frontend at `/app-ui` for browser `/app/` access, and an `api` flavor (`api-latest`, `api-sha-*`, date tags) with only the Rust binary. The Tauri desktop can use either flavor via `NOTESMITH_DESKTOP_DAEMON_URL` because remote-daemon mode serves the frontend from embedded desktop assets.
- **MCP server** — The `notesmith mcp start` stdio server. It maps MCP tool/resource requests onto the shared `notesmith-ops` operations layer (`NotesmithMcp` wraps a `LocalOps`); it currently builds its own in-memory indexes rather than proxying through the HTTP daemon (daemon-hosted MCP is planned — ADR 0010).
- **Ops layer** — `notesmith-ops` defines the canonical vault-operation surface. `Ops` is the trait (read + write methods); `LocalOps` is the in-process implementation backed by engine/cache/search index/template engine; `ReadOnlyOps<O>` wraps any `Ops` and rejects every write (used to expose read-only agent surfaces without auth). See ADR 0010.
- **VaultState** — Per-vault runtime state held by the daemon: cache, search index, engine, root path, config (ArcSwap), template engine.
- **AppState** — Global daemon state containing all VaultStates and shared config.
- **VaultEvent** — An SSE event broadcast when something changes in a vault: note CRUD, task updates, config changes, cache rebuilds.
- **VaultWatcher** — A filesystem watcher (notify crate) that detects file changes, classifies them (note vs config), debounces, and emits VaultEvents.

## Hook System

- **Hook** — An external command triggered by a vault event. Receives JSON payload via stdin. Failures never block the triggering operation.
- **Hook Events** — Six events: `on_note_create`, `on_note_update`, `on_note_route`, `on_periodic_create`, `on_task_change`, `on_field_change`.
- **on_field_change** — Scoped to `watch_fields` list. Batched per save (one invocation with all field changes). Each change has an `action` discriminator: `add`, `change`, `remove`.

## Save Pipeline

- **SavePipeline** — Pre-write normalization applied to note content: frontmatter stamping (created/updated timestamps), YAML key sorting, trailing whitespace cleanup. (`notesmith-vault::apply_save_pipeline`)

## Security & Conflict Detection

- **WriteGuard** — An Axum extractor that checks the `Origin` header on write requests. Allows localhost, `tauri://localhost`, `notesmith-app://localhost`, and `http(s)://notesmith-app.localhost` origins; rejects foreign origins.
- **ETag** — A BLAKE3 hash of config file content used for optimistic concurrency. GET returns it; PUT requires `If-Match`.
- **Capabilities** — A server-driven feature flags endpoint (`GET /api/capabilities`) that tells the frontend what the deployment supports (desktop vs hosted, config editing, local path opening).

## Frontend

- **Design Tokens** — Current components still consume legacy `--ns-*` tokens from `ui/app/src/app.css`, but the new theme engine contract now lives in `ui/app/src/styles/`: the theme catalog is `ui/app/src/styles/theme-catalog.json`, `crates/theme-gen` precomputes `ui/app/src/styles/themes/*.css`, and those ramp primitives (`--neutral-*`, `--blue-*`, etc.) map into bare semantic tokens such as `--bg-default` and `--text-default`, with a separate high-contrast overlay file. Components should migrate toward the semantic layer instead of defining ad-hoc colors.
- **Theme System** — Theme state lives in `localStorage` under `notesmith:theme` as `{ theme, followSystem, darkTheme, lightTheme, visualMode }`. A blocking inline script in `app.html` resolves the active theme before paint, applies layered `data-theme`, `data-tone`, and `data-mode` attributes on `<html>` to prevent flash, and `theme.svelte.ts` later corrects `data-tone` from the real catalog lookup while keeping those attributes in sync at runtime.
- **Settings Page** — A dedicated `/settings` route with left sidebar navigation and right content area for editing vault config in-app. Sections: General (name, homepage, capture folder/template), Daily Notes, Editor, Git, Hooks, Appearance (flat visual theme gallery with live preview, optional follow-system dark/light pair selectors, and a high-contrast toggle). Per-section Save/Revert with ETag-based conflict detection.
- **Right Rail** — A collapsible tabbed panel (`⌘\`) with three modes: **Metadata** (frontmatter key/values, `_`-prefixed keys hidden), **Links** (backlinks + outgoing), **TOC** (live table of contents from editor headings with click-to-scroll and active heading highlight). Tab selection persists in `localStorage`.
- **Middle Pane** — A resizable panel between sidebar and editor, opened by custom sidebar items to show folder listings or query results.
- **Folder Notes** — Same-name markdown notes (`Folder/Folder.md`) represented by their folder row in FileTree. Folder rows open the hidden folder note from the name control, expand from the chevron, and support creation/rename actions from the context menu.
- **Command Palette** — A fuzzy-searchable overlay for executing commands (⌘K / ⌘P). Shows footer hints for keyboard navigation and can switch into a theme-picker sub-mode that previews catalog themes before confirmation.
- **Input Palette** — A sequential multi-step input overlay (same chrome as Command Palette) used for note creation, capture, and template instantiation. Supports text input and fuzzy list picker modes. Replaces `window.prompt()` which is broken in Tauri's WKWebView.
- **Toast Stack** — Non-blocking corner notifications (success/error/warning) with auto-dismiss. Replaces `window.alert()`. Managed by `toast-store.svelte.ts`.
- **Quick Switcher** — A fuzzy note search overlay for rapid navigation (⌘O).
- **Vault Menu** — Browser-only dropdown anchored on the vault name in the workspace chrome (`VaultMenu.svelte`), exposing Switch Vault, Add Vault, and Settings. Rendered only when the Tauri runtime is absent (`isBrowserVaultMenu`); the desktop app uses its native OS menu and window-per-vault navigation instead. Pure helpers live in `ui/app/src/lib/vault-menu.ts`.
- **Status Bar** — A 28px bar at the bottom of the app showing: connection status with diagnostic popover (left), vault name (center), cursor position, word count, and save indicator (right). Editor state shared via `editor-status.svelte.ts`.
- **Note Icons** — Notes can set `_icon:` (emoji) in frontmatter to override their icon in file trees, tabs, and quick switcher. Falls back to type-based defaults. `_`-prefixed frontmatter keys are reserved for system/UI use and hidden from metadata panels.
