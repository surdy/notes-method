# Notes Method

A method for organizing notes using a generic, programmable workspace. The structure and workflows are user-defined through configuration — not hardcoded into the application.

## Context

This method is implemented by **Notesmith**, a custom markdown notes app. The definitive application blueprint is `plans/notesmith-plan.md`.
The repository root is also the Notesmith Cargo workspace root, with Rust crates living under `crates/` alongside the planning, vault, and spike directories.
- Notesmith exposes the vault to AI clients through MCP. The daemon hosts per-vault MCP endpoints over HTTP/SSE, and `notesmith mcp start` is a stdio↔HTTP bridge to them, so stdio-only clients (e.g. Claude Desktop) and HTTP clients share the daemon's live indexes and can create, read, search, route, and template notes.

## Core Principles

1. **Notes are plain markdown** — Notesmith never modifies your files in ways that break other editors.
2. **Structure emerges from metadata** — Fields, tags, and links create relationships without rigid folder hierarchies.
3. **Configuration, not code** — Workflows are defined via YAML/TOML/SQL files in `.notesmith/`, not compiled into the app.
4. **Only `.notesmith/` is required** — All other paths and folder structure are user-defined.
5. **Fresh start** — No migration from previous schemas (pre-v1).

## Data Primitives

| Primitive | Description |
|-----------|-------------|
| **Note** | A markdown file with optional frontmatter |
| **Field** | Key-value metadata (from frontmatter or inline `[key:: value]`) |
| **Tag** | Labels (from `tags:` frontmatter or inline `#hashtag`) |
| **Task** | Checkbox item with configurable status characters |
| **Link** | Wikilink, embed, or markdown link between notes |
| **Periodic Note** | Date-bound note (daily, weekly, monthly, quarterly, yearly) |

## Vault Structure

Only `.notesmith/` is required. Everything else is user-defined:

```text
.notesmith/
  vault.toml          # Vault configuration
  fields.toml         # Field registry (autocomplete, validation)
  routing.yaml        # Routing rules
  views.sql           # User-defined SQL views
  sidebar.yaml        # Sidebar view definitions
  templates/          # Note templates
  prompts/            # AI prompt templates
  skill.md            # AI agent context
<user-defined folders and notes>
```

The blessed customer-facing work configuration (meetings/customers/streams/people and the query recipes the search primitives are built for) is `docs/example-work-notes-kit.md`; its design record is `plans/work-notes-simplification-design.md`.

## Sidebar Views

- By default the sidebar shows only a **Files** tab. Its Quill Rail file tree is a quiet, folder-icon-free typographic outline: disclosure carets identify folders, while hairline vertical rails and short Branch Spine connectors make deep hierarchy legible. The selected note uses a narrow accent spine. No tab bar is rendered unless custom views are configured.
- The top of the Files tab has one compact toolbar: note search, **New Note**, **New Folder**, and **Refresh**. New Note collects a title and destination folder through the shared input palette. New Folder creates the folder's same-name folder note so it appears immediately in the notes-derived tree; entering an existing folder opens its folder note.
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
- The main note workspace should also include a contextual, collapsible **right dock** on the right. The dock presents a single unified tab row — **Metadata**, **Links**, and **TOC** (the **Context** surface for the active note) plus **Chat** (the embedded AI agent). Both surfaces share one column so opening chat never squeezes the editor with a second panel, and the last-used tab is remembered per vault. The **Links** tab also surfaces a **Relevant** section: notes related to the active note, ranked by embedding similarity blended with link-graph proximity (degrading to graph-only when the vault has no embeddings).
- Notes can set `_icon:` in frontmatter to show a custom emoji in file trees, quick switchers, and editor tabs. The Files tree omits a generic document icon when `_icon` is absent so the Quill Rail stays typographic; other surfaces may use a generic fallback. Frontmatter keys prefixed with `_` are reserved for system/UI use and should stay hidden from metadata panels.
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
- Reading and preview rendering should default to Obsidian-style single-newline line breaks, with an editor config toggle to restore strict CommonMark line-break behavior when desired.
- Source and Live Preview editor modes should support a setting to show or hide CodeMirror line numbers, defaulting to visible.
- Live Preview should render markdown tables as editable table widgets: cell text can be edited visually, and rows/columns can be appended or removed without dropping into raw pipe syntax.
- Reading View and Live Preview should syntax-highlight fenced code blocks when the fence declares a supported language. Unsupported or unlabeled fences should remain escaped plain code. In Live Preview, rendered code blocks should switch back to editable source when the cursor enters the fence or the rendered block is clicked.
- Live Preview should render callout blocks with the same built-in Obsidian-style callout chrome as Reading View when the cursor is outside the block. When the cursor enters any line in the callout block, or when the rendered callout is clicked, the whole block should switch back to editable source text.
- Reading View callouts should follow Obsidian's built-in callout behavior, including supported type identifiers and aliases, note fallback for unsupported types, custom titles, title-only callouts, nested callouts, and foldable callouts. Custom callout CSS/plugin definitions are out of scope for now.
- Dashboard notes should stay as normal markdown files in the editor; fenced `notesmith` and `notesmith sql` blocks should execute read-only SQL against the cache and render inline result tables.

