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
- **GlobalConfig** — App-wide settings in `~/.config/notesmith/config.toml`. Contains daemon bind address, CLI auto-start policy, the vault registry, and an optional `[agents]` section for agent discovery (per-id launch overrides, custom ACP agents, and a `debug` diagnostics flag).
- **SidebarConfig** — Per-vault sidebar view definitions in `.notesmith/sidebar.yaml`. Defines custom views with sections (recently-viewed, custom-folders, custom-items).
- **RoutingConfig** — Per-vault routing rules in `.notesmith/routing.yaml`. Expressive YAML DSL with boolean combinators (all/any/not), field/tag predicates, and full mutations (move, set/remove fields, add/remove tags).
- **FieldRegistry** — Per-vault field definitions in `.notesmith/fields.toml`. Advisory type/value constraints for autocomplete and validation.
- **UserViews** — Per-vault SQL view definitions in `.notesmith/views.sql`. Creates persistent views in the cache database for dashboard blocks and queries.

## Capture & Routing

- **Capture** — A first-class command that writes timestamped notes to the configured capture folder. Delegates to the template system internally. When `capture.folder = ""`, captures land in the vault root.
- **Routing** — Rule-based note filing using an expressive YAML DSL (`.notesmith/routing.yaml`). Rules match on field values, tag presence/absence, path globs, and boolean combinators (all/any/not). Mutations: move_to, set_fields, remove_fields, add_tags, remove_tags. Supports both manual trigger (`notesmith route apply`) and auto-routing (opt-in per rule).
- **Route Log** — An append-only audit table recording every routing operation (from_path, to_path, rule_id, mutations). Enables undo via `notesmith route undo`.
- **Daemon-backed CLI commands** — `capture`, `query`, `note`, `search`, `template`, `route`, `periodic`, `task`, `reindex`, `daily`, `url-open`, and `mcp start`. They resolve a daemon base URL (the global `--url` flag or `NOTESMITH_URL` env var when set, otherwise the local bind) and, for the local daemon, probe `/api/status` and auto-start it when `[daemon].auto_start = true`. A remote target (`--url`/`NOTESMITH_URL`) is used verbatim and never auto-started; the `daemon` lifecycle subcommands always manage the local daemon. Base-URL resolution is centralized in `notesmith-cli`'s `daemon_client` (`resolve_override`/`set_remote_override`).

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
- **Container image flavors** — GHCR publishes an `app` flavor (`latest`, `sha-*`, date tags) with the SvelteKit frontend at `/app-ui` for browser `/app/` access, and an `api` flavor (`api-latest`, `api-sha-*`, date tags) with only the Rust binary. The Tauri desktop can use either flavor: when a remote server is the active connection (see **Connection Management**), the desktop serves the frontend from embedded assets and sends only API/SSE traffic to the daemon.
- **MCP server** — Exposes the vault to MCP clients in two ways: daemon-hosted streamable-HTTP endpoints mounted per vault at `/mcp/<vault>` (full) and `/mcp-ro/<vault>` (read-only) that reuse the daemon's live indexes, and the `notesmith mcp start` command which is a **stdio↔HTTP bridge** to those endpoints (it forwards every stdio request to the daemon; no embedded in-memory index path). Both map MCP tool/resource requests onto the shared `notesmith-ops` operations layer (`NotesmithMcp` wraps an `Arc<dyn Ops>`; the bridge holds a `Peer<RoleClient>` to the daemon). See ADR 0010.
- **Ops layer** — `notesmith-ops` defines the canonical vault-operation surface. `Ops` is the trait (read + write methods); `LocalOps` is the in-process implementation backed by engine/cache/search index/template engine; `ReadOnlyOps<O>` wraps any `Ops` and rejects every write (used to expose read-only agent surfaces without auth). See ADR 0010.
- **VaultState** — Per-vault runtime state held by the daemon: cache, search index, engine, root path, config (ArcSwap), template engine. The cache, search index, and template engine are held behind `Arc` so they can be shared into per-session daemon-hosted MCP handlers (`LocalOps::from_shared`).
- **AppState** — Global daemon state containing all VaultStates and shared config. Also owns the durable transcript store (`transcripts: Arc<TranscriptStore>`).
- **VaultEvent** — An SSE event broadcast when something changes in a vault: note CRUD, task updates, config changes, cache rebuilds.
- **Agent client (`notesmith-agent`)** — Drives external coding agents over **ACP** via the Zed `agent-client-protocol` crate; the crate owns the wire protocol. A declarative **registry** (`registry.rs`, ADR 0013) is the single source of truth for the five built-in agents — Copilot (`copilot --acp`), Claude (`npx @zed-industries/claude-code-acp`), Codex (`codex-acp`), Gemini (`gemini --experimental-acp`), and OpenCode (`opencode acp`) — each an `AgentDescriptor` with ordered launch candidates and setup hints; `AcpSession` builders consume it. `AcpSession` spawns the agent, runs `initialize` → `session/new` → multi-turn `session/prompt`, and normalizes `session/update` into `AgentEvent`s. The active vault is passed as a `session/new` `mcpServers` entry, modeled by the `McpBinding` transport enum; transport is **capability-aware** (ADR 0012, Decision 2): an **HTTP** endpoint (`/mcp/<vault>` or `/mcp-ro/<vault>`) is preferred when the agent advertises `mcpCapabilities.http` at `initialize` (Copilot is HTTP/SSE-only), with a local **stdio** bridge (`notesmith mcp start`) supplied as a fallback for local daemons when the agent lacks HTTP MCP support. **Permission policy** (`permission.rs`): writes prompt per-call (allow once / allow always-session / deny), read-only mode allows its (read-only) requests silently since it can only ever expose safe reads, grants cached session-scoped only. **Break-glass** (`acp_client.rs`, default OFF): an app-level toggle that advertises vault-scoped fs read/write + terminal; paths are normalized to the vault root, writes/terminal are blocked in read-only mode. **Context injection** (`context.rs`): a one-time session preamble carries a bounded `VaultSummary` (name, note count, top tags/folders) plus the vault's `.notesmith/skill.md` (caller-provided, missing/blank degrades to summary-only), and each turn carries an `EditorContext` block (active note, selection, open tabs) injected ahead of the user message — all size-bounded. **Model selection** (`model.rs`): `parse_model_picker` normalizes the `session/new` result into a `ModelPicker` — preferring a `configOptions` entry with `category: "model"`, falling back to the deprecated `modes`, `None` when neither (no picker, no error). Notesmith hardcodes no model list; `AcpSession::model_picker()` exposes the options and `select_model()` applies a choice via `session/set_config_option` (or `session/set_mode` for the modes fallback). See ADR 0012.
- **Transcript store (`notesmith-transcript`)** — Daemon-owned, durable per-vault chat history (ADR 0012 Decision 13). A single SQLite database (`transcripts.sqlite`) in the durable data dir (`XDG_DATA_HOME`/local-data → `notesmith/`) — deliberately **outside** any vault (so it neither clutters nor syncs) and **outside** the rebuildable index cache (`cache.sqlite`, which is dropped on schema bumps/reindex). `TranscriptStore` exposes vault-scoped `create_thread` / `list_threads` / `get_thread` / `rename_thread` / `delete_thread` / `append_message` / `load_messages`; every method is keyed by vault so vault A's history is invisible under vault B. Reads degrade per ADR 0009 (corrupt/partial rows are skipped with a `WARN`, never panic). Reopening a thread re-establishes the ACP child session lazily (the `AcpSession` driver starts on first `send`/`select_model`). The daemon opens the store at startup (`build_app_state` → `transcripts_path()`); the per-vault transcript REST endpoints and the chat UI (`agent/chat-store.svelte.ts`) consume it.
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
- **Right Dock** — A single collapsible right-side dock (`⌘\`, `RightDock.svelte`) with a top-level segmented control switching between two surfaces: **Context** (the tabbed right rail) and **Chat** (the agent panel). The active segment persists per vault in `localStorage` (`notesmith:dock-segment:<vault>`, helpers in `ui/app/src/lib/right-dock.ts`). Both segments share one column so the editor is never squeezed by two stacked panels; the ✦ workspace-chrome button is a shortcut that opens the dock directly on the Chat segment. The Chat pane is mounted lazily on first activation, then kept mounted to preserve the live agent session across segment switches.
- **Right Rail** — The **Context** segment of the Right Dock: a tabbed panel with three modes: **Metadata** (frontmatter key/values, `_`-prefixed keys hidden), **Links** (backlinks + outgoing), **TOC** (live table of contents from editor headings with click-to-scroll and active heading highlight). Tab selection persists in `localStorage`.
- **Middle Pane** — A resizable panel between sidebar and editor, opened by custom sidebar items to show folder listings or query results.
- **Folder Notes** — Same-name markdown notes (`Folder/Folder.md`) represented by their folder row in FileTree. Folder rows open the hidden folder note from the name control, expand from the chevron, and support creation/rename actions from the context menu.
- **Command Palette** — A fuzzy-searchable overlay for executing commands (⌘K / ⌘P). Shows footer hints for keyboard navigation and can switch into a theme-picker sub-mode that previews catalog themes before confirmation.
- **Input Palette** — A sequential multi-step input overlay (same chrome as Command Palette) used for note creation, capture, and template instantiation. Supports text input and fuzzy list picker modes. Replaces `window.prompt()` which is broken in Tauri's WKWebView.
- **Toast Stack** — Non-blocking corner notifications (success/error/warning) with auto-dismiss. Replaces `window.alert()`. Managed by `toast-store.svelte.ts`.
- **Quick Switcher** — A fuzzy note search overlay for rapid navigation (⌘O).
- **Vault Menu** — Browser-only dropdown anchored on the vault name in the workspace chrome (`VaultMenu.svelte`), exposing Switch Vault, Add Vault, and Settings. Rendered only when the Tauri runtime is absent (`isBrowserVaultMenu`); the desktop app uses its native OS menu and window-per-vault navigation instead. Pure helpers live in `ui/app/src/lib/vault-menu.ts`.
- **Status Bar** — A 28px bar at the bottom of the app showing: the **connection switcher** with diagnostic popover (left), vault name (center), cursor position, word count, and save indicator (right). The switcher (`ConnectionSwitcher.svelte`, pure helpers in `ui/app/src/lib/connection/connection-view.ts`) is a pill + dropdown that switches the active daemon between **This Mac** (local) and any saved server at runtime; it shares the persisted server list with Settings → Connection. Editor state shared via `editor-status.svelte.ts`.
- **Connection Management** — Desktop-only management of which daemon the app talks to. The persisted server list (`servers.json` in the app config dir, `crates/notesmith-tauri/src/servers.rs`) is the single source of truth, edited in **Settings → Connection** (`ConnectionSettings.svelte`, client in `ui/app/src/lib/connection/connection-client.ts`) and switched from the status-bar pill. Tauri `connection_*` commands list/add/update/remove/test servers and `connection_set_active` retargets the live webview (re-navigates each window, emits `notesmith://connection-changed`). `effective_settings` derives the daemon URL purely from the store's active selection (`active_target`), so the app is local-only by default and the in-app choice — not the environment — is authoritative. See ADR 0014.
- **Note Icons** — Notes can set `_icon:` (emoji) in frontmatter to override their icon in file trees, tabs, and quick switcher. Falls back to type-based defaults. `_`-prefixed frontmatter keys are reserved for system/UI use and hidden from metadata panels.
- **Agent Chat Panel** — The in-app AI chat surface (`ui/app/src/lib/components/agent/AgentPanel.svelte`, ADR 0012 Phase 8), surfaced as the **Chat** segment of the Right Dock. Three layers: (1) a transport boundary (`agent/agent-client.ts`) abstracting the Tauri IPC bridge behind an `AgentClient` interface — `TauriAgentClient` in the desktop app, `UnavailableAgentClient` (panel shows a desktop-only notice) elsewhere; (2) a pure reducer (`agent/conversation.ts`) folding the normalized `AgentEvent` stream (user message, streamed deltas, tool call/result, status, done, error) into renderable items; (3) a runes orchestration store (`agent/chat-store.svelte.ts`) injecting the client + the per-vault transcript REST client. The panel exposes agent + model pickers, a read-only/read-write toggle (RO by default — writes hard-denied), an "operating on `<vault>`" scope badge, a conversation/thread list, and a write-permission prompt (allow once / allow for session / deny). Tauri `event.listen` delivers a `{ payload }` envelope, unwrapped by `eventPayload()` in `agent-client.ts`.
- **Agent IPC Bridge** — The desktop-only host for the ACP client (`crates/notesmith-tauri/src/agent_bridge.rs`, ADR 0012 Decision 4). Hosts one `AcpSession` per chat session in a `tokio::select!` pump task that multiplexes ACP events (emitted to the UI as `notesmith://agent-event`) against UI commands. Thirteen `#[tauri::command]`s — `agent_list`, `agent_start`, `agent_prompt`, `agent_select_model`, `agent_set_read_only`, `agent_answer_permission`, `agent_stop`, the ADR 0013 discovery trio `agent_diagnostics`, `agent_config_get`, `agent_config_set`, plus the issue-192 runtime-diagnostics trio `agent_diagnostics_log`, `agent_diagnostics_set_verbose`, `agent_diagnostics_clear` — bridge JS camelCase ↔ Rust snake_case. The effective agent list merges the `notesmith-agent` registry with the `config.toml` `[agents]` section (`agent_config_get`/`agent_config_set`): a user override wins, `enabled = false` hides a built-in, and custom ids launch verbatim (with `~`/`$VAR` expansion and per-agent env). At startup `agent_path.rs` resolves an augmented `PATH` (login-shell query + curated dirs) so GUI-launched apps can find Homebrew/nvm-installed CLIs. On-demand structured discovery diagnostics live in a sibling module `agent_diag.rs`, whose `agent_diagnostics` command probes each registry agent (bounded `--version` spawn + capped stdout) and returns a step-by-step trace (resolved PATH, dirs searched, per-candidate found/probe, verdict `available`/`not_found`/`probe_failed`, plus a parsed `detectedVersion` and a `versionWarning` when below the registry's optional `minVersion` floor). A process-global, bounded **runtime diagnostics log** (`notesmith-agent` `diag_log.rs`, `AgentDiagnosticsLog`) records recent agent errors always and — when its verbose toggle is on — a "wire-ish" log of the ACP messages the bridge mediates at *our* boundary (outgoing prompts, emitted events, permission/fs/terminal requests); it cannot capture the raw JSON-RPC bytes, which the `agent_client_protocol` crate owns. Write-permission requests surface as `notesmith://agent-permission` and are answered via a `oneshot` channel (defaulting to Deny). MCP is bound to the local daemon's `/mcp[-ro]/<vault>` HTTP endpoint; the vault root comes from `GlobalConfig`. The Svelte panel and the bridge are validated end-to-end by headless Playwright flows (`ui/app/e2e/agent-chat.spec.ts`, `agent-picker.spec.ts`, `agent-settings.spec.ts`) that mount the real components with the bridge + transcript endpoints stubbed.
