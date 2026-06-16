# Notesmith Plan — Definitive Architectural Blueprint

Notesmith is the final synthesized plan for the custom markdown notes application that replaces Obsidian for the workflow in `notes-method.md`. This version supersedes the earlier domain-specific model. The application is now designed as a **generic programmable workspace** built from reusable primitives: notes, fields, tags, tasks, links, templates, routing rules, SQL views, and hooks.

This document is intentionally implementation-grade. Where a design choice is final, this document treats it as settled.

## Table of Contents

- [1. Overview & Vision](#1-overview--vision)
- [2. Technology Stack](#2-technology-stack)
- [3. Architecture](#3-architecture)
- [4. Crate Layout](#4-crate-layout)
- [5. Data Model](#5-data-model)
- [6. Data Layer](#6-data-layer)
- [7. Query System](#7-query-system)
- [8. Template Engine](#8-template-engine)
- [9. Routing Engine](#9-routing-engine)
- [10. Task Engine](#10-task-engine)
- [11. Capture](#11-capture)
- [12. Periodic Notes](#12-periodic-notes)
- [13. Git Integration](#13-git-integration)
- [14. Hook System](#14-hook-system)
- [15. HTTP API Design](#15-http-api-design)
- [16. CLI Design](#16-cli-design)
- [17. URL Scheme](#17-url-scheme)
- [18. Agent Integration](#18-agent-integration)
- [19. GUI Design](#19-gui-design)
- [20. Dashboards](#20-dashboards)
- [21. Copy as HTML](#21-copy-as-html)
- [22. Configuration](#22-configuration)
- [23. Multi-Vault](#23-multi-vault)
- [24. Obsidian Compatibility](#24-obsidian-compatibility)
- [25. File Watching & Conflict Handling](#25-file-watching--conflict-handling)
- [26. Performance Targets](#26-performance-targets)
- [27. Testing Strategy](#27-testing-strategy)
- [28. Implementation Phases](#28-implementation-phases)
- [29. Open Questions & Deferrals](#29-open-questions--deferrals)

## 1. Overview & Vision

### 1.1 Why Notesmith exists

Notesmith exists for three reasons:

1. **Agentic control.** The system must be easy for agents to drive through structured commands instead of plugin UIs and ad hoc scripting.
2. **Web-hosted architecture.** v1 is local-first, but the core must already fit a future reverse-proxied web deployment.
3. **Programmable knowledge workspace.** The left rail, dashboards, routing, templates, tasks, and automation must be driven by generic data primitives instead of hardcoded business concepts.

The product name is **Notesmith**.

### 1.2 Design principles

1. **Files are the source of truth.** The vault on disk is authoritative. SQLite, Tantivy, and in-memory indexes are caches.
2. **Generic primitives beat built-in domains.** Notesmith understands notes, fields, tags, links, tasks, periodic notes, and routes. It does not hardcode customers, projects, meetings, or any other business schema.
3. **Fields are unified at query time.** Users query by key and value only. The system may preserve source-specific round-tripping details internally, but the stable query contract does not distinguish frontmatter from inline fields.
4. **Tags remain tags.** Tags are first-class and separate from fields because they model classification and browsing better than arbitrary key/value metadata.
5. **HTTP-first everywhere.** The daemon speaks REST + SSE natively. The GUI, CLI, and MCP adapter all sit on top of that surface.
6. **SQL is the one structured query language.** Dashboards, templates, agents, and power users all meet at the same read-only SQL surface over stable views.
7. **Templates and routing are programmable, not magical.** Template context is explicit. Routing uses a documented YAML DSL. No hidden SQL snippets or special-case domain routers.
8. **Agents are first-class users.** Every important action is available from the `notesmith` CLI and described in `.notesmith/skill.md`.
9. **Plain markdown round-trips losslessly.** The vault remains valid Obsidian-flavored markdown and remains readable outside Notesmith.
10. **No plugin system.** Templates, tasks, routing, dashboards, git sync, sidebar views, URL handling, copy-as-HTML, and hook execution are built in.
11. **Thin desktop shell.** Tauri exists to provide a native window, deep links, tray integration, and OS affordances. The app itself is served by the daemon.
12. **Multi-vault from day one.** Vault naming, routing, caching, and watching all assume more than one vault.
13. **Starter kits are documentation, not product surface.** Notesmith may ship documented example workflows such as `docs/example-work-notes-kit.md`, but it does not bundle opinionated vault modes, registries, or downloadable kits.
14. **Fresh start over migration debt.** The generic model replaces the old domain-specific schema cleanly. Earlier experimental caches and model assumptions are not migrated.
15. **Pragmatic over ornamental.** Tabs ship in v1; splits do not. Passive dashboards ship; notifications do not.

### 1.3 Obsidian plugin → Notesmith built-in mapping

| Obsidian plugin / feature | Notesmith built-in | Final stance |
|---|---|---|
| Templater | `notesmith-templates` with `minijinja` + subprocess enrichment | Replaced; no embedded JS runtime |
| Tasks | Built-in task parser + config-backed status model + SQL views | Replaced |
| Dataview | SQL over SQLite cache via `notesmith sql` blocks | Replaced; SQL only |
| QuickAdd | CLI + command palette + URL scheme + first-class capture | Replaced |
| Auto Note Mover | Routing engine with YAML DSL | Replaced |
| Periodic Notes + Calendar | Generic periodic note engine (daily/weekly/monthly/quarterly/yearly) + calendar UI | Replaced |
| Homepage | Homepage config + native dashboard opening the configured note | Replaced |
| Linter | Save pipeline in Rust | Replaced |
| Hotkeys for specific files | Command palette entries + configurable shortcuts | Replaced |
| Bookmarks | Built-in pinned items and sidebar views | Replaced |
| Obsidian Git | Thin opt-in git integration | Replaced |
| Bases | Native tables/components over SQL results | Replaced |

### 1.4 Builder reality

The architecture is optimized for a workflow where the **user guides architecture and agents write most of the code**. That means:

- Rust is used for the durable core and CLI contract.
- SvelteKit is used because it is the user's preferred UI stack and is fast to iterate on with agent assistance.
- The system is decomposed into sharp library seams so code generation and review remain tractable.
- The definitive reference for the old work-notes workflow moves out of the core architecture and into `docs/example-work-notes-kit.md`.

## 2. Technology Stack

### 2.1 Primary stack

| Layer | Choice | Why |
|---|---|---|
| Core language | **Rust** | Fast parser/indexer, single-binary distribution, good crate ecosystem for filesystem tooling |
| GUI framework | **SvelteKit** | User preference, small runtime, excellent fit for document-centric UI |
| Desktop shell | **Tauri v2** | Thin native shell pointing at localhost, deep-link support, tray, native windowing |
| Daemon / API | **Axum + Tokio** | HTTP-first REST API with native SSE support |
| Editor | **CodeMirror 6** | Incremental editing, extensible markdown grammar, live preview decorations |
| Markdown renderer | **comrak** | Rust CommonMark + GFM renderer; also powers copy-as-HTML |
| Frontmatter | **serde + serde_yaml** | YAML parsing with preservation of unknown keys |
| Structured query cache | **SQLite via rusqlite** | Rebuildable local cache for views and dashboard queries |
| Full-text search | **Tantivy** | Embedded full-text search index separate from the SQLite cache |
| Template engine | **minijinja** | Jinja2-like syntax, sandboxed, deterministic |
| File watching | **notify** | Cross-platform watcher for all configured vaults |
| Git integration | **git2** | Built-in opt-in commit/pull/push timers |
| Hashing | **blake3** | Fast content hashes for notes, tasks, and optimistic concurrency |
| Config formats | **TOML + YAML + SQL + Markdown** | TOML for config, YAML for rules, SQL for views, Markdown for prompts/templates |

### 2.2 Key Rust crates

| Crate | Role |
|---|---|
| `axum` | REST API, static file serving, SSE |
| `tokio` | Async runtime |
| `serde`, `serde_json`, `serde_yaml`, `toml` | Serialization |
| `clap` | CLI command tree |
| `notify` | File watching |
| `rusqlite` | Cache access |
| `tantivy` | Search index |
| `minijinja` | Templates |
| `comrak` | Markdown → HTML rendering |
| `git2` | Git automation |
| `blake3` | Content hashes |
| `tracing`, `tracing-subscriber` | Structured logs |
| `chrono` | Date/time logic |
| `walkdir`, `ignore` | Vault traversal |
| `tauri`, `tauri-plugin-deep-link` | Thin desktop shell + URL scheme |

### 2.3 TurboVault stance

TurboVault is evaluated in **Phase 0** and is used only behind a `VaultEngine` trait. If the spike proves it saves time without forcing bad abstractions, keep it; if not, swap to a native implementation without changing the rest of the workspace.

## 3. Architecture

### 3.1 Runtime shape

```text
vaults/ (plain markdown)
        │
        ▼
┌────────────────────────────────────────────────────────────────────┐
│ notesmith daemon (started via `notesmith daemon start`)           │
│                                                                    │
│  VaultEngine  →  Parser/Indexer  →  SQLite cache + Tantivy         │
│       │                 │                    │                      │
│       ├──────── Templates / Routing / Tasks / Periodics / Hooks    │
│       │                                                            │
│       └──────── Axum HTTP server (REST + SSE + static app files)   │
└───────────────┬───────────────────────┬────────────────────────────┘
                │                       │
                ▼                       ▼
        `notesmith` CLI          Tauri shell → http://127.0.0.1:27183/app/
                │
                ▼
         Skill-file-driven agents
                │
                ▼
            MCP adapter
```

### 3.2 Daemon contract

- The daemon is the long-lived owner of file watching, indexing, routing, periodic-note scheduling fallback, and SSE fan-out.
- It is launched with `notesmith daemon start`.
- Default bind address is `127.0.0.1:27183`.
- `--bind` can expose it elsewhere, but the daemon itself remains auth-ignorant.
- The compiled SvelteKit app is served by the daemon under `/app/` when frontend assets are present. Containerized deployments publish two flavors: `app` bundles the frontend and sets `NOTESMITH_APP_DIR=/app-ui` for browser access, while `api` is binary-only for CLI/MCP/API-only use and Tauri desktop clients with embedded frontend assets.
- There is **no separate `notesmithd` binary**.

### 3.3 Tauri shell role

Tauri is a **thin shell pointing at localhost**. It is responsible for:

- starting the daemon if needed,
- opening the native window onto the local app URL,
- registering `notesmith://` deep links,
- exposing the system tray and basic native menu items.

The Tauri shell does **not** own business logic, query execution, or note state.
The desktop app is **local-only by default** and connects to remote daemons through an in-app **server list** (`servers.json`), managed in **Settings → Connection** and switched at runtime from a **status-bar pill** (shared list, no restart). When a remote server is the active connection, the shell loads bundled SvelteKit assets from the `notesmith-app://localhost/app/` custom protocol and passes `apiBase=<daemon>` to the frontend. API calls and SSE streams target that daemon, so the desktop app works with both container flavors; only browser access requires the `app` flavor's daemon-served `/app/`. The active selection (`servers.json`'s `active_id`) is authoritative. See ADR 0014.

In remote-daemon mode, vault creation and registration are daemon-side operations. The UI sends vault management requests to the configured API base and asks for server/container paths instead of opening a local folder picker, so a desktop client cannot accidentally register a Mac path on a homelab daemon.

### 3.4 Authentication model

| Context | Model |
|---|---|
| Local desktop / CLI | **No auth** |
| Web-hosted later | **External auth proxy** in front of the daemon |
| Authorization | **None in core**; auth only, no authz |

The daemon does not implement login screens, session stores, RBAC, or token parsing. For web-hosted deployments later, a reverse proxy performs authentication and forwards requests to the daemon. Because there is no authz layer, that deployment model is intentionally scoped to trusted single-user or tightly-controlled environments.

### 3.5 Core Rust traits

```rust
trait VaultEngine: Send + Sync {
    fn scan(&self, root: &Path) -> anyhow::Result<Vec<DiscoveredNote>>;
    fn read(&self, root: &Path, path: &VaultPath) -> anyhow::Result<String>;
    fn write(
        &self,
        root: &Path,
        path: &VaultPath,
        expected_hash: Option<blake3::Hash>,
        content: &str,
    ) -> anyhow::Result<WriteResult>;
    fn move_path(&self, root: &Path, from: &VaultPath, to: &VaultPath) -> anyhow::Result<()>;
    fn delete(&self, root: &Path, path: &VaultPath) -> anyhow::Result<()>;
    fn watch(&self, root: &Path) -> anyhow::Result<Box<dyn VaultWatcher>>;
}

trait VaultOps {
    fn note_create(&self, req: CreateNoteReq) -> anyhow::Result<NoteSummary>;
    fn note_get(&self, req: GetNoteReq) -> anyhow::Result<NoteDocument>;
    fn note_put(&self, req: PutNoteReq) -> anyhow::Result<NoteSummary>;
    fn note_mutate(&self, req: MutateNoteReq) -> anyhow::Result<NoteSummary>;
    fn route_preview(&self, req: RoutePreviewReq) -> anyhow::Result<RoutePreview>;
    fn route_apply(&self, req: RouteApplyReq) -> anyhow::Result<RouteResult>;
    fn route_undo(&self, req: RouteUndoReq) -> anyhow::Result<RouteResult>;
    fn task_toggle(&self, req: ToggleTaskReq) -> anyhow::Result<TaskRecord>;
    fn task_set_status(&self, req: SetTaskStatusReq) -> anyhow::Result<TaskRecord>;
    fn capture(&self, req: CaptureReq) -> anyhow::Result<NoteSummary>;
    fn periodic_ensure(&self, req: EnsurePeriodicReq) -> anyhow::Result<NoteSummary>;
    fn query_sql(&self, req: SqlQueryReq) -> anyhow::Result<QueryResult>;
    fn search(&self, req: SearchReq) -> anyhow::Result<SearchResult>;
}
```

`VaultEngine` isolates TurboVault. `VaultOps` is the stable application surface surfaced through HTTP, CLI, GUI, and MCP.

## 4. Crate Layout

```text
notesmith/
├── Cargo.toml
├── crates/
│   ├── notesmith-core/        # note model, parser, OFM extensions, VaultOps trait
│   ├── notesmith-vault/       # VaultEngine trait + TurboVault/native adapters
│   ├── notesmith-index/       # SQLite cache builder + Tantivy indexing
│   ├── notesmith-query/       # stable views + SQL execution + dashboard helpers
│   ├── notesmith-templates/   # minijinja env, prompt specs, context assembly
│   ├── notesmith-routing/     # YAML DSL, mutation planning, route log, undo
│   ├── notesmith-tasks/       # task parsing, config-backed status resolution
│   ├── notesmith-hooks/       # subprocess hook runner + payload contracts
│   ├── notesmith-git/         # opt-in git timers and sync helpers
│   ├── notesmith-html/        # comrak-based HTML rendering and clipboard helpers
│   ├── notesmith-config/      # global/per-vault config loading and validation
│   ├── notesmith-http/        # Axum daemon, REST endpoints, SSE, static app serving
│   ├── notesmith-mcp/         # MCP adapter on top of VaultOps
│   ├── notesmith-cli/         # clap command tree; produces the `notesmith` binary
│   ├── theme-gen/             # build-time theme CSS generator from the catalog JSON
│   └── notesmith-tauri/       # thin desktop shell
├── ui/
│   └── app/                   # SvelteKit frontend
└── plans/
```

### 4.1 Ownership rules

- `notesmith-core` never depends on Axum, Tauri, or SvelteKit.
- `notesmith-http` owns the daemon runtime; `notesmith-cli` is a client plus daemon launcher.
- `notesmith-tauri` depends on the compiled SvelteKit bundle and the CLI crate, not vice versa.
- MCP never gets private capabilities; it only wraps existing `VaultOps` operations.
- The simplification to a generic workspace happens **inside existing crates**. Crates do not merge just because the domain model becomes simpler.

### 4.2 Crate responsibility notes

| Crate | Generic-model implication |
|---|---|
| `notesmith-core` | Replaces typed note-kind assumptions with generic note/field/tag/task primitives |
| `notesmith-index` | Owns the unified fields table, tags table, task status lookup cache, and route log schema |
| `notesmith-query` | Publishes generic stable views only (`v_notes`, `v_fields`, `v_tasks`, `v_task_fields`, `v_backlinks`, `v_periodic`) |
| `notesmith-templates` | Builds context from static config, SQL queries, and hook enrichment |
| `notesmith-routing` | Evaluates YAML predicates over generic fields/tags/path and applies note mutations |
| `notesmith-tasks` | Resolves configurable status characters into stable groups (`open`, `done`) |
| `notesmith-config` | Loads `.notesmith/vault.toml`, `.notesmith/fields.toml`, `.notesmith/views.sql`, `.notesmith/routing.yaml` |

## 5. Data Model

### 5.1 Core entity: Note

The core entity is a markdown note. A note is a file plus parsed projections. It is **not** a member of a hardcoded Rust enum such as `Customer`, `Meeting`, or `Stream`.

```rust
pub struct Note {
    pub vault: VaultName,
    pub path: VaultPath,
    pub title: String,
    pub frontmatter: Option<serde_yaml::Mapping>,
    pub body: String,
    pub ast: Option<Ast>,
    pub blocks: Vec<Block>,
    pub links: Vec<Link>,
    pub fields: Vec<Field>,
    pub tags: Vec<Tag>,
    pub tasks: Vec<Task>,
    pub periodic: Option<PeriodicStamp>,
    pub mtime: SystemTime,
    pub hash: blake3::Hash,
}
```

A note may contain any combination of:

- frontmatter,
- inline fields (`[key:: value]`),
- inline tags and frontmatter tags,
- task list items,
- wikilinks and embeds,
- periodic metadata,
- dashboard SQL blocks,
- arbitrary markdown.

There is no first-class `type` discriminator in the architecture. If a workspace wants a `type` field, it may define and query one like any other field, but Notesmith does not give it special semantics.

### 5.2 Fields (unified, no source distinction)

Fields are the generic metadata primitive. They are indexed from user-authored note content and exposed through a unified query contract.

#### 5.2.1 Field semantics

- A field has a `key`, a string `value`, and a `value_type` hint.
- Multiple values for the same key are allowed.
- Query APIs expose fields by key/value only.
- SQL users do not need to know whether a value came from frontmatter or inline syntax.
- Field origin may be retained in parser internals for round-tripping, but it is not part of the stable `v_fields` contract.

#### 5.2.2 Canonical Rust shape

```rust
pub struct Field {
    pub key: String,
    pub value_text: String,
    pub value_type: FieldValueType,
    pub ordinal: u32,
}

pub enum FieldValueType {
    Text,
    Integer,
    Number,
    Boolean,
    Date,
    DateTime,
    Link,
    Json,
    Unknown,
}
```

#### 5.2.3 Reserved vs ordinary keys

Notesmith reserves a very small namespace for system metadata. User fields remain unconstrained outside that namespace.

| Namespace | Meaning |
|---|---|
| `_notesmith.*` | Internal metadata written by Notesmith when needed |
| `_ui.*` | Frontend-only display hints if the user chooses to use them |
| everything else | User space |

Notesmith does **not** reserve semantic business keys like `customer`, `stream`, `meeting-kind`, or `state`.

#### 5.2.4 Repeated fields

Repeated values are legal and queryable.

```markdown
---
owner:
  - Alex
  - Priya
priority: 2
---

# Project Atlas

[owner:: Morgan]
[status:: active]
```

The field inventory becomes conceptually:

| key | value | type |
|---|---|---|
| `owner` | `Alex` | `text` |
| `owner` | `Priya` | `text` |
| `priority` | `2` | `integer` |
| `owner` | `Morgan` | `text` |
| `status` | `active` | `text` |

Users query all of them the same way:

```sql
SELECT note_path, field_key, field_value
FROM v_fields
WHERE field_key = 'owner';
```

#### 5.2.5 Field registry

Field registry metadata lives in `.notesmith/fields.toml`, not in `vault.toml`. Registry entries are optional and advisory, not required for indexing.

Example:

```toml
version = 1

[fields.owner]
type = "string"
description = "Who currently owns the note or task"
multivalue = true

[fields.priority]
type = "integer"
description = "Lower number = higher urgency"

[fields.status]
type = "enum"
values = ["idea", "active", "blocked", "done"]

[fields.area]
type = "link"
autocomplete = { sql = "SELECT title AS value FROM v_notes WHERE path GLOB 'Areas/**' ORDER BY title" }
```

The registry is used for:

- editor autocomplete,
- form builders,
- routing type resolution,
- validation warnings,
- UI labels and help text,
- future schema-aware agent prompts.

The registry does **not** gate what users can write. Unknown fields still index and query normally.

### 5.3 Tags

Tags remain separate from fields.

#### 5.3.1 Why tags are separate

Tags serve a different purpose from fields:

- tags are lightweight classification,
- tags work well for browsing and quick filtering,
- tags map directly to existing OFM/Obsidian expectations,
- tags frequently want set semantics and hierarchical slashes,
- tags should not require `key=value` ceremony.

A field such as `status = active` is not the same thing as a tag such as `#active`.

#### 5.3.2 Tag sources

Notesmith indexes tags from:

- frontmatter `tags:` arrays,
- inline `#tag` syntax in the markdown body.

All tags normalize into the tags table without preserving source in the query API.

#### 5.3.3 Canonical Rust shape

```rust
pub struct Tag {
    pub tag: String,
}
```

#### 5.3.4 Tag examples

```markdown
---
tags:
  - project/active
  - team/platform
---

# Atlas

This note also has inline tags: #focus #review
```

Resulting tag set:

- `project/active`
- `team/platform`
- `focus`
- `review`

### 5.4 Tasks (configurable statuses)

Tasks remain markdown checkbox items on disk, but their meaning is now configurable.

#### 5.4.1 Core task model

```rust
pub struct Task {
    pub task_hash: blake3::Hash,
    pub text: String,
    pub status_char: char,
    pub status_group: TaskStatusGroup,
    pub line: u32,
    pub heading_path: Option<String>,
    pub fields: Vec<Field>,
    pub raw_markdown: String,
}

pub enum TaskStatusGroup {
    Open,
    Done,
}
```

#### 5.4.2 On-disk syntax

```markdown
- [ ] Draft summary [area:: [[Platform]]] [due:: 2026-07-01]
- [/] Implement parser [owner:: me]
- [b] Waiting on upstream [blocked_by:: vendor]
- [x] Sent update [completed_at:: 2026-06-01]
```

Notesmith stores:

- the raw checkbox character (`status_char`),
- the resolved group (`status_group` = `open` or `done`),
- the task body text,
- any task-local fields.

#### 5.4.3 Status configuration

Status configuration lives in `vault.toml`, not in code.

```toml
[[tasks.statuses]]
char = " "
label = "To Do"
group = "open"
icon = "☐"
order = 10

[[tasks.statuses]]
char = "/"
label = "In Progress"
group = "open"
icon = "◐"
order = 20

[[tasks.statuses]]
char = "b"
label = "Blocked"
group = "open"
icon = "⛔"
order = 30

[[tasks.statuses]]
char = "x"
label = "Done"
group = "done"
icon = "✅"
order = 90
```

The default starter examples may mirror the older seven-status workflow, but that is documentation only. The engine itself is status-character configurable.

#### 5.4.4 Unknown status behavior

If a task uses a checkbox character not defined in config:

- the parser does not drop the task,
- `status_char` remains the raw character,
- `status_group` defaults to `open`,
- the index logs a warning,
- the UI shows a fallback label like `Unknown (?)`.

This keeps malformed or drifted config from destroying task visibility.

### 5.5 Links

Links remain first-class.

```rust
pub enum LinkKind {
    Wiki,
    Embed,
    HeadingRef,
    BlockRef,
    Markdown,
    External,
}

pub struct Link {
    pub source_note: VaultPath,
    pub raw_target: String,
    pub resolved_note: Option<VaultPath>,
    pub kind: LinkKind,
    pub heading_ref: Option<String>,
    pub block_ref: Option<String>,
}
```

Notesmith does **not** add a separate relationships layer for typed edges. Typed backlinks emerge from a combination of:

- normal note links,
- field queries,
- task fields,
- user-defined SQL views.

Example typed relationship query:

```sql
SELECT f.note_path, f.field_value AS owner_note
FROM v_fields f
WHERE f.field_key = 'owner'
  AND f.value_type = 'link';
```

### 5.6 Periodic Notes

Periodic notes are built-in, generic note classes keyed by calendar period.

Supported kinds:

- daily,
- weekly,
- monthly,
- quarterly,
- yearly.

#### 5.6.1 Canonical periodic stamp

```rust
pub struct PeriodicStamp {
    pub kind: PeriodicKind,
    pub key: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
}

pub enum PeriodicKind {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}
```

#### 5.6.2 Periodic identity rules

| Kind | Key format | Example |
|---|---|---|
| daily | `YYYY-MM-DD` | `2026-05-14` |
| weekly | ISO week `YYYY-Www` | `2026-W20` |
| monthly | `YYYY-MM` | `2026-05` |
| quarterly | `YYYY-Qn` | `2026-Q2` |
| yearly | `YYYY` | `2026` |

#### 5.6.3 Periodic notes are still notes

Periodic notes are not stored in a separate file format. They are ordinary markdown files with extra indexed periodic metadata. A workspace may choose any path layout through config.

Example layouts that are both valid:

```text
Journal/Daily/2026-05-14.md
Journal/Weekly/2026-W20.md
Journal/Monthly/2026-05.md
```

or:

```text
Periods/2026/05/2026-05-14.md
Periods/2026/Weeks/2026-W20.md
Periods/2026/Quarters/2026-Q2.md
```

Only `.notesmith/` is required. Everything else is user-defined.

### 5.7 OFM support contract

| Syntax | Meaning | Notes |
|---|---|---|
| `[[Wiki Link]]`, `[[Note\|alias]]`, `[[Note#Heading]]`, `[[Note#^block-id]]` | Wikilinks | Resolved in Notesmith; preserved verbatim on disk |
| `![[Embed]]` | Embed | Rendered inline in preview |
| `> [!note]` etc. | Callouts | Full OFM callout rendering |
| `- [ ]`, `- [/]`, `- [x]`, `- [b]`, etc. | Task states | Status characters are config-driven via `vault.toml` |
| `[key:: value]` | Inline field | Indexed into the unified fields cache |
| ```` ```notesmith sql ```` | Live SQL block | Executed read-only by Notesmith; inert in Obsidian |
| `%% comment %%` | Obsidian comments | Preserved in source |
| `^block-id` | Block references | Indexed for backlink resolution |
| `==highlight==` | Highlight | Preserved |
| Frontmatter `tags:` + inline `#tag` | Tags | Indexed into separate tags table |
| Attachments in arbitrary user folders | Passthrough files | Served statically; no custom asset pipeline |

## 6. Data Layer

### 6.1 Cache philosophy

The SQLite file is a **rebuildable cache**, not a database of record. Deleting `~/.cache/notesmith/{vault-name}/cache.sqlite` must be annoying but never destructive.

### 6.2 Cache location

Each vault gets its own cache directory:

```text
~/.cache/notesmith/{vault-name}/
├── cache.sqlite
├── tantivy/
└── state.json
```

The vault itself stays clean; no cache files live inside the note tree.

### 6.3 Core cache schema

This is a fresh schema for the generic workspace model. Earlier experimental schemas are not migrated.

#### 6.3.1 Base tables

```sql
CREATE TABLE notes (
  vault_name TEXT NOT NULL,
  path TEXT NOT NULL,
  title TEXT NOT NULL,
  folder_path TEXT NOT NULL,
  frontmatter_json TEXT NOT NULL,
  body_excerpt TEXT NOT NULL,
  first_heading TEXT,
  created_at TEXT,
  updated_at TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  is_periodic INTEGER NOT NULL DEFAULT 0,
  periodic_kind TEXT,
  period_key TEXT,
  period_start TEXT,
  period_end TEXT,
  mtime_unix INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  PRIMARY KEY (vault_name, path)
);

CREATE TABLE fields (
  vault_name TEXT NOT NULL,
  note_path TEXT NOT NULL,
  field_key TEXT NOT NULL,
  field_value TEXT NOT NULL,
  value_type TEXT NOT NULL DEFAULT 'text',
  ordinal INTEGER NOT NULL DEFAULT 0,
  normalized_value TEXT,
  PRIMARY KEY (vault_name, note_path, field_key, ordinal)
);

CREATE TABLE tags (
  vault_name TEXT NOT NULL,
  note_path TEXT NOT NULL,
  tag TEXT NOT NULL,
  PRIMARY KEY (vault_name, note_path, tag)
);

CREATE TABLE links (
  vault_name TEXT NOT NULL,
  src_path TEXT NOT NULL,
  dst_path TEXT,
  raw_target TEXT NOT NULL,
  kind TEXT NOT NULL,
  heading_ref TEXT,
  block_ref TEXT
);

CREATE TABLE task_status_defs (
  vault_name TEXT NOT NULL,
  status_char TEXT NOT NULL,
  label TEXT NOT NULL,
  status_group TEXT NOT NULL,
  icon TEXT,
  sort_order INTEGER NOT NULL,
  PRIMARY KEY (vault_name, status_char)
);

CREATE TABLE tasks (
  vault_name TEXT NOT NULL,
  task_hash TEXT NOT NULL,
  note_path TEXT NOT NULL,
  heading_path TEXT,
  ordinal INTEGER NOT NULL,
  line INTEGER NOT NULL,
  status_char TEXT NOT NULL,
  status_group TEXT NOT NULL,
  text TEXT NOT NULL,
  raw_markdown TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  PRIMARY KEY (vault_name, task_hash)
);

CREATE TABLE task_fields (
  vault_name TEXT NOT NULL,
  task_hash TEXT NOT NULL,
  field_key TEXT NOT NULL,
  field_value TEXT NOT NULL,
  value_type TEXT NOT NULL DEFAULT 'text',
  ordinal INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (vault_name, task_hash, field_key, ordinal)
);

CREATE TABLE periodic_notes (
  vault_name TEXT NOT NULL,
  note_path TEXT NOT NULL,
  kind TEXT NOT NULL,
  period_key TEXT NOT NULL,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  template_name TEXT,
  created_by TEXT NOT NULL,
  PRIMARY KEY (vault_name, note_path)
);

CREATE TABLE field_registry (
  vault_name TEXT NOT NULL,
  field_key TEXT NOT NULL,
  declared_type TEXT NOT NULL DEFAULT 'text',
  multivalue INTEGER NOT NULL DEFAULT 0,
  enum_values_json TEXT,
  autocomplete_sql TEXT,
  autocomplete_values_json TEXT,
  description TEXT,
  PRIMARY KEY (vault_name, field_key)
);

CREATE TABLE route_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  vault_name TEXT NOT NULL,
  matched_rule_id TEXT,
  auto_applied INTEGER NOT NULL DEFAULT 0,
  note_path_before TEXT NOT NULL,
  note_path_after TEXT NOT NULL,
  content_hash_before TEXT NOT NULL,
  content_hash_after TEXT NOT NULL,
  fields_before_json TEXT NOT NULL,
  fields_after_json TEXT NOT NULL,
  tags_before_json TEXT NOT NULL,
  tags_after_json TEXT NOT NULL,
  mutation_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  undone_at TEXT
);
```

#### 6.3.2 Indexes

```sql
CREATE INDEX idx_fields_key_value
  ON fields (vault_name, field_key, field_value);

CREATE INDEX idx_tags_tag
  ON tags (vault_name, tag);

CREATE INDEX idx_links_dst
  ON links (vault_name, dst_path);

CREATE INDEX idx_tasks_group
  ON tasks (vault_name, status_group, note_path);

CREATE INDEX idx_periodic_kind_key
  ON periodic_notes (vault_name, kind, period_key);

CREATE INDEX idx_route_log_path_after
  ON route_log (vault_name, note_path_after, created_at DESC);
```

#### 6.3.3 Schema commentary

- `notes` is the note inventory table and cache anchor.
- `fields` is unified and intentionally source-agnostic.
- `tags` is separate by design.
- `task_status_defs` mirrors `vault.toml` task status config into SQL-friendly form.
- `task_fields` makes task-level inline metadata queryable without inventing special task columns.
- `periodic_notes` exists even though periodic metadata is also projected into `notes`; it makes uniqueness checks and scheduling logic explicit.
- `field_registry` mirrors `.notesmith/fields.toml` into the cache to support editor and query helpers.
- `route_log` is retained for audit and undo.

#### 6.3.4 Field storage choice

Field values are stored as text plus a type hint.

That is the durable rule:

- `field_value` is always a string,
- `value_type` says how Notesmith believes the value should be interpreted,
- higher-level code may cast or parse based on `value_type` or registry declarations.

Example rows:

| field_key | field_value | value_type |
|---|---|---|
| `priority` | `2` | `integer` |
| `reviewed` | `true` | `boolean` |
| `due` | `2026-05-20` | `date` |
| `owner` | `[[Alex]]` | `link` |
| `notes` | `{"mode":"focus"}` | `json` |

#### 6.3.5 Route log usage

`route_log` supports three workflows:

1. **Audit** — show what rule moved or mutated a note.
2. **Undo** — revert the most recent route operation for a note.
3. **Forensics** — explain why an auto-route changed a path or metadata field.

Example audit query:

```sql
SELECT matched_rule_id, note_path_before, note_path_after, created_at
FROM route_log
WHERE note_path_after = 'Projects/Atlas/Atlas.md'
ORDER BY created_at DESC;
```

### 6.4 Stable SQL views

**Views are the API. Tables are implementation detail.**

Day-one required views are generic and stable.

#### 6.4.1 `v_notes`

```sql
CREATE VIEW v_notes AS
SELECT
  n.vault_name,
  n.path,
  n.folder_path,
  n.title,
  n.created_at,
  n.updated_at,
  n.archived,
  n.is_periodic,
  n.periodic_kind,
  n.period_key,
  n.period_start,
  n.period_end,
  n.mtime_unix,
  n.content_hash,
  n.first_heading,
  n.body_excerpt,
  n.frontmatter_json,
  (SELECT COUNT(*) FROM fields f WHERE f.vault_name = n.vault_name AND f.note_path = n.path) AS field_count,
  (SELECT COUNT(*) FROM tags t WHERE t.vault_name = n.vault_name AND t.note_path = n.path) AS tag_count,
  (SELECT COUNT(*) FROM tasks tk WHERE tk.vault_name = n.vault_name AND tk.note_path = n.path) AS task_count
FROM notes n;
```

#### 6.4.2 `v_fields`

```sql
CREATE VIEW v_fields AS
SELECT
  vault_name,
  note_path,
  field_key,
  field_value,
  value_type,
  ordinal,
  normalized_value
FROM fields;
```

#### 6.4.3 `v_tasks`

```sql
CREATE VIEW v_tasks AS
SELECT
  t.vault_name,
  t.task_hash,
  t.note_path,
  t.heading_path,
  t.ordinal,
  t.line,
  t.status_char,
  t.status_group,
  COALESCE(sd.label, 'Unknown') AS status_label,
  COALESCE(sd.icon, '') AS status_icon,
  t.text,
  t.raw_markdown,
  t.content_hash,
  (SELECT COUNT(*) FROM task_fields tf WHERE tf.vault_name = t.vault_name AND tf.task_hash = t.task_hash) AS field_count
FROM tasks t
LEFT JOIN task_status_defs sd
  ON sd.vault_name = t.vault_name
 AND sd.status_char = t.status_char;
```

#### 6.4.4 `v_task_fields`

```sql
CREATE VIEW v_task_fields AS
SELECT
  tf.vault_name,
  tf.task_hash,
  t.note_path,
  tf.field_key,
  tf.field_value,
  tf.value_type,
  tf.ordinal
FROM task_fields tf
JOIN tasks t
  ON t.vault_name = tf.vault_name
 AND t.task_hash = tf.task_hash;
```

#### 6.4.5 `v_backlinks`

```sql
CREATE VIEW v_backlinks AS
SELECT
  dst_path AS note_path,
  src_path AS backlink_path,
  kind,
  raw_target,
  heading_ref,
  block_ref
FROM links
WHERE dst_path IS NOT NULL;
```

#### 6.4.6 `v_periodic`

```sql
CREATE VIEW v_periodic AS
SELECT
  p.vault_name,
  p.note_path,
  p.kind,
  p.period_key,
  p.period_start,
  p.period_end,
  p.template_name,
  p.created_by,
  n.title,
  n.updated_at,
  n.archived
FROM periodic_notes p
JOIN notes n
  ON n.vault_name = p.vault_name
 AND n.path = p.note_path;
```

#### 6.4.7 Stable-view guidance

These views are intentionally generic. They do **not** include domain-specific convenience projections like `v_customers` or `v_streams`.

If a workspace wants those semantics, it creates them in `.notesmith/views.sql`.

Example:

```sql
CREATE VIEW v_projects AS
SELECT n.path, n.title
FROM v_notes n
WHERE EXISTS (
  SELECT 1
  FROM v_fields f
  WHERE f.note_path = n.path
    AND f.field_key = 'kind'
    AND f.field_value = 'project'
);
```

### 6.5 User-defined views (`.notesmith/views.sql`)

User-defined SQL views are first-class.

#### 6.5.1 File location

```text
.notesmith/views.sql
```

#### 6.5.2 Loader rules

- The loader executes after the core schema and core `v_*` views are created.
- User views may reference core views and other previously defined user views.
- Core view names may not be overridden.
- The loader rejects statements other than `CREATE VIEW`, `CREATE TEMP VIEW`, or `DROP VIEW IF EXISTS` for user-owned view names.
- Failures in `views.sql` should surface as config/index warnings, not daemon crashes.

#### 6.5.3 Example file

```sql
DROP VIEW IF EXISTS v_open_items;
CREATE VIEW v_open_items AS
SELECT
  t.note_path,
  t.text,
  t.status_label,
  tf.field_value AS area
FROM v_tasks t
LEFT JOIN v_task_fields tf
  ON tf.task_hash = t.task_hash
 AND tf.field_key = 'area'
WHERE t.status_group = 'open';

DROP VIEW IF EXISTS v_focus_notes;
CREATE VIEW v_focus_notes AS
SELECT
  n.path,
  n.title,
  n.updated_at
FROM v_notes n
WHERE EXISTS (
  SELECT 1 FROM tags t
  WHERE t.note_path = n.path
    AND t.tag = 'focus'
);
```

#### 6.5.4 Optional `v_tags`

`v_tags` is not part of the required stable contract list, but it is reasonable to add later as a convenience view if tag-heavy dashboards justify it. v1 keeps the stable required set intentionally small.

### 6.6 Search index

Tantivy holds the full-text index. SQLite handles structured queries; Tantivy handles ranking and tokenized text search. The daemon keeps them in sync from the same parse pass.

## 7. Query System

### 7.1 One query language: SQL

There is **no DQL, no NDQL, and no Tasks DSL**. Notesmith uses SQL only.

That choice is deliberate:

- agents already understand SQL,
- dashboards gain a stable, inspectable contract,
- there is no second parser to maintain,
- compatibility complexity moves into views, not user syntax,
- generic workspaces can create domain-specific meaning without waiting for core code changes.

### 7.2 Query surfaces

| Surface | Form |
|---|---|
| Markdown notes | `notesmith sql` fenced code blocks |
| CLI | `notesmith query sql ...` |
| HTTP API | `POST /api/v/{vault-name}/query/sql` |
| Native dashboards | Stored SQL snippets executed against stable views |
| Templates | Named SQL context queries inside template metadata |
| Agents | CLI, MCP, or HTTP against the same read-only SQL contract |

### 7.3 Markdown syntax

````markdown
```notesmith sql
SELECT n.title, n.updated_at
FROM v_notes n
WHERE EXISTS (
  SELECT 1
  FROM v_fields f
  WHERE f.note_path = n.path
    AND f.field_key = 'status'
    AND f.field_value = 'active'
)
ORDER BY n.updated_at DESC;
```
````

Only SQL is valid inside the block. The renderer executes the statement against the named vault and renders the result as a table, list, or chart depending on the surrounding component.

### 7.4 Query execution rules

1. Default execution target is the stable `v_*` view layer.
2. Raw base-table access is allowed only for debugging and internal tooling.
3. Dashboard code blocks are **strictly read-only** and must be `SELECT` statements.
4. The renderer never writes query results back into notes.
5. Query failures render visible errors inline rather than silently failing.
6. Query blocks execute against the current vault only unless a future cross-vault story is explicitly added.

### 7.5 Read-only enforcement

Read-only means:

- `SELECT` and `WITH ... SELECT` are allowed,
- `INSERT`, `UPDATE`, `DELETE`, `ALTER`, `CREATE`, `DROP`, `ATTACH`, and `PRAGMA` are rejected,
- multi-statement input is rejected,
- dashboard SQL blocks and the query API share the same validator.

Example rejected dashboard block:

````markdown
```notesmith sql
DELETE FROM notes;
```
````

Rendered result in the UI:

```text
Query rejected: dashboard SQL blocks must be read-only SELECT statements.
```

### 7.6 Examples

#### 7.6.1 Find all notes with a field key

```sql
SELECT note_path, field_value
FROM v_fields
WHERE field_key = 'owner'
ORDER BY note_path;
```

#### 7.6.2 Find periodic notes for this quarter

```sql
SELECT note_path, period_key, updated_at
FROM v_periodic
WHERE kind = 'quarterly'
  AND period_key = '2026-Q2';
```

#### 7.6.3 Join task fields to notes

```sql
SELECT t.note_path, t.text, tf.field_value AS due
FROM v_tasks t
JOIN v_task_fields tf
  ON tf.task_hash = t.task_hash
WHERE tf.field_key = 'due'
  AND t.status_group = 'open'
ORDER BY tf.field_value;
```

## 8. Template Engine

### 8.1 Engine choice

Notesmith uses **minijinja**. It replaces Templater with a deterministic, sandboxed engine that is easy for both humans and agents to reason about.

### 8.2 Template file format

Templates live under a user-chosen visible workspace folder such as `Assets/templates/` and use a YAML preamble for metadata, followed by the markdown body.

```markdown
---
notesmith:
  name: generic-note
  description: Generic note with field prompts
  output_path: "{{ folder }}/{{ title | slug }}.md"
  prompts:
    - { name: title, type: text, required: true }
    - { name: folder, type: text, required: true }
    - { name: status, type: enum, values: [idea, active, blocked, done], required: false }
  context:
    static:
      starter_tag: inbox
    sql:
      recent_notes: |
        SELECT path, title, updated_at
        FROM v_notes
        ORDER BY updated_at DESC
        LIMIT 5
      known_statuses: |
        SELECT DISTINCT field_value AS value
        FROM v_fields
        WHERE field_key = 'status'
        ORDER BY value
    hook:
      event: on_note_create
---
---
status: {{ prompt.status or "idea" }}
created: {{ now }}
updated: {{ now }}
---

# {{ prompt.title }}

Tags: #{{ context.static.starter_tag }}

## Recent notes
{% for row in context.sql.recent_notes.rows %}
- [[{{ row.path }}|{{ row.title }}]]
{% endfor %}
```

### 8.3 Three context layers

Templates get three context layers, merged in a fixed order.

#### 8.3.1 Layer 1 — static context

Static context comes from template metadata and Notesmith built-ins.

Included values:

- `vault.name`
- `now`, `today`, `tomorrow`, `yesterday`
- `periodic` info when relevant
- `context.static.*` from the template preamble
- `prompt.*` values

#### 8.3.2 Layer 2 — SQL query context

Named SQL queries run after static context exists.

Contract:

- each query gets rendered with minijinja first,
- each query must be read-only,
- each result is exposed as a table-like object under `context.sql.<name>`.

Example query context shape:

```json
{
  "context": {
    "sql": {
      "recent_notes": {
        "columns": ["path", "title", "updated_at"],
        "rows": [
          {"path": "Inbox/one.md", "title": "one", "updated_at": "2026-05-14T08:00:00Z"}
        ]
      }
    }
  }
}
```

#### 8.3.3 Layer 3 — hook enrichment

The final context layer comes from the relevant lifecycle hook.

Rules:

- note creation templates may request `on_note_create` enrichment,
- periodic note templates may request `on_periodic_create` enrichment,
- capture templates also use `on_note_create` because capture ultimately creates a note,
- hook stdout is parsed as JSON and merged under `context.hook`.

Example hook result:

```json
{
  "suggested_tags": ["focus", "week-20"],
  "summary": "Generated from weekly review"
}
```

Merged template usage:

```markdown
{% for tag in context.hook.suggested_tags or [] %}
#{{ tag }}
{% endfor %}
```

### 8.4 Built-in helpers

| Helper | Purpose |
|---|---|
| `today`, `tomorrow`, `yesterday`, `now` | Date helpers |
| `slug(s)`, `title_case(s)` | String helpers |
| `prompt(name)` | Access fulfilled prompt values |
| `periodic.kind`, `periodic.key` | Periodic note helpers |
| `to_wikilink(s)` | Convert a string to `[[Name]]` |
| `json(value)` | Serialize structured data for frontmatter or fenced blocks |

There is **no arbitrary SQL helper function inside the markdown body**. SQL context is declared up front in template metadata so the render plan stays inspectable and cacheable.

### 8.5 Prompt fulfillment

Prompt fulfillment is identical across surfaces:

- **CLI:** flags or interactive fallback when attached to a TTY.
- **HTTP:** request body includes `prompts`.
- **GUI:** input palette.
- **Agents:** supply prompt values directly through CLI or API.

### 8.6 Replacing Templater JavaScript

The escape hatch is **subprocess hooks**, not embedded JavaScript. Hooks receive structured JSON on stdin and can emit structured JSON on stdout. That keeps the core small and the behavior inspectable.

### 8.7 Example periodic template

```markdown
---
notesmith:
  name: weekly-review
  output_path: "Journal/Weekly/{{ periodic.key }}.md"
  context:
    static:
      title: "Weekly Review {{ periodic.key }}"
    sql:
      open_tasks: |
        SELECT text, note_path
        FROM v_tasks
        WHERE status_group = 'open'
        ORDER BY note_path, ordinal
      touched_notes: |
        SELECT title, path, updated_at
        FROM v_notes
        WHERE updated_at >= datetime('now', '-7 day')
        ORDER BY updated_at DESC
        LIMIT 20
    hook:
      event: on_periodic_create
---
# {{ context.static.title }}

## Open tasks
{% for row in context.sql.open_tasks.rows %}
- [ ] {{ row.text }} ({{ row.note_path }})
{% endfor %}
```

## 9. Routing Engine

### 9.1 Goal

The routing engine files notes by evaluating a documented YAML DSL over generic note state.

It replaces domain-specific archive logic with a deterministic mutation engine that can:

- move notes,
- set fields,
- remove fields,
- add tags,
- remove tags,
- log every route operation,
- run either manually or automatically.

### 9.2 Rule file

Rules live in `.notesmith/routing.yaml`.

```yaml
version: 1
defaults:
  on_exists: rename
rules:
  - id: inbox-project-note
    auto: false
    when:
      all:
        - path_glob: "Inbox/**"
        - field:
            key: status
            op: eq
            value: active
        - tags_include: [inbox]
        - not:
            tags_include: [archived]
    then:
      set_fields:
        reviewed_at: "{{ now }}"
      add_tags: [triaged]
      remove_tags: [inbox]
      move_to: "Projects/{{ fields.status | default('misc') }}/{{ filename }}"

  - id: auto-periodic-journal
    auto: true
    when:
      all:
        - field:
            key: periodic.kind
            op: eq
            value: daily
        - path_glob: "Inbox/**"
    then:
      move_to: "Journal/Daily/{{ fields['periodic.key'] }}.md"
```

### 9.3 Predicate DSL

The `when` clause uses YAML, not SQL.

#### 9.3.1 Supported predicates

| Predicate | Shape | Meaning |
|---|---|---|
| `field` | `{ key, op, value }` | Compare a field value |
| `field_exists` | `field_exists: key_name` | Match when a field key exists |
| `tags_include` | `tags_include: [a, b]` | Match when all listed tags exist |
| `tags_exclude` | `tags_exclude: [a, b]` | Match when none of the listed tags exist |
| `path_glob` | `path_glob: "Inbox/**"` | Match vault-relative path glob |
| `all` | `all: [ ... ]` | Logical AND |
| `any` | `any: [ ... ]` | Logical OR |
| `not` | `not: <predicate>` | Logical NOT |

#### 9.3.2 Supported operators for `field`

| Operator | Meaning |
|---|---|
| `eq` | equals |
| `ne` | not equals |
| `lt` | less than |
| `lte` | less than or equal |
| `gt` | greater than |
| `gte` | greater than or equal |
| `contains` | substring contains for text |
| `matches` | regex match |

#### 9.3.3 Type resolution for comparisons

When evaluating `lt`, `lte`, `gt`, or `gte`, the router resolves type in this order:

1. field registry declaration from `.notesmith/fields.toml`,
2. indexed `value_type` hint,
3. fallback lexical string comparison.

This keeps the DSL expressive without inventing field-specific code.

#### 9.3.4 Multi-value field behavior

For repeated field keys:

- equality-style predicates match if **any** value matches,
- `field_exists` matches if at least one value exists,
- comparison operators succeed if any value satisfies the comparison.

### 9.4 `then` clause mutations

The `then` clause supports full note mutation.

#### 9.4.1 Mutation keys

| Key | Type | Meaning |
|---|---|---|
| `move_to` | string template | Destination path |
| `set_fields` | mapping | Set or overwrite field values |
| `remove_fields` | list | Remove all values for those field keys |
| `add_tags` | list | Add tags idempotently |
| `remove_tags` | list | Remove tags if present |

#### 9.4.2 Example mutation-only rule

```yaml
- id: mark-reviewed
  when:
    tags_include: [needs-review]
  then:
    set_fields:
      reviewed_at: "{{ now }}"
    add_tags: [reviewed]
    remove_tags: [needs-review]
```

`move_to` is optional. A rule may mutate metadata without moving the note.

#### 9.4.3 Mutation execution order

Execution order is fixed:

1. start from the current parsed note state,
2. remove listed fields,
3. set fields,
4. remove tags,
5. add tags,
6. render `move_to` against the **post-mutation draft state**,
7. write the updated note content,
8. move the file if needed,
9. append a `route_log` entry.

Rendering `move_to` against the post-mutation state allows rules like:

```yaml
then:
  set_fields:
    bucket: archive
  move_to: "{{ fields.bucket }}/{{ filename }}"
```

### 9.5 Manual vs auto routing

Routing supports both manual and automatic application.

#### 9.5.1 Default manual

Rules are manual unless `auto: true` is set.

That means:

- `notesmith route preview <path>` and `notesmith route apply <path>` always consider all rules,
- autosave, capture, periodic creation, and note updates only consider rules where `auto: true`.

#### 9.5.2 Auto-route trigger points

Auto rules are evaluated after successful note creation or update, including:

- note creation via template,
- capture-created notes,
- periodic note creation,
- normal note save.

#### 9.5.3 Loop prevention

Auto routing uses these safety rules:

- only the first matching auto rule runs,
- a route that would result in no content or path change is skipped,
- a note is not re-routed within the same save transaction,
- route preview shows the exact change plan before an explicit manual apply.

### 9.6 Rule selection semantics

Rules are evaluated top to bottom. The **first matching rule wins**.

That keeps behavior deterministic and previewable. If users want more complex logic, they should compose it into a single rule with `all` / `any` / `not`.

### 9.7 Route log and undo

Every applied route writes a `route_log` row.

Example row shape:

```json
{
  "id": 42,
  "matched_rule_id": "inbox-project-note",
  "auto_applied": false,
  "note_path_before": "Inbox/atlas.md",
  "note_path_after": "Projects/active/atlas.md",
  "mutation_json": {
    "set_fields": {"reviewed_at": "2026-05-14T08:00:00Z"},
    "remove_tags": ["inbox"],
    "add_tags": ["triaged"]
  }
}
```

Undo uses the most recent unapplied log entry:

```bash
notesmith route undo --id 42
```

Undo restores:

- prior path,
- prior fields,
- prior tags,
- prior content hash expectations.

If the current note hash does not match `content_hash_after`, undo returns a conflict instead of guessing.

### 9.8 Examples

#### 9.8.1 Route notes based on a numeric field

```yaml
- id: urgent-work
  auto: false
  when:
    all:
      - field:
          key: priority
          op: lte
          value: 2
      - tags_exclude: [archive]
  then:
    add_tags: [focus]
    move_to: "Work/Urgent/{{ filename }}"
```

#### 9.8.2 Route notes based on missing field

```yaml
- id: missing-owner
  when:
    all:
      - path_glob: "Inbox/**"
      - not:
          field_exists: owner
  then:
    add_tags: [needs-owner]
```

#### 9.8.3 Route periodic notes by kind

```yaml
- id: weekly-notes
  auto: true
  when:
    field:
      key: periodic.kind
      op: eq
      value: weekly
  then:
    move_to: "Journal/Weekly/{{ fields['periodic.key'] }}.md"
```

## 10. Task Engine

### 10.1 Canonical task syntax

Tasks remain markdown list items with bracketed status characters.

```markdown
- [ ] Draft proposal [project:: [[Atlas]]] [owner:: me] [due:: 2026-06-01]
- [/] Review open questions [area:: planning]
- [b] Waiting on legal [blocked_by:: counsel]
- [w] Waiting for external input [owner:: vendor]
- [h] Deferred until Q4 [review_in:: 2026-10]
- [x] Published summary [completed_at:: 2026-05-14]
- [-] Superseded by new note [replaced_by:: [[Atlas v2]]]
```

Notesmith does not assume those seven statuses are universal; they are simply a useful example configuration.

### 10.2 Status resolution model

The engine resolves each task into:

- `status_char` — raw character inside `[ ]`,
- `status_group` — `open` or `done`,
- `status_label` — from config,
- `status_icon` — from config.

This keeps the model minimal but expressive enough for UI and queries.

### 10.3 Task fields

Task-level metadata is generic and field-based.

Examples:

- `[owner:: me]`
- `[due:: 2026-05-20]`
- `[area:: [[Platform]]]`
- `[estimate:: 3]`

The parser extracts these into `task_fields` instead of special columns.

### 10.4 Content-hash anchored toggling

Task updates are **not line-number based**.

1. Each parsed task gets `task_hash = blake3(normalize(text + status_char + task_fields + heading_path))`.
2. The UI and API submit `note_path + task_hash` when toggling or changing status.
3. The mutator reparses the note, finds the matching normalized task content, and rewrites only that task line.
4. If multiple tasks collide, the operation returns a conflict instead of guessing.

This survives line insertions, formatting shifts, and most agent edits.

### 10.5 Task transitions

Transitions are config-driven rather than hardcoded in Rust.

Optional config extension:

```toml
[[tasks.statuses]]
char = " "
label = "To Do"
group = "open"
icon = "☐"
order = 10
next = ["/", "b", "x"]

[[tasks.statuses]]
char = "x"
label = "Done"
group = "done"
icon = "✅"
order = 90
next = [" "]
```

If `next` is omitted, the UI may present all configured status characters.

### 10.6 Querying tasks

Task lists are SQL projections over `v_tasks` and `v_task_fields`. Convenience commands such as `notesmith task open` are wrappers that generate SQL, not a second query language.

Examples:

```sql
SELECT note_path, text, status_label
FROM v_tasks
WHERE status_group = 'open'
ORDER BY note_path, ordinal;
```

```sql
SELECT t.note_path, t.text, tf.field_value AS due
FROM v_tasks t
JOIN v_task_fields tf ON tf.task_hash = t.task_hash
WHERE tf.field_key = 'due'
  AND t.status_group = 'open'
ORDER BY tf.field_value;
```

### 10.7 Hook implications

A task save may emit:

- `on_task_change` if tasks changed,
- `on_field_change` if watched task-related note fields changed separately,
- `on_note_update` because the note content changed.

### 10.8 UI implications

The UI can render both generic and opinionated task views because the engine supplies:

- raw char,
- resolved group,
- config label,
- config icon,
- generic task fields.

That is enough to recreate older workflows as user-defined views without hardcoding them into the core model.

## 11. Capture

### 11.1 First-class capture

Capture remains a first-class workflow with dedicated API, CLI, URL, and GUI entry points.

### 11.2 Capture delegates to templates internally

Internally, capture is implemented by the template engine.

That means capture:

- selects a configured capture template,
- supplies built-in prompts such as captured text and timestamp,
- renders through the same context system as normal templates,
- optionally receives `on_note_create` hook enrichment,
- writes a normal note.

The dedicated capture command exists because the workflow matters, not because the implementation is separate.

### 11.3 Capture surfaces

| Surface | Form |
|---|---|
| HTTP | `POST /api/v/{vault-name}/capture` |
| CLI | `notesmith capture "text"` |
| URL scheme | `notesmith://app/capture/{vault-name}?text=...` |
| GUI | Quick Capture command + hotkey |

### 11.4 Capture behavior

By default, capture creates a note in the configured capture folder using the configured capture template and a timestamp-based filename.

Example config:

```toml
[capture]
folder = "Inbox"
template = "capture-note"
filename = "{{ now | date('%Y-%m-%d %H-%M-%S') }} - {{ prompt.text | slug }}.md"
```

Example template skeleton:

```markdown
---
notesmith:
  name: capture-note
  output_path: "{{ config.capture.folder }}/{{ config.capture.filename }}"
  prompts:
    - { name: text, type: text, required: true }
---
# {{ prompt.text }}

created: {{ now }}
```

### 11.5 Capture backlog workflow

1. Capture quickly.
2. Enrich or rewrite the note as needed.
3. Use `route apply` when the note is ready for long-term placement.
4. Use dashboards and SQL views to drive the backlog to zero.

## 12. Periodic Notes

### 12.1 Product stance

Periodic notes are built-in and generic. v1 supports:

- daily,
- weekly,
- monthly,
- quarterly,
- yearly.

### 12.2 Configuration model

Periodic configuration lives in `vault.toml`.

```toml
[periodic.daily]
enabled = true
path = "Journal/Daily/{{ key }}.md"
template = "daily-note"
generate_at = "06:30"
timezone = "America/Los_Angeles"
catch_up = true

[periodic.weekly]
enabled = true
path = "Journal/Weekly/{{ key }}.md"
template = "weekly-review"
open_on = "monday"

[periodic.monthly]
enabled = true
path = "Journal/Monthly/{{ key }}.md"
template = "monthly-review"

[periodic.quarterly]
enabled = true
path = "Journal/Quarterly/{{ key }}.md"
template = "quarterly-review"

[periodic.yearly]
enabled = true
path = "Journal/Yearly/{{ key }}.md"
template = "yearly-review"
```

### 12.3 CLI contract

```bash
notesmith periodic ensure --kind daily --key 2026-05-14
notesmith periodic ensure --kind weekly --key 2026-W20
notesmith periodic open --kind monthly --key 2026-05
notesmith periodic list --kind quarterly
```

### 12.4 HTTP contract

```http
POST /api/v/{vault}/periodic/ensure
GET  /api/v/{vault}/periodic/{kind}/{key}
GET  /api/v/{vault}/periodic?kind=weekly
```

### 12.5 Creation flow

1. Resolve the target kind and key.
2. Compute `period_start` and `period_end`.
3. Check `periodic_notes` for an existing note.
4. If missing, render the configured template.
5. Optionally enrich context via `on_periodic_create`.
6. Write the note.
7. Optionally apply matching auto routes.
8. Emit SSE + hooks.

### 12.6 Periodic context object

Templates for periodic notes receive:

```json
{
  "periodic": {
    "kind": "weekly",
    "key": "2026-W20",
    "period_start": "2026-05-11",
    "period_end": "2026-05-17"
  }
}
```

### 12.7 Scheduler stance

Periodic creation is primarily user-, agent-, or command-driven, with a daemon scheduler as fallback for automated daily creation. The scheduler should stay simple and transparent.

### 12.8 SQL examples

```sql
SELECT note_path, kind, period_key
FROM v_periodic
ORDER BY kind, period_key DESC;
```

```sql
SELECT note_path
FROM v_periodic
WHERE kind = 'weekly'
  AND period_start >= date('now', '-90 day');
```

## 13. Git Integration

### 13.1 Scope

Git integration is **thin, opt-in, and built in**. It is for backup and history, not collaborative conflict resolution.

### 13.2 Capabilities

- status,
- pull,
- push,
- timer-based auto-commit,
- timer-based auto-pull / auto-push.

### 13.3 Configuration

```toml
[git]
enabled = true
auto_commit_every = "15m"
auto_pull_every = "30m"
auto_push_every = "30m"
commit_message = "notesmith: {{ operation }} {{ summary }}"
```

### 13.4 Conflict stance

There is **no special conflict resolver**. If git produces raw conflict markers, Notesmith shows them as text and lets the user or an agent resolve them explicitly.

## 14. Hook System

### 14.1 Model

Hooks are subprocesses. They receive JSON on stdin and may return JSON on stdout.

Hooks are **non-blocking by default** for side effects. Template enrichment hooks are bounded by a short timeout and may return JSON used during render.

### 14.2 v1 event list

v1 ships exactly six hook events:

1. `on_note_create`
2. `on_note_update`
3. `on_note_route`
4. `on_periodic_create`
5. `on_task_change`
6. `on_field_change`

There is no seventh generic enrichment hook. Template enrichment uses the relevant lifecycle event.

### 14.3 Configuration example

```toml
[hooks.on_note_create]
command = "Assets/scripts/on-note-create.py"
timeout = "2s"
mode = "enrich"

[hooks.on_note_update]
command = "Assets/scripts/on-note-update.py"
timeout = "2s"
mode = "fire_and_forget"

[hooks.on_note_route]
command = "Assets/scripts/on-note-route.py"
timeout = "2s"
mode = "fire_and_forget"

[hooks.on_periodic_create]
command = "Assets/scripts/on-periodic-create.py"
timeout = "2s"
mode = "enrich"

[hooks.on_task_change]
command = "Assets/scripts/on-task-change.py"
timeout = "2s"
mode = "fire_and_forget"

[hooks.on_field_change]
command = "Assets/scripts/on-field-change.py"
timeout = "2s"
mode = "fire_and_forget"
watch = ["status", "owner", "priority", "reviewed_at"]
```

### 14.4 Payload shape: `on_note_create`

```json
{
  "event": "on_note_create",
  "vault": "work",
  "path": "Inbox/2026-05-14 - atlas.md",
  "title": "atlas",
  "fields": {
    "status": ["idea"],
    "owner": ["me"]
  },
  "tags": ["inbox"],
  "source": "capture",
  "template": "capture-note"
}
```

When `mode = "enrich"`, stdout JSON is merged into template context under `context.hook`.

### 14.5 Payload shape: `on_note_update`

```json
{
  "event": "on_note_update",
  "vault": "work",
  "path": "Projects/Atlas.md",
  "hash_before": "abc",
  "hash_after": "def",
  "changed": {
    "content": true,
    "fields": true,
    "tags": false,
    "tasks": true
  }
}
```

### 14.6 Payload shape: `on_note_route`

```json
{
  "event": "on_note_route",
  "vault": "work",
  "matched_rule_id": "inbox-project-note",
  "auto_applied": false,
  "path_before": "Inbox/atlas.md",
  "path_after": "Projects/active/atlas.md",
  "mutation": {
    "set_fields": {"reviewed_at": "2026-05-14T08:00:00Z"},
    "remove_tags": ["inbox"],
    "add_tags": ["triaged"]
  },
  "route_log_id": 42
}
```

### 14.7 Payload shape: `on_periodic_create`

```json
{
  "event": "on_periodic_create",
  "vault": "work",
  "kind": "weekly",
  "key": "2026-W20",
  "period_start": "2026-05-11",
  "period_end": "2026-05-17",
  "path": "Journal/Weekly/2026-W20.md",
  "template": "weekly-review"
}
```

### 14.8 Payload shape: `on_task_change`

`on_task_change` is batched per save.

```json
{
  "event": "on_task_change",
  "vault": "work",
  "path": "Projects/Atlas.md",
  "changes": [
    {
      "action": "status_change",
      "task_hash": "abc",
      "status_char_before": " ",
      "status_char_after": "/",
      "status_group_before": "open",
      "status_group_after": "open",
      "text": "Draft proposal"
    },
    {
      "action": "add",
      "task_hash": "def",
      "status_char_after": " ",
      "status_group_after": "open",
      "text": "Book review session"
    }
  ]
}
```

### 14.9 Payload shape: `on_field_change`

`on_field_change` is scoped to watched keys and batched per save.

It fires when a watched key is added, changed, or removed.

```json
{
  "event": "on_field_change",
  "vault": "work",
  "path": "Projects/Atlas.md",
  "changes": [
    {
      "key": "owner",
      "action": "change",
      "old": ["me"],
      "new": ["team"],
      "value_type": "text"
    },
    {
      "key": "priority",
      "action": "add",
      "old": [],
      "new": ["2"],
      "value_type": "integer"
    },
    {
      "key": "reviewed_at",
      "action": "remove",
      "old": ["2026-05-01T09:00:00Z"],
      "new": [],
      "value_type": "datetime"
    }
  ]
}
```

This satisfies the final rule that `on_field_change` fires on add/change/remove with an explicit `action` field.

### 14.10 Failure stance

- Hook failures must not abort note saves, routing, capture, or periodic creation.
- Notesmith logs hook stderr and timeout failures.
- Enrichment hooks degrade to empty context when they fail.
- The payload contract is versioned by event name and documented in the skill file.

## 15. HTTP API Design

### 15.1 Principles

- REST over HTTP.
- SSE for real-time updates.
- Multi-vault addressing in the URL.
- Local daemon is unauthenticated by design.
- Generic model first: field/tag/task/periodic operations instead of note-type-specific endpoints.

### 15.2 Endpoint table

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/ping` | health check |
| `GET` | `/app/*` | compiled SvelteKit app |
| `GET` | `/api/status` | daemon status |
| `GET` | `/api/v/{vault}/notes` | list notes |
| `POST` | `/api/v/{vault}/notes` | create note |
| `GET` | `/api/v/{vault}/notes/{path...}` | fetch note |
| `PUT` | `/api/v/{vault}/notes/{path...}` | replace note contents |
| `PATCH` | `/api/v/{vault}/notes/{path...}` | patch note content/metadata |
| `POST` | `/api/v/{vault}/notes/{path...}/move` | move note |
| `POST` | `/api/v/{vault}/notes/{path...}/mutate` | set/remove fields and tags |
| `GET` | `/api/v/{vault}/config` | read vault config |
| `PUT` | `/api/v/{vault}/config` | write vault config |
| `GET` | `/api/v/{vault}/fields/registry` | read resolved field registry |
| `POST` | `/api/v/{vault}/route/preview` | preview destination + mutations |
| `POST` | `/api/v/{vault}/route/apply` | apply one or more routes |
| `POST` | `/api/v/{vault}/route/undo` | undo a prior route log entry |
| `GET` | `/api/v/{vault}/route/log` | inspect route history |
| `POST` | `/api/v/{vault}/capture` | quick capture |
| `POST` | `/api/v/{vault}/periodic/ensure` | create/fetch periodic note |
| `GET` | `/api/v/{vault}/periodic/{kind}/{key}` | fetch one periodic note |
| `GET` | `/api/v/{vault}/periodic` | list periodic notes |
| `GET` | `/api/v/{vault}/tasks` | list tasks |
| `POST` | `/api/v/{vault}/tasks` | add task |
| `POST` | `/api/v/{vault}/tasks/toggle` | content-hash anchored toggle |
| `POST` | `/api/v/{vault}/tasks/status` | set an explicit status char |
| `POST` | `/api/v/{vault}/query/sql` | execute read-only SQL |
| `GET` | `/api/v/{vault}/search` | Tantivy search |
| `GET` | `/api/v/{vault}/templates` | list templates |
| `POST` | `/api/v/{vault}/templates/{name}/render` | render template |
| `POST` | `/api/v/{vault}/templates/{name}/instantiate` | create note from template |
| `GET` | `/api/v/{vault}/sidebar-config` | resolved sidebar model |
| `GET` | `/api/v/{vault}/events` | SSE stream |
| `GET` | `/api/v/{vault}/assets/{path...}` | serve attachment / asset file |
| `POST` | `/api/v/{vault}/copy-html` | render note to HTML fragment/full doc |

### 15.3 Example payloads

#### 15.3.1 `POST /api/v/{vault}/notes/{path...}/mutate`

```json
{
  "set_fields": {
    "status": ["active"],
    "owner": ["me"]
  },
  "remove_fields": ["draft"],
  "add_tags": ["focus"],
  "remove_tags": ["inbox"],
  "expected_hash": "abc123"
}
```

#### 15.3.2 `POST /api/v/{vault}/route/preview`

```json
{
  "path": "Inbox/atlas.md"
}
```

Response:

```json
{
  "matched_rule_id": "inbox-project-note",
  "auto_applied": false,
  "changes": {
    "set_fields": {"reviewed_at": ["2026-05-14T08:00:00Z"]},
    "remove_tags": ["inbox"],
    "add_tags": ["triaged"],
    "move_to": "Projects/active/atlas.md"
  }
}
```

#### 15.3.3 `POST /api/v/{vault}/periodic/ensure`

```json
{
  "kind": "weekly",
  "key": "2026-W20",
  "open_if_exists": true
}
```

### 15.4 SSE event types

```text
note.created
note.updated
note.moved
note.deleted
note.routed
periodic.created
task.changed
field.changed
config.changed
cache.rebuilt
search.reindexed
```

SSE is chosen because it is simpler than WebSocket, easier to proxy, and completely adequate for Notesmith's one-way real-time update needs.

### 15.5 Read-only query example

```bash
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT path, title FROM v_notes ORDER BY updated_at DESC LIMIT 10"}'
```

## 16. CLI Design

### 16.1 Binary model

There is exactly one binary: **`notesmith`**.

The daemon is a subcommand, not a second executable.

### 16.2 Command tree

```text
notesmith [--vault <name|path>] [--format text|json|ndjson]

  daemon start [--bind 127.0.0.1:27183]
  daemon logs

  vault list
  vault detect
  vault info
  vault reindex
  vault init [path]

  note create|get|put|append|delete|move|mutate
  template list|render|instantiate
  route preview|apply|undo|log
  task list|add|toggle|set-status
  capture
  periodic open|ensure|list
  query sql <sql-or-file>
  search <terms>
  dashboard render <path>
  git status|pull|push|sync
  mcp start
  copy-html <path>
  skill print
```

### 16.3 Vault selection

Vault selection works like git:

1. Walk upward from `$PWD` looking for `.notesmith/vault.toml`.
2. If found, use that vault.
3. Otherwise honor `--vault <name>`.
4. Otherwise use the default named vault from global config.

### 16.4 Output rules

- Human-readable text when attached to a TTY.
- JSON when piped or when `--format json` is set.
- Errors are structured for agent consumption.
- Daemon-backed CLI commands auto-start the HTTP daemon when `[daemon].auto_start = true` and `/api/status` is not healthy.
- `notesmith mcp start` is a stdio↔HTTP bridge to the daemon's per-vault MCP endpoint (`/mcp/<vault>`, or `/mcp-ro/<vault>` with `--read-only`): it resolves a daemon URL (global `--url`/`NOTESMITH_URL`, otherwise the local daemon auto-started on demand) and forwards every stdio request, sharing the daemon's live indexes.
- A global `--url` flag and `NOTESMITH_URL` env var retarget daemon-backed commands (and `mcp start`) at a remote daemon; `daemon` lifecycle subcommands stay local.

### 16.5 Pipe-friendly examples

```bash
notesmith query sql "SELECT note_path, field_value FROM v_fields WHERE field_key = 'owner'" --format json | jq '.'

notesmith capture "Need follow-up with Atlas" --vault work

notesmith task list --format json | jq '.[] | select(.status_group == "open")'

notesmith periodic ensure --kind weekly --key 2026-W20

notesmith copy-html "Projects/Atlas.md" --stdout | pbcopy
```

### 16.6 Note mutation example

```bash
notesmith note mutate Projects/Atlas.md \
  --set-field status=active \
  --set-field owner=me \
  --remove-field draft \
  --add-tag focus \
  --remove-tag inbox
```

### 16.7 Routing examples

```bash
notesmith route preview Inbox/atlas.md
notesmith route apply Inbox/atlas.md
notesmith route log Projects/active/atlas.md --limit 10
notesmith route undo --id 42
```

## 17. URL Scheme

### 17.1 Namespaces

- `notesmith://app/...` — built-in Notesmith actions
- `notesmith://user/...` — reserved for user-defined shortcuts and custom actions

This namespacing is first-class in v1.

### 17.2 Built-in actions

| URL | Effect |
|---|---|
| `notesmith://app/open?vault=work&path=Projects/Atlas.md` | Open a note |
| `notesmith://app/search?vault=work&query=atlas` | Open search results |
| `notesmith://app/template/new?vault=work&name=generic-note` | New note from template |
| `notesmith://app/capture/work?text=Follow%20up` | Quick capture |
| `notesmith://app/periodic/open?vault=work&kind=weekly&key=2026-W20` | Open a periodic note |
| `notesmith://app/copy-html?vault=work&path=Dashboards/Home.md` | Copy note as HTML |

### 17.3 User namespace

User actions are defined in `.notesmith/url-actions.yaml`.

```yaml
version: 1
actions:
  weekly-review:
    run:
      - notesmith
      - periodic
      - open
      - --vault
      - work
      - --kind
      - weekly
      - --key
      - 2026-W20
```

`notesmith://user/weekly-review` resolves through that table.

## 18. Agent Integration

### 18.1 Priority order

1. **CLI + skill file** — primary
2. **MCP** — fallback for GUI-only agent clients

There is **no ACP server**.

### 18.2 Why CLI + skill is primary

A skill file is more context-efficient than forcing an agent to rediscover the entire tool surface from an RPC schema. It can encode exact commands, field conventions, routing rules, SQL view contracts, and workflow recipes in the user's language.

### 18.3 Skill file contents

The canonical per-vault skill file is `.notesmith/skill.md`. It contains:

- command cheat sheet,
- workspace folder conventions,
- field registry summary,
- task status config summary,
- stable SQL view contract,
- periodic workflow,
- capture triage workflow,
- routing rules summary,
- examples for common tasks.

Example excerpt:

```markdown
# Notesmith Skill

## Create a note from a template
notesmith template instantiate generic-note --vault work --prompt title="Atlas"

## List notes tagged focus
notesmith query sql "SELECT note_path FROM v_fields WHERE field_key = 'status' AND field_value = 'active'"

## Route a prepared note
notesmith route apply "Inbox/atlas.md"
```

### 18.4 MCP scope

The MCP adapter exposes only existing operations such as note read/write, SQL query, routing, capture workflows, periodic note creation, and task mutation. It exists for clients that cannot run the CLI directly. It is served by the daemon as streamable-HTTP endpoints mounted per vault at `/mcp/<vault>` (full) and `/mcp-ro/<vault>` (read-only), reusing the daemon's live indexes via the shared `notesmith-ops` layer; `notesmith mcp start` is a thin stdio↔HTTP bridge to those endpoints rather than a standalone server with its own in-memory indexes.

## 19. GUI Design

### 19.1 Core UI layout

The GUI is a SvelteKit app served by the daemon and wrapped by Tauri.

v1 layout:

- left sidebar,
- tab bar,
- primary editor/view tab,
- right rail for backlinks, fields, tags, and note metadata,
- command palette as the primary navigation surface.

Tabs ship in v1. Split panes do not.

Theme assets are generated at build time from `ui/app/src/styles/theme-catalog.json` by the `theme-gen` workspace binary. It writes `ui/app/src/styles/themes/*.css` with 12-step neutral and ANSI hue ramps interpolated in OKLab/OKLCH space. Split-surface themes additionally emit a `[data-theme="..."] .editor-surface` block so the editor can use a light-paper ramp while the surrounding chrome stays dark. The frontend surfaces that catalog in two places: a flat visual theme gallery in Settings → Appearance with optional follow-system dark/light pair selectors, and a command-palette theme picker that previews themes while the user arrows through results.

### 19.2 Sidebar views

Sidebar views are user-defined in `.notesmith/sidebar.yaml`. By default (no YAML file), the sidebar shows only the Files tab — a standard file/folder tree with no tab bar.

When ≥1 custom view is configured, a tab bar appears at the top of the sidebar. Files is always present and always first. Tabs use a **fixed 2-column grid** (icon + name), wrapping to additional rows as needed. Views support an optional `badge_query` for tab-level badge counts.

FileTree supports same-name folder notes through the same-name markdown convention: `Projects/Atlas/Atlas.md` represents `Projects/Atlas/`. The folder name opens the folder note, the disclosure chevron expands/collapses children, and the duplicate child note is hidden only in that tree position. Folder context menus support opening/creating folder notes and renaming folders; Notesmith-initiated folder renames sync the same-name folder-note filename when present and block collisions instead of rewriting links.

Each view contains **sections** stacked vertically with horizontal separators. Sections are collapsible (state persisted in localStorage) and show item count badges on headers.

#### Section types

| Type | Behavior |
|---|---|
| `recently-viewed` | Shows recently viewed/edited notes. Mode: `viewed \| edited \| both`. Tracked by frontend (localStorage). Default limit: 10. |
| `custom-folders` | Lists configured vault folders. Each renders its tree using the FileTree component, rooted at that folder (leaf name displayed, full path as tooltip). Fully expandable. |
| `custom-items` | Each item has a name and icon (emoji). Clicking opens a **middle pane** between sidebar and reading pane. |

#### Middle pane

Custom items open a resizable middle pane (drag handle, default 300px, width persisted in localStorage per vault + item). Only one item is active at a time — clicking another replaces the content. An explicit close button dismisses the pane. Switching tabs closes it.

Two source variants for custom items:

- **Folder source**: lists notes in a folder (optionally recursive) with title + 2-line body snippet. Default sort: `modified_at DESC`, configurable.
- **Query source**: executes a SQL query and renders results using column mapping (`title_column`, `subtitle_column`, `badge_columns`).

Clicking a note in the middle pane opens it in the reading pane (respecting the user's current view mode). For query-backed items, the reading pane scrolls to the relevant line when the query returns `path` and `line` columns.

#### Config schema example

```yaml
views:
  - id: workflow
    name: "Workflow"
    icon: "⚡"
    badge_query: |
      SELECT count(*)
      FROM v_notes
      WHERE path LIKE 'Inbox/%'
    sections:
      - type: recently-viewed
        label: "Recent"
        mode: both
        limit: 10

      - type: custom-folders
        label: "Workspace"
        folders:
          - "Projects"
          - "Areas"
          - "Reference"

      - type: custom-items
        label: "Focus"
        items:
          - name: "Open tasks"
            icon: "✅"
            source:
              query: |
                SELECT note_path AS path, text AS title, status_label AS subtitle, line
                FROM v_tasks
                WHERE status_group = 'open'
              title_column: "title"
              subtitle_column: "subtitle"
```

### 19.3 Editor experience

The editor is a **v1-critical feature**. It must support full OFM editing with live preview, not just raw markdown text.

Implementation stance:

- CodeMirror 6 is the source editor.
- Live preview is rendered through decorations and inline widgets.
- There is no separate split-preview mode in v1.
- Read mode uses the same tab shell, not a separate pane system.
- Metadata panels expose fields and tags from the unified generic model.

### 19.4 Right dock (Context & Chat)

The right dock is a single collapsible right-side panel with a top-level segmented control switching between two surfaces: **Context** and **Chat**. Both share one column so opening chat never adds a second panel beside the editor, and the active segment is remembered per vault.

The **Context** surface is contextual to the active note, with core tabs:

- **Metadata** — resolved fields and tags for the active note,
- **Links** — outgoing links and backlinks,
- **TOC** — live table of contents from headings.

Metadata tab behavior:

- fields grouped by key,
- repeated values shown explicitly,
- tags rendered separately from fields,
- `_`-prefixed system/UI keys hidden by default.

The **Chat** surface hosts the embedded AI agent (see §ADR 0012). It is mounted lazily on first use — the external agent process is not started until chat is first opened — and then stays mounted so the conversation survives switching back to Context.

### 19.5 Default hotkeys

| Action | Key | Equivalent command |
|---|---|---|
| Open a periodic note picker | `⌘D` | `notesmith periodic open ...` |
| Route current note | `⌘⇧A` | `notesmith route apply <current>` |
| Quick Capture | `⌘⇧N` | `notesmith capture` |
| Toggle current task status | `⌘⏎` | `notesmith task toggle ...` |
| Quick switcher | `⌘O` | note switcher |
| Command palette | `⌘K` and `⌘P` | palette |
| Global search | `⌘⇧F` | search UI |

### 19.6 Passive notification stance

Notesmith has **no push notifications**. Dashboards, sidebar views, task widgets, and periodic notes are the attention surfaces.

## 20. Dashboards

### 20.1 Product stance

Dashboards exist in two forms:

1. **Native Svelte components** — primary
2. **Markdown dashboard files with `notesmith sql` blocks** — secondary and compatibility-friendly

Native dashboards are the default shipped experience. Markdown dashboards remain important because they are editable, versionable, and easy for agents to inspect.

### 20.2 Read-only rule

Dashboard SQL blocks are **strictly read-only**. They must be `SELECT` statements. The renderer rejects mutations and multiple statements.

### 20.3 Example: `Dashboards/Home.md`

````markdown
# Home

## Recently updated notes
```notesmith sql
SELECT title, path, updated_at
FROM v_notes
ORDER BY updated_at DESC
LIMIT 15;
```

## Open tasks with due dates
```notesmith sql
SELECT t.text, t.note_path, tf.field_value AS due
FROM v_tasks t
JOIN v_task_fields tf ON tf.task_hash = t.task_hash
WHERE t.status_group = 'open'
  AND tf.field_key = 'due'
ORDER BY tf.field_value;
```

## This week's periodic notes
```notesmith sql
SELECT kind, period_key, note_path
FROM v_periodic
WHERE period_start >= date('now', '-7 day')
ORDER BY period_start DESC;
```
````

### 20.4 Example: `Dashboards/Inbox.md`

````markdown
# Inbox

## Unrouted notes
```notesmith sql
SELECT path, title, updated_at
FROM v_notes
WHERE path LIKE 'Inbox/%'
ORDER BY updated_at ASC;
```

## Notes missing owner
```notesmith sql
SELECT n.path, n.title
FROM v_notes n
WHERE NOT EXISTS (
  SELECT 1
  FROM v_fields f
  WHERE f.note_path = n.path
    AND f.field_key = 'owner'
)
ORDER BY n.path;
```
````

### 20.5 User-defined dashboards via user views

Because the model is generic, opinionated dashboards should usually be built from user views in `.notesmith/views.sql`.

Example:

```sql
CREATE VIEW v_focus_review AS
SELECT n.path, n.title, n.updated_at
FROM v_notes n
WHERE EXISTS (
  SELECT 1 FROM v_fields f
  WHERE f.note_path = n.path
    AND f.field_key = 'status'
    AND f.field_value = 'active'
)
AND EXISTS (
  SELECT 1 FROM tags t
  WHERE t.note_path = n.path
    AND t.tag = 'focus'
);
```

Then a dashboard note can safely do:

````markdown
```notesmith sql
SELECT * FROM v_focus_review ORDER BY updated_at DESC;
```
````

## 21. Copy as HTML

### 21.1 Feature

Notesmith ships built-in **Copy as HTML** support in both the CLI and editor UI.

### 21.2 CLI contract

```bash
notesmith copy-html "Projects/Atlas.md"
```

Behavior:

- if stdout is piped, emit HTML,
- if stdout is a TTY, copy to the clipboard and print a short confirmation,
- `--stdout` forces stdout behavior.

### 21.3 Implementation

- `comrak` renders CommonMark + GFM.
- Notesmith pre-processes OFM-specific constructs such as wikilinks, embeds, callouts, and task metadata before render.
- The editor's **Copy as HTML** action calls the same Rust library used by the CLI.

## 22. Configuration

### 22.1 Split config model

| Scope | Location | Purpose |
|---|---|---|
| Global | `~/.config/notesmith/config.toml` | daemon defaults, vault registry, UI defaults |
| Per-vault | `.notesmith/` | vault-specific behavior, fields, views, routes, prompts, skill file |

### 22.2 Global config example

```toml
[daemon]
bind = "127.0.0.1:27183"
auto_start = true

default_vault = "work"

[vaults.work]
path = "/Users/surdy/Notes/work"

[vaults.personal]
path = "/Users/surdy/Notes/personal"
```

`daemon.auto_start = true` means daemon-backed CLI commands automatically spawn the HTTP daemon on first use. Setting it to `false` restores the manual `notesmith daemon start` flow.

### 22.3 Per-vault `vault.toml`

`vault.toml` holds operational config, not the field registry.

```toml
schema_version = 1
name = "work"
homepage = "Dashboards/Home.md"

[capture]
folder = "Inbox"
template = "capture-note"
filename = "{{ now | date('%Y-%m-%d %H-%M-%S') }} - {{ prompt.text | slug }}.md"

[editor]
live_preview = true
default_mode = "source"
show_line_numbers = true
hide_duplicate_h1 = true
paste_url_image_whitelist = ""

[periodic.daily]
enabled = true
path = "Journal/Daily/{{ key }}.md"
template = "daily-note"
generate_at = "06:30"
timezone = "America/Los_Angeles"
catch_up = true

[periodic.weekly]
enabled = true
path = "Journal/Weekly/{{ key }}.md"
template = "weekly-review"

[[tasks.statuses]]
char = " "
label = "To Do"
group = "open"
icon = "☐"
order = 10

[[tasks.statuses]]
char = "/"
label = "In Progress"
group = "open"
icon = "◐"
order = 20

[[tasks.statuses]]
char = "x"
label = "Done"
group = "done"
icon = "✅"
order = 90

[git]
enabled = false

[hooks.on_field_change]
command = "Assets/scripts/on-field-change.py"
timeout = "2s"
mode = "fire_and_forget"
watch = ["status", "owner"]
```

### 22.4 Per-vault `fields.toml`

Field registry is separate by design.

```toml
version = 1

[fields.status]
type = "enum"
values = ["idea", "active", "blocked", "done"]
description = "Lifecycle state for notes"

[fields.owner]
type = "string"
multivalue = true
description = "Person or role currently responsible"
autocomplete = { values = ["me", "team", "external"] }

[fields.priority]
type = "integer"
description = "Smaller number = higher priority"

[fields.area]
type = "link"
autocomplete = { sql = "SELECT title AS value FROM v_notes WHERE path GLOB 'Areas/**' ORDER BY title" }
```

### 22.5 Per-vault `views.sql`

User-defined SQL views live in a dedicated file.

```sql
DROP VIEW IF EXISTS v_due_tasks;
CREATE VIEW v_due_tasks AS
SELECT t.note_path, t.text, tf.field_value AS due
FROM v_tasks t
JOIN v_task_fields tf ON tf.task_hash = t.task_hash
WHERE tf.field_key = 'due';
```

### 22.6 Per-vault `routing.yaml`

Routing rules live in YAML.

```yaml
version: 1
defaults:
  on_exists: rename
rules:
  - id: triage-focus
    auto: false
    when:
      all:
        - path_glob: "Inbox/**"
        - tags_include: [focus]
    then:
      move_to: "Projects/Focus/{{ filename }}"
```

### 22.7 Per-vault hidden directory layout

```text
.notesmith/
├── vault.toml
├── fields.toml
├── views.sql
├── routing.yaml
├── sidebar.yaml
├── url-actions.yaml
├── prompts/
│   └── weekly-review.md
└── skill.md
```

Only `.notesmith/` is required. All visible workspace folders are user-defined through convention and config.

### 22.8 Visible assets

Templates and scripts remain visible vault assets under user-chosen folders such as:

```text
Assets/
├── templates/
├── scripts/
└── data/
```

That path is a convention, not a requirement.

### 22.9 Starter kit stance

Starter kits are documented examples only. Notesmith does not bundle a starter kit registry, downloader, installer, or special runtime mode.

The work-notes example belongs in:

```text
docs/example-work-notes-kit.md
```

### 22.10 Schema version stance

`schema_version = 1` in `vault.toml` refers to the **new generic model**. There is no automatic migration from the earlier domain-specific experimental schema.

## 23. Multi-Vault

### 23.1 Core stance

Notesmith is **multi-vault aware** from the start.

### 23.2 Selection model

- CLI prefers git-style `$PWD` detection.
- HTTP always uses named vault routing: `/api/v/{vault-name}/...`.
- Global config stores the named vault registry.

### 23.3 Always-on watching

The daemon watches **all configured vaults all the time**. This ensures immediate index updates even when agents edit files outside the current UI session.

### 23.4 Operational consequences

- caches are per-vault,
- SSE streams are vault-scoped,
- sidebar state is vault-scoped,
- Tauri can switch vaults without restarting the daemon,
- field registries and user views are vault-scoped,
- route rules are vault-scoped.

## 24. Obsidian Compatibility

### 24.1 Compatibility stance

Notesmith is **read-write compatible enough for shared markdown usage**, while still treating Notesmith-specific features as inert extensions in other tools.

That means:

- the vault can still be opened in Obsidian,
- Notesmith does not write into `.obsidian/`,
- `notesmith sql` blocks remain inert code fences instead of corrupting notes,
- task checkbox syntax stays on disk,
- tags stay normal OFM tags,
- attachments stay normal files.

### 24.2 Migration stance

The product model is a **fresh start**, not a migration project.

Specifically:

1. the old domain-specific cache schema is discarded,
2. the old typed note-kind model is discarded,
3. old experimental built-in views like `v_customers` and `v_streams` are not preserved,
4. users who want those projections recreate them as ordinary fields + user-defined views.

This is still friendly to existing markdown notes, but it does **not** try to preserve old Notesmith-internal abstractions.

## 25. File Watching & Conflict Handling

### 25.1 File watching

`notify` watches all configured vault roots. The daemon debounces events, reparses only affected notes, updates the cache and search index, and pushes updates over SSE.

The watcher also observes `.notesmith/` files:

- `vault.toml`,
- `fields.toml`,
- `views.sql`,
- `routing.yaml`,
- `sidebar.yaml`,
- `url-actions.yaml`,
- `skill.md`.

### 25.2 Dirty-buffer behavior

Notesmith uses a VS Code-style conflict policy:

| External change while editor state is… | Behavior |
|---|---|
| Clean | Silent reload |
| Dirty | Prompt the user |

### 25.3 Save-time conflict check

Every write carries an `expected_hash`. If the current note hash differs, the save returns a conflict and the UI presents:

- reload from disk,
- compare changes,
- save as new note,
- overwrite deliberately.

The dirty buffer always remains intact until the user decides.

### 25.4 Config reload rules

- `vault.toml` reload updates in-memory operational settings.
- `fields.toml` reload refreshes the field registry and editor completion data.
- `views.sql` reload rebuilds user-defined views.
- `routing.yaml` reload refreshes the routing AST.
- Invalid config leaves the last valid config active and emits a warning event.

## 26. Performance Targets

These are **aspirational targets**, not promises made before measurement. Build first, measure, then optimize.

| Operation | Target |
|---|---|
| Cold reindex, 10k notes | `< 5s` |
| Incremental reindex after one note save | `< 50ms` |
| Typical dashboard SQL query | `< 100ms` |
| Tantivy search | `< 75ms` |
| GUI startup to first paint with warm daemon | `< 500ms` |
| Copy note as HTML | `< 30ms` |
| Route preview | `< 20ms` |
| Task toggle | `< 20ms` |

## 27. Testing Strategy

### 27.1 Test mix

Notesmith uses:

- **unit tests** for parser, router, tasks, config, and query helpers,
- **integration tests** for CLI and HTTP contracts,
- **snapshot tests** using the `insta` crate.

There is **no Playwright and no browser E2E suite** in v1 beyond targeted headless flows when frontend loading/race bugs require them.

### 27.2 Golden vault fixture

A checked-in `golden-vault/` fixture represents the canonical generic workspace. It should contain:

- notes with frontmatter-only fields,
- notes with inline-only fields,
- notes mixing both sources,
- repeated fields,
- tags from both frontmatter and inline syntax,
- tasks with configurable status characters,
- daily/weekly/monthly/quarterly/yearly notes,
- routing edge cases,
- dashboard notes,
- malformed-content regression fixtures.

### 27.3 Snapshot targets

Snapshot tests cover:

- rendered SQL results,
- template instantiation,
- routing decisions,
- parsed task inventories,
- parsed field inventories,
- copy-as-HTML output,
- sidebar view resolution.

### 27.4 Integration targets

Integration tests run the real `notesmith` binary against the golden vault and hit the real HTTP endpoints. The goal is contract confidence, not UI click simulation.

Required generic-model integration coverage:

1. `v_notes` returns the documented columns.
2. `v_fields` contains frontmatter and inline fields without source distinction in the query contract.
3. `v_tasks` joins status config correctly.
4. `v_task_fields` exposes generic task metadata.
5. `v_backlinks` resolves wikilinks and embeds.
6. `v_periodic` returns all five periodic kinds.
7. dashboard SQL blocks reject non-`SELECT` statements.
8. routing DSL matches fields/tags/path predicates and produces the expected mutations.
9. `on_field_change` batches watched keys per save and emits `add` / `change` / `remove` actions.

### 27.5 Malformed-content requirements

Every new parser, renderer, or indexer that touches note content requires:

1. happy-path test with well-formed input,
2. malformed-content test that degrades without panic,
3. no-panic test for pathological input.

This applies to:

- field extraction,
- task parsing,
- frontmatter parsing,
- routing preview,
- template render preflight,
- dashboard SQL block parsing.

### 27.6 Contract test examples

#### 27.6.1 Exact frontend query test

If the frontend ships a query such as:

```sql
SELECT note_path, text, status_label FROM v_tasks WHERE status_group = 'open'
```

an integration test should execute that exact query against a real indexed SQLite database.

#### 27.6.2 Route preview regression

Given this note state:

```markdown
---
status: active
---

# Atlas

#focus
```

and this rule:

```yaml
when:
  all:
    - field:
        key: status
        op: eq
        value: active
    - tags_include: [focus]
then:
  move_to: "Projects/Focus/{{ filename }}"
```

the snapshot should assert the preview plan exactly.

## 28. Implementation Phases

### 28.1 Phase list

| Phase | Scope | Exit criterion |
|---|---|---|
| Phase 0 | Schema reset + TurboVault evaluation spike | Decide keep vs swap behind `VaultEngine`; finalize generic cache schema |
| Phase 1 | Generic parser/index foundation | Notes parse into note/field/tag/task/link primitives; cache builds successfully |
| Phase 2 | Stable SQL contract | `v_notes`, `v_fields`, `v_tasks`, `v_task_fields`, `v_backlinks`, `v_periodic` live and tested |
| Phase 3 | Template + capture pipeline | Three-layer template context works; capture delegates internally to templates |
| Phase 4 | Task status config + periodic engine | Configurable task statuses and all five periodic kinds ship |
| Phase 5 | Routing DSL + route log | Manual and auto routing work; undo via `route_log` works |
| Phase 6 | Hooks + field/task change events | All six hooks implemented with watched-key batching |
| Phase 7 | HTTP/CLI/MCP contract hardening | Generic API and CLI fully aligned, skill file updated |
| Phase 8 | GUI + dashboards + docs | Generic metadata UI, read-only dashboards, docs/example-work-notes-kit.md |
| Phase 9 | Hardening | performance, malformed-content resilience, packaging |

### 28.2 Phase 0 — fresh start

Phase 0 explicitly discards the earlier domain-specific assumptions.

Deliverables:

- remove typed note-kind schema assumptions from plan and code,
- define fresh SQLite schema,
- define fresh config surface,
- decide whether TurboVault stays behind `VaultEngine`.

### 28.3 Phase 1 — parser/index baseline

Deliverables:

- unified field extraction,
- separate tag extraction,
- generic link extraction,
- task extraction with raw `status_char`,
- periodic stamp extraction,
- cache rebuild path.

### 28.4 Phase 2 — SQL contract

Deliverables:

- stable views,
- read-only SQL validator,
- `.notesmith/views.sql` loader,
- query API,
- integration tests for exact frontend queries.

### 28.5 Phase 3 — template + capture

Deliverables:

- static context layer,
- SQL context layer,
- hook enrichment layer,
- capture command delegating internally to templates,
- prompt fulfillment across CLI, GUI, HTTP.

### 28.6 Phase 4 — tasks + periodics

Deliverables:

- config-backed task status registry,
- task toggling and explicit status set,
- daily/weekly/monthly/quarterly/yearly creation,
- `v_periodic` tests.

### 28.7 Phase 5 — routing

Deliverables:

- YAML predicate parser,
- first-match rule evaluation,
- mutation planner,
- manual preview/apply,
- auto-route on save,
- `route_log` + undo.

### 28.8 Phase 6 — hooks

Deliverables:

- six event payloads,
- watched-key scoping for `on_field_change`,
- batched per-save events,
- timeout/error handling,
- enrichment merge for create/periodic templates.

### 28.9 Phase 7 — surface alignment

Deliverables:

- HTTP endpoints updated to generic model,
- CLI commands updated to generic model,
- MCP parity for note/query/route/task/periodic actions,
- `.notesmith/skill.md` updated.

### 28.10 Phase 8 — GUI + docs

Deliverables:

- metadata panel shows fields + tags separately,
- dashboard renderer enforces read-only SQL,
- sidebar examples use generic queries,
- `docs/example-work-notes-kit.md` documents the opinionated work-notes workflow outside the core architecture.

### 28.11 Phase 9 — hardening

Deliverables:

- malformed-content regression fixtures,
- performance measurements,
- packaging polish,
- operational diagnostics.

## 29. Open Questions & Deferrals

### 29.1 Not open anymore

These are settled by this blueprint:

- fields are unified in the query API,
- tags are separate from fields,
- task statuses are configurable and store raw `status_char` + resolved group,
- field registry lives in `.notesmith/fields.toml`,
- periodic kinds are daily/weekly/monthly/quarterly/yearly,
- route log stays,
- the hook list is exactly six events,
- routing uses YAML DSL rather than raw SQL,
- manual routing is default; auto routing is per-rule opt-in,
- starter kits are docs only,
- user-defined SQL views live in `.notesmith/views.sql`,
- capture stays first-class while delegating internally to templates,
- routing `then` supports full mutation,
- templates have three context layers,
- `on_field_change` is watched-key scoped and batched,
- there is no special relationship system,
- dashboard SQL is read-only,
- fields store text + type hint,
- Tantivy stays,
- crate boundaries stay,
- only `.notesmith/` is required,
- `.notesmith/skill.md` stays,
- this is a fresh start with no migration,
- work-notes moves to `docs/example-work-notes-kit.md`.

### 29.2 Actual deferrals

| Topic | Decision |
|---|---|
| Hosted multi-user authz | Deferred; core remains auth-ignorant |
| Visual route builder | Deferred; YAML remains source of truth |
| Writable dashboard blocks | Deliberately excluded; dashboards stay read-only |
| Built-in starter kit registry | Deliberately excluded |
| Special relationship graph model | Deliberately excluded |
| Cross-vault SQL joins | Deferred |
| Mobile app | Deferred |
| Push notifications | Deliberately excluded; passive surfaces only |
| Split panes | Deferred; tabs only in v1 |

### 29.3 Final stance

Notesmith is now specified as a **generic programmable markdown workspace** with explicit, inspectable primitives. Domain workflows are built from fields, tags, tasks, views, templates, routes, and hooks — not hardcoded into the core data model.

This document is the definitive build blueprint for Notesmith.