## Desktop App UX

- The primary desktop experience should be a three-pane app: sidebar on the left, tabbed editor workspace in the center, and a collapsible right dock (Context + AI Chat) on the right.
- The top workspace chrome should be a single Obsidian-like bar spanning the left sidebar, editor tabs, and right dock. Sidebar show/hide controls belong in that bar, not as floating affordances that overlap the editor, and should use panel-left/panel-right icons instead of directional arrow glyphs.
- The note workspace should use tabs that persist across launches and remember each tab's current view mode.
- Each open tab should support three modes: **Source**, **Live Preview**, and **Reading View**, with a breadcrumb toolbar and a simple mode toggle in the header.
- Folder notes should follow the same-name markdown convention: a folder note for `Customers/Acme/` is `Customers/Acme/Acme.md`. Matching is exact and markdown-only; dot-prefixed folders such as `.notesmith/` are excluded, and there is no vault-root folder-note concept.
- In every shared FileTree surface, including custom folder views, a folder with a same-name folder note should open that note when the folder name is clicked, while the disclosure chevron expands or collapses the folder. Folders without folder notes keep the current expand/collapse row behavior.
- Folder-note files represented by a folder row should be hidden from that tree position to avoid duplicates, but remain normal notes in search, quick switcher, tabs, backlinks, and other non-tree surfaces. When a hidden folder note is active, its folder row should show selected styling; no extra folder-note badge or underline is needed initially.
- Folder-note creation should be available from both a command-palette flow and a folder right-click context menu. The command should use a searchable picker of existing folders. New folder notes start with an H1 matching the folder name and no special frontmatter; creating an already-existing folder note opens it and shows a non-blocking toast.
- The folder context menu should initially include only folder-note open/create actions and **Rename Folder**. Folder rename should sync the same-name folder-note filename for Notesmith-initiated renames, block the rename if the synced target filename would collide, and should not rewrite wikilinks or embeds.
- Folder overview code blocks, bulk folder-note conversion, arbitrary non-markdown folder-note file types, external filesystem rename inference, and breadcrumb/path-segment folder-note opening are out of scope for the first folder-notes project.
- The desktop shell should provide a command palette, quick switcher, and keyboard-first navigation for note creation, search, daily notes, capture, archiving, view toggling, and theme switching.
- The desktop app should launch **local-only by default** and let users connect to one or more remote daemons without environment variables. Servers (name, URL, optional token) are managed in **Settings → Connection** — the system of record. Connections are **per-window** (ADR 0017): the "New Window" menu lists vaults grouped by server, picking one opens a window bound to that server, and the status-bar **badge** shows that window's own connection (local/remote, live/offline) — local and remote vaults can be open side by side. When a window is connected to a remote server, the shell serves its embedded SvelteKit assets locally and sends API/SSE traffic to that daemon, so desktop clients work with the binary-only `api` container flavor while the `app` flavor remains necessary for browser `/app/` access. The saved `active_id` is a non-destructive default for new windows; destructive server edits (URL change, removal) are blocked while windows are open against that server.
- In remote-daemon mode, vault management should operate on the remote daemon's registry. Creating or adding a vault should call the remote API with a server/container path, and the UI should not present a local folder picker for remote vault paths.
- Removing a vault should unregister it by default without deleting markdown files. Destructive file deletion should be an explicit opt-in in the confirmation flow and should delete the daemon-side vault folder only when selected.
- In browser (hosted) mode, where there is no native OS menu or window-per-vault model, the vault name in the workspace chrome should act as a dropdown exposing Switch Vault, Add Vault, and Settings. This menu is browser-only (detected by the absence of the Tauri runtime); the desktop app keeps its native menu and window-per-vault navigation unchanged.
- Notesmith should ship exactly three carefully tuned themes rather than a broad catalog: **Dark** (the Graphite Precision direction), **Light** (the Porcelain direction), and **Split** (the Studio direction, with dark sidebars/chrome around a light editor). Settings presents them as a compact three-option selector with representative workspace previews, concise descriptions, and no catalog-style author/tag metadata. New vaults continue to follow the operating-system appearance by default, using Dark and Light; Dark is the no-flash fallback before vault config loads. Appearance state remains split into a manual theme (`theme`), an optional follow-system pairing (`followSystem`, `darkTheme`, `lightTheme`), and a visual mode (`data-mode`, including a high-contrast overlay). Follow-system may use Dark or Split for the dark appearance and Light for the light appearance; the settings UI therefore offers a compact Dark/Split choice and presents Light as fixed rather than as a redundant one-option selector. The active theme drives `data-theme`, and `data-tone` is derived from the active catalog entry. Theme preference persists in both `localStorage` and the vault's `[appearance]` config and applies without flash on load. Saved themes removed by this simplification migrate by their former role: dark themes to Dark, light themes to Light, and the old Manuscript split-surface theme to Split.
- The theme engine maps generated ramp primitives (`--neutral-*`, `--blue-*`, etc.) into a small shared semantic-token contract (`--bg-default`, `--text-default`, `--accent`, and peers). The three-entry catalog lives at `ui/app/src/styles/theme-catalog.json`, and the `theme-gen` workspace binary precomputes `dark.css`, `light.css`, and `split.css` with 12-step OKLCH/OKLab ramps. Each catalog entry also supplies explicit semantic surface and border values so sidebars, chrome, tabs, inputs, and separators match the approved design instead of being approximated from evenly spaced neutral-ramp steps. Split additionally supplies an explicit editor palette and editor-surface semantics so its light middle pane is tuned independently from its dark chrome. High-contrast remains a final overlay on top of generated theme values and explicitly re-scopes itself inside Split's editor surface; `--text-inverse` is the one tone-specific exception because badge/accent backgrounds can flip polarity between dark and light themes.
- Note creation, capture, and template workflows should use a **sequential input palette** (VS Code/Raycast style) instead of native browser prompts, which are broken in Tauri's WKWebView. Alerts and success messages should use non-blocking **toast notifications**.
- The app should use a `notesmith://app/...` URL scheme for deep links.

## Capture Workflow

- All captured notes start in the configured capture location (configurable in `vault.toml`).
- **Web clipping** extends capture to web pages: the daemon fetches a URL (SSRF-guarded, bounded), extracts the readable article server-side, and writes a Markdown note with provenance frontmatter (`source_url`, `source_type: article`, …) tagged `inbox` so the routing engine files it like any other capture. Clips are deduplicated by canonical `source_url`, can download images into the vault, and support per-domain templates (`[clip]` config). Triggers: `POST /clip`, `notesmith clip <url>`, and a Manifest V3 browser extension (`ui/extension/`). See `docs/adr/0020-web-clipper.md`.
- Once a note is enriched with fields and tags, it can be routed to its permanent location.
- **Routing engine** (`.notesmith/routing.yaml`) determines each note's destination based on field values, tag presence, and path globs. Supports full mutations: move, set/remove fields, add/remove tags.
- Both manual routing (`notesmith route apply <path>`) and auto-routing (opt-in per rule) are supported.
- All routing operations are logged in `route_log` for audit and undo.
- The goal is to keep the capture backlog at zero.

## Periodic Notes

- Configurable periodic notes for all time periods: daily, weekly, monthly, quarterly, yearly.
- Each period kind has its own folder, template, and filename pattern configured in `vault.toml`.
- Primary creation via CLI (`notesmith periodic open <kind>`), API, or external agent.
- Vault-specific agent instructions live in `.notesmith/skill.md`, and saved prompt templates in `.notesmith/prompts/`.
- Vault hooks can trigger external automation on note creation (`on_note_create`) and periodic note creation (`on_periodic_create`) without blocking the underlying action.

## Tasks

- Aggregated task views collect tasks from all notes into a single surface.
- Task statuses are configurable: each status is a single character mapped to a label, group (open/done), and icon via `vault.toml`.
- Default statuses: `[ ]`=Todo, `[x]`=Done, `[/]`=InProgress, `[b]`=Blocked, `[w]`=Waiting, `[h]`=On Hold, `[-]`=Cancelled.
- Users can add custom status characters without code changes.
- Tasks can have inline fields (e.g., `[due:: 2026-06-01]`, `[assigned:: me]`).
- Aggregated views group by status_group (open/done) and show associated fields.

## Git Integration

- Opt-in per-vault git sync via `[git]` in `vault.toml` (`enabled`, `auto_commit_every`, `auto_pull_every`, `auto_push_every`, `commit_message`, `commit_on_inactivity`).
- Auto-commit stages only note-relevant file types (`.md`, `.yaml`, `.yml`, `.toml`, `.json`, images, `.pdf`).
- Local-only versioning is supported: leave the remote-sync intervals (`auto_pull_every`, `auto_push_every`) empty and only commits happen — no `origin` required.
- Enabling git auto-initializes the repository: if the vault isn't a git repo yet, saving `enabled = true` runs `git init`, scaffolds a minimal `.gitignore` (OS cruft only; notes and `.notesmith/` stay tracked), and records an initial commit. It is idempotent and also available as `POST /api/v/{vault}/git/init`.
- Tolaria-style inactivity checkpoints: `commit_on_inactivity` (e.g. `2m`) commits automatically once edits have been idle for that window. The desktop flushes the editor buffer to disk first; a headless daemon timer covers non-desktop clients.
- When `commit_message` is unset, the message is generated from the changed-file list (e.g. `Update note-a.md, note-b.md and 3 more`).
- Pull uses fast-forward only — conflicts abort and log a warning instead of attempting resolution.
- Auto-push always pulls first to minimize conflicts.
- CLI: `notesmith git {status, pull, push, sync, log}`.
- HTTP: `GET /api/v/{vault}/git/status`, `POST /api/v/{vault}/git/init`, `GET /api/v/{vault}/git/log`, `GET /api/v/{vault}/git/diff/{sha}`, `POST /api/v/{vault}/git/sync`, `POST /api/v/{vault}/git/commit`.
- The desktop status bar shows a changed-files badge when git is enabled; clicking it opens a git-history view (commit list + per-commit diff) with a "Commit now" action.
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
- `notesmith mcp start` is a stdio↔HTTP bridge to the daemon's MCP endpoint rather than a standalone server with its own in-memory indexes. It resolves a daemon URL (the global `--url`/`NOTESMITH_URL` when set, otherwise the local daemon, auto-started on demand), connects to `/mcp/<vault>` or `/mcp-ro/<vault>`, and forwards every stdio request, so all MCP clients share the daemon's live indexes and the shared `notesmith-ops` operation logic.
- The CLI can target a remote daemon via a global `--url` flag or the `NOTESMITH_URL` env var (the flag wins), overriding the configured local bind for all daemon-backed commands and `mcp start`. A remote target is used verbatim and never auto-started (TLS terminated by a reverse proxy; reverse-proxy subpaths supported); the `daemon` lifecycle subcommands always manage the local daemon.

## Agent Access Architecture

- All vault operations are defined once by the `notesmith-ops` crate: an `Ops` trait (read + write methods), a `LocalOps` in-process implementation backed by the engine/cache/search index/template engine, and a `ReadOnlyOps<O>` wrapper that rejects every write so a read-only agent surface can be exposed without authentication.
- The target architecture (see `docs/adr/0010-agent-access-architecture.md`) makes the daemon the single source of truth and turns every adapter (MCP, CLI) into a thin client of a daemon — local or remote. **Implemented:** daemon-hosted MCP over HTTP/SSE at per-vault paths `/mcp/<vault>` (full) and `/mcp-ro/<vault>` (read-only), reusing the daemon's live indexes (`LocalOps::from_shared`); a `notesmith mcp start` stdio↔HTTP bridge for stdio-only clients (the embedded in-memory engine path has been removed); and a CLI remote profile (global `--url`/`NOTESMITH_URL`) that retargets daemon-backed commands at a remote daemon. The per-vault endpoints route on the `{vault}` path parameter and resolve against the daemon's shared state per request, so a vault added after startup is reachable without a restart. **Planned:** authentication and per-identity scopes (Phase 5), deferred under the LAN/VPN trust model.
- Read-only agent operation is selected by which endpoint an agent connects to, not by identity; it guards against agent mistakes, not malicious actors. Authentication and per-identity scopes are deferred under the LAN/VPN trust model, with TLS terminated by a reverse proxy (disable SSE buffering for `/mcp` paths).

### AI Integration Roadmap

- The forward plan for AI features follows one principle (see `docs/adr/0015-ai-agent-integration-roadmap.md`): **the daemon does the heavy lifting, exposes it as an MCP tool, and the user's ACP agent decides when to call it — Notesmith never runs its own chat LLM.** Client-side surfaces (slash commands, inline editor commands, context pills) compose prompts and talk to the agent directly over ACP; data/retrieval/ingestion features ship as MCP tools. The only daemon-adjacent model is a local *embeddings* model (ADR 0018): a colocated `notesmith embed` worker owns `embeddings.db` and the daemon embeds queries and reads it read-only. This backend now ships (local/offline by default; a real `bge-small-en-v1.5` model behind the `local-embed` feature, a non-semantic hash embedder otherwise), with cloud embedders still deferred.
- Phases (tracked by epic #183): **P0** foundation polish, **P1** out-of-the-box chat magic (static custom prompts + default slash set, inline editor commands, insert/replace/apply, `@`-context pills), **P2** retrieval — the embeddings backend (#198), hybrid `vault_search` (#199, RRF over Tantivy lexical + vector), `time_query` (#200), and Relevant Notes (#201) now ship — **P3** fact memory & multimodal: the fact-note model and fact/wiki/both/session-only routing rubric are accepted and dogfooded in the personal memory vault, the lifecycle MCP tools now ship, and embedded chat can attach one configured companion memory vault with default `vault:<active-vault>` scope guidance (ADR 0021); stale-review UX remains backlog. **P4** scale & CLI edge (headless `notes ai`, customization discovery, MCP-server management UI, `@agent` routing). The full task breakdown lives in `plans/ai-integration-roadmap.md`.
- Practical fact-memory usage, routing examples, lifecycle maintenance, safety rules, and current limitations are documented in `docs/fact-memory.md`.

### Work-System Integrations

- Work-system data stays laptop-local and enters the vault through removable, config-declared connector jobs (ADR 0025). Calendar events are `kind: event` notes; raw email never enters Notesmith and only a summary persists; Teams transcripts are sidecar `kind: transcript` notes.
- The September 4, 2026 transcript spike proved that delegated Work IQ access can list transcript metadata and retrieve JSON-string-wrapped WebVTT. Calendar event notes must persist the Teams `join_url`; recurring instances reuse that URL, so transcript timestamps identify the occurrence before assigning its `event_id`.
- A real recurring-series probe verified that timestamp selection can be unambiguous by days. Calendar and transcript timestamps must be normalized explicitly to UTC, calendar pagination must be complete, and an out-of-window or ambiguous transcript remains unfiled rather than being attached to a guessed meeting.
- Delegated online-meeting resolution is incomplete but not limited to meetings organized by the signed-in user. Some other-organized series resolve while others are denied with `403 Forbidden` / `3003: User does not have access to lookup meeting`; organizer domain, recurrence, audience, and sampled join-URL shape did not distinguish them. Confirmed denials are cached for seven days using hashed join keys and reported separately, while genuine connector failures remain visible and retryable.
- The shared transcript segment model includes an optional speaker. Teams VTT speaker tags render as `[M:SS] Name: text`, while YouTube and local-audio transcripts use the same format with no speaker. Verbatim customer-call transcripts are permitted only in the local work vault and its corporate Git remote, never the personal homelab.
