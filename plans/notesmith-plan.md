# Notesmith Plan — Definitive Architectural Blueprint

Notesmith is the final synthesized plan for the custom markdown notes application that replaces Obsidian for the workflow in `notes-method.md`. It supersedes the earlier model plans; Opus 4.7 is the base, but every decision in this document takes precedence.

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
- [11. Capture Workflow](#11-capture-workflow)
- [12. Daily Notes](#12-daily-notes)
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
3. **Custom sidebar views.** The left rail is not a dumb file tree; it supports virtual folders and metadata-based grouping driven by frontmatter.

The product name is **Notesmith**.

### 1.2 Design principles

1. **Files are the source of truth.** The vault on disk is authoritative. SQLite, Tantivy, and in-memory indexes are caches.
2. **HTTP-first everywhere.** The daemon speaks REST + SSE natively. The GUI, CLI, and MCP adapter all sit on top of that surface.
3. **Agents are first-class users.** Every important action is available from the `notesmith` CLI and described in a skill file.
4. **Plain markdown round-trips losslessly.** The vault remains valid Obsidian-flavored markdown and remains readable outside Notesmith.
5. **No plugin system.** Templates, tasks, routing, dashboards, git sync, sidebar views, URL handling, and copy-as-HTML are built in.
6. **Thin desktop shell.** Tauri exists to provide a native window, deep links, tray integration, and OS affordances. The app itself is served by the daemon.
7. **Multi-vault from day one.** Vault naming, routing, caching, and watching all assume more than one vault.
8. **Pragmatic over ornamental.** Tabs ship in v1; splits do not. Passive dashboards ship; notifications do not.

### 1.3 Obsidian plugin → Notesmith built-in mapping

| Obsidian plugin / feature | Notesmith built-in | Final stance |
|---|---|---|
| Templater | `notesmith-templates` with `minijinja` + subprocess hooks | Replaced; no embedded JS runtime |
| Tasks | Built-in task parser + SQL-backed task views | Replaced; 7-status model on disk |
| Dataview | SQL over SQLite cache via `notesmith sql` blocks | Replaced; SQL only |
| QuickAdd | CLI + command palette + URL scheme + quick capture | Replaced |
| Auto Note Mover | Routing engine with YAML rules | Replaced |
| Periodic Notes + Calendar | Agent-driven daily notes + daemon fallback scheduler + calendar UI | Replaced |
| Homepage | Homepage config + native dashboard opening `Dashboards/Home.md` | Replaced |
| Linter | Save pipeline in Rust | Replaced |
| Hotkeys for specific files | Command palette entries + configurable shortcuts | Replaced |
| Bookmarks | Built-in pinned items and sidebar views | Replaced |
| Obsidian Git | Thin opt-in git integration | Replaced |
| Bases | Native tables/components over SQL results | Replaced |

### 1.4 Builder reality

The architecture is optimized for a workflow where the **user guides architecture and agents write most of the code**. That means:

- Rust is used for the durable core and CLI contract.
- SvelteKit is used because it is the user's preferred UI stack and is fast to iterate on with agent assistance.
- The system is decomposed into sharp library seams so codegen and review are tractable.

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
| Frontmatter | **serde + serde_yaml** | Typed YAML parsing with preservation of unknown fields |
| Structured query cache | **SQLite via rusqlite** | Rebuildable local cache for views and dashboard queries |
| Full-text search | **Tantivy** | Embedded full-text search index separate from the SQLite cache |
| Template engine | **minijinja** | Jinja2-like syntax, sandboxed, deterministic |
| File watching | **notify** | Cross-platform watcher for all configured vaults |
| Git integration | **git2** | Built-in opt-in commit/pull/push timers |
| Hashing | **blake3** | Fast content hashes for notes and task anchoring |
| Config formats | **TOML + YAML + Markdown** | TOML for config, YAML for rules/views, Markdown for prompts/templates |

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
│       ├──────── Templates / Routing / Tasks / Git / Hooks          │
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

- The daemon is the long-lived owner of file watching, indexing, routing, scheduler fallback, and SSE fan-out.
- It is launched with `notesmith daemon start`.
- Default bind address is `127.0.0.1:27183`.
- `--bind` can expose it elsewhere, but the daemon itself remains auth-ignorant.
- The compiled SvelteKit app is served by the daemon under `/app/`.
- There is **no separate `notesmithd` binary**.

### 3.3 Tauri shell role

Tauri is a **thin shell pointing at localhost**. It is responsible for:

- starting the daemon if needed,
- opening the native window onto the local app URL,
- registering `notesmith://` deep links,
- exposing the system tray and basic native menu items.

The Tauri shell does **not** own business logic, query execution, or note state.

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
    fn scan(&self, root: &Path) -> anyhow::Result<Vec<Note>>;
    fn read(&self, root: &Path, path: &VaultPath) -> anyhow::Result<String>;
    fn write(
        &self,
        root: &Path,
        path: &VaultPath,
        expected_hash: Option<blake3::Hash>,
        content: &str,
    ) -> anyhow::Result<WriteResult>;
    fn move_path(&self, root: &Path, from: &VaultPath, to: &VaultPath) -> anyhow::Result<()>;
    fn watch(&self, root: &Path) -> anyhow::Result<Box<dyn VaultWatcher>>;
}

trait VaultOps {
    fn note_create(&self, req: CreateNoteReq) -> anyhow::Result<NoteSummary>;
    fn note_get(&self, req: GetNoteReq) -> anyhow::Result<Note>;
    fn note_put(&self, req: PutNoteReq) -> anyhow::Result<NoteSummary>;
    fn route_apply(&self, req: RouteApplyReq) -> anyhow::Result<RouteResult>;
    fn task_toggle(&self, req: ToggleTaskReq) -> anyhow::Result<Task>;
    fn capture(&self, req: CaptureReq) -> anyhow::Result<NoteSummary>;
    fn daily_ensure(&self, req: EnsureDailyReq) -> anyhow::Result<NoteSummary>;
    fn query_sql(&self, req: SqlQueryReq) -> anyhow::Result<QueryResult>;
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
│   ├── notesmith-query/       # stable views + SQL execution + dashboard renderer helpers
│   ├── notesmith-templates/   # minijinja env, prompt specs, template instantiation
│   ├── notesmith-routing/     # YAML rules, destination resolver, archive workflow
│   ├── notesmith-tasks/       # task parsing, transitions, content-hash matching
│   ├── notesmith-hooks/       # subprocess hook runner
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

## 5. Data Model

### 5.1 Canonical `Note` type

```rust
pub struct Note {
    pub vault: VaultName,
    pub path: VaultPath,
    pub frontmatter: Frontmatter,
    pub body: String,
    pub ast: Option<Ast>,
    pub blocks: Vec<Block>,
    pub links: Vec<Link>,
    pub inline_fields: Vec<InlineField>,
    pub mtime: SystemTime,
    pub hash: blake3::Hash,
}
```

Everything important is derived from `Note` plus a parsed task/link/block inventory.

### 5.2 Canonical `Frontmatter` type

`Frontmatter` is closed for known note kinds and open for unknown kinds:

```rust
pub enum Frontmatter {
    Daily(DailyMeta),
    Meeting(MeetingMeta),
    Stream(StreamMeta),
    Customer(CustomerMeta),
    AccountInfo(AccountInfoMeta),
    Glossary(GlossaryMeta),
    Milestones(MilestonesMeta),
    Note(NoteMeta),
    Other(serde_yaml::Mapping),
}
```

Unknown note types are preserved verbatim. Notesmith never strips user metadata just because it does not understand it.

### 5.3 Frontmatter schema contract

| Key | Type | Values / shape | Notes |
|---|---|---|---|
| `type` | string | `daily`, `meeting`, `stream`, `customer`, `account-info`, `glossary`, `milestones`, `note` | Primary discriminator |
| `meeting-kind` | string | `internal`, `external` | Meeting notes only |
| `customer` | wikilink string | `[[Acme Corp]]` | Always stored as a wikilink on disk |
| `stream` | wikilink string / null | `[[Migration to v2]]` | Optional on meetings and notes |
| `state` | string | `Active`, `On Hold`, `Temp`, `Inactive` | Customer index note only |
| `status` | string | `In Progress`, `Blocked`, `Done`, `Awaiting Customer`, `On Hold` | Stream note only |
| `priority` | string | `P0`, `P1`, `P2`, `P3` | Stream note only |
| `owner` | string | `me`, `customer`, or free text | Streams/tasks |
| `date` | date | `YYYY-MM-DD` | Daily and meeting notes |
| `started` / `target` | date | `YYYY-MM-DD` | Streams |
| `archived` | bool | `true` / `false` | Routing state |
| `archived-at` | datetime | `YYYY-MM-DD HH:mm` | Stamped by router |
| `created` / `updated` | datetime | `YYYY-MM-DD HH:mm` | Maintained by save pipeline |
| `tags` | list | YAML array | Lightweight tagging only |

### 5.4 Canonical visible vault structure

```text
Capture/
Daily/
Tasks/
  Tasks - Active.md
  Tasks - Blocked.md
  Tasks - Awaiting Customer.md
  Tasks - On Hold.md
  Tasks - By Customer.md
Customers/
  <Customer>/
    <Customer>.md
    Account Info/
      Account Info.md
      Glossary.md
      Dates and Milestones.md
    Internal Meetings/
    External Meetings/
    Streams/
General/
  Journal/
Dashboards/
  Home.md
  Capture Triage.md
  Customers.md
  Streams.md
Assets/
  templates/
  scripts/
  data/
.notesmith/
  vault.toml
  routing.yaml
  sidebar-views.yaml
  prompts/
  skill.md
```

### 5.5 Naming conventions

| Artifact | Convention |
|---|---|
| Customer index note | `Customers/<Customer>/<Customer>.md` |
| Meeting note | `YYYY-MM-DD - <Customer> - <Internal|External> - <Topic>.md` |
| Stream note | Human-readable stream name |
| Daily note | `YYYY-MM-DD.md` |
| Default attachment location | `Assets/data/` |

### 5.6 OFM support contract

| Syntax | Meaning | Notes |
|---|---|---|
| `[[Wiki Link]]`, `[[Note\|alias]]`, `[[Note#Heading]]`, `[[Note#^block-id]]` | Wikilinks | Resolved in Notesmith; preserved verbatim on disk |
| `![[Embed]]` | Embed | Rendered inline in preview |
| `> [!note]` etc. | Callouts | Full OFM callout rendering |
| `- [ ]`, `- [/]`, `- [b]`, `- [w]`, `- [h]`, `- [x]`, `- [-]` | Task states | Canonical task syntax |
| `[key:: value]` | Inline field | Indexed into SQLite cache |
| ```` ```notesmith sql ```` | Live SQL block | Executed by Notesmith; inert in Obsidian |
| `%% comment %%` | Obsidian comments | Preserved in source |
| `^block-id` | Block references | Indexed for backlink resolution |
| `==highlight==` | Highlight | Preserved |
| Attachments in `Assets/data/` | Passthrough files | Served statically; no custom asset pipeline |

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

```sql
CREATE TABLE notes (
  vault_name TEXT NOT NULL,
  path TEXT NOT NULL,
  title TEXT NOT NULL,
  type TEXT NOT NULL,
  frontmatter_json TEXT NOT NULL,
  customer TEXT,
  stream TEXT,
  state TEXT,
  status TEXT,
  date TEXT,
  created_at TEXT,
  updated_at TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  mtime_unix INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  body_excerpt TEXT NOT NULL,
  PRIMARY KEY (vault_name, path)
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

CREATE TABLE inline_fields (
  vault_name TEXT NOT NULL,
  note_path TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  value_json TEXT
);

CREATE TABLE tasks (
  vault_name TEXT NOT NULL,
  task_hash TEXT NOT NULL,
  note_path TEXT NOT NULL,
  heading_path TEXT,
  ordinal INTEGER NOT NULL,
  status TEXT NOT NULL,
  text TEXT NOT NULL,
  customer TEXT,
  stream TEXT,
  owner TEXT,
  due TEXT,
  scheduled TEXT,
  start_date TEXT,
  done_at TEXT,
  priority INTEGER,
  recurrence TEXT,
  raw_markdown TEXT NOT NULL,
  PRIMARY KEY (vault_name, task_hash)
);
```

### 6.4 Stable SQL view contract

**Views are the API. Tables are implementation detail.**

Day-one required views:

```sql
CREATE VIEW v_notes AS
SELECT
  vault_name,
  path,
  title,
  type,
  customer,
  stream,
  state,
  status,
  date,
  created_at,
  updated_at,
  archived,
  mtime_unix,
  frontmatter_json
FROM notes;

CREATE VIEW v_tasks AS
SELECT
  t.vault_name,
  t.task_hash,
  t.note_path,
  t.heading_path,
  t.ordinal,
  t.status,
  t.text,
  t.customer,
  t.stream,
  t.owner,
  t.due,
  t.scheduled,
  t.start_date,
  t.done_at,
  t.priority
FROM tasks t;

CREATE VIEW v_backlinks AS
SELECT
  dst_path AS note_path,
  src_path AS backlink_path,
  kind,
  heading_ref,
  block_ref
FROM links
WHERE dst_path IS NOT NULL;
```

Additive follow-on views, introduced when dashboards need them:

```sql
CREATE VIEW v_customers AS
SELECT * FROM v_notes WHERE type = 'customer';

CREATE VIEW v_streams AS
SELECT * FROM v_notes WHERE type = 'stream';
```

This satisfies both decisions: start minimal, but use SQL views as the stable contract and grow additively.

### 6.5 Search index

Tantivy holds the full-text index. SQLite handles structured queries; Tantivy handles ranking and tokenized text search. The daemon keeps them in sync from the same parse pass.

## 7. Query System

### 7.1 One query language: SQL

There is **no DQL, no NDQL, and no Tasks DSL**. Notesmith uses SQL only.

That choice is deliberate:

- agents already understand SQL,
- dashboards gain a stable, inspectable contract,
- there is no second parser to maintain,
- compatibility complexity moves into views, not user syntax.

### 7.2 Query surfaces

| Surface | Form |
|---|---|
| Markdown notes | `notesmith sql` fenced code blocks |
| CLI | `notesmith query sql ...` |
| HTTP API | `POST /api/v/{vault-name}/query/sql` |
| Native dashboards | Stored SQL snippets executed against stable views |
| Templates | `query(sql)` helper inside minijinja |

### 7.3 Markdown syntax

````markdown
```notesmith sql
SELECT title, state, updated_at
FROM v_customers
WHERE state = 'Active'
ORDER BY title;
```
````

Only SQL is valid inside the block. The renderer executes the statement against the named vault and renders the result as a table, list, or chart depending on the surrounding component.

### 7.4 Query execution rules

1. Default execution target is the stable `v_*` view layer.
2. Raw base-table access is allowed only for debugging and internal tooling.
3. Query blocks are read-only.
4. The renderer never writes query results back into notes.
5. Query failures render visible errors inline rather than silently failing.

## 8. Template Engine

### 8.1 Engine choice

Notesmith uses **minijinja**. It replaces Templater with a deterministic, sandboxed engine that is easy for both humans and agents to reason about.

### 8.2 Template file format

Templates live in `Assets/templates/*.md.j2` and use a YAML preamble for prompt metadata, followed by the actual note markdown.

```markdown
---
notesmith:
  name: external-meeting
  description: Customer external meeting
  output_path: "{{ today }} - {{ customer }} - External - {{ topic }}.md"
  prompts:
    - { name: customer, type: customer-picker, required: true }
    - { name: stream, type: stream-picker, required: false }
    - { name: topic, type: text, required: true }
---
---
type: meeting
meeting-kind: external
customer: "[[{{ customer }}]]"
stream: "{% if stream %}[[{{ stream }}]]{% endif %}"
date: {{ today }}
created: {{ now }}
updated: {{ now }}
archived: false
tags: [meeting, external]
---

# {{ today }} — [[{{ customer }}]] — External: {{ topic }}

## Agenda

## Notes

## Decisions

## Customer asks

## Action items (ours)
- [ ] Follow up [customer:: [[{{ customer }}]]]{% if stream %} [stream:: [[{{ stream }}]]]{% endif %}

## Action items (theirs)
- [w] Awaiting response [customer:: [[{{ customer }}]]] [owner:: customer]
```

### 8.3 Built-in helpers

| Helper | Purpose |
|---|---|
| `today`, `tomorrow`, `yesterday`, `now` | Date helpers |
| `query(sql)` | Execute SQL against the stable views |
| `customer(name)` | Lookup typed customer metadata |
| `slug(s)`, `title_case(s)` | String helpers |
| `prompt(name)` | Access fulfilled prompt values |
| `next_id()` | Stable monotonic IDs when needed |
| `as_wikilink` | Convert string to `[[Name]]` |

### 8.4 Prompt fulfillment

Prompt fulfillment is identical across surfaces:

- **CLI:** flags or interactive fallback when attached to a TTY.
- **HTTP:** request body includes `prompts`.
- **GUI:** modal prompt form.
- **Agents:** supply prompt values directly through CLI or API.

### 8.5 Replacing Templater JavaScript

The escape hatch is **subprocess hooks**, not embedded JavaScript. Hooks receive structured JSON on stdin and can emit structured JSON on stdout. That keeps the core small and the behavior inspectable.

## 9. Routing Engine

### 9.1 Goal

The routing engine replaces Auto Note Mover and the old archive script with a deterministic rules engine.

### 9.2 Rule file

Rules live in `.notesmith/routing.yaml`.

```yaml
version: 1
defaults:
  on_exists: rename
rules:
  - id: archive-external-meeting
    when:
      path: "Capture/**"
      frontmatter.type: meeting
      frontmatter.meeting-kind: external
    then:
      move_to: "Customers/{{ frontmatter.customer | unwikilink }}/External Meetings/{{ filename }}"

  - id: archive-internal-meeting
    when:
      path: "Capture/**"
      frontmatter.type: meeting
      frontmatter.meeting-kind: internal
    then:
      move_to: "Customers/{{ frontmatter.customer | unwikilink }}/Internal Meetings/{{ filename }}"

  - id: archive-stream
    when:
      path: "Capture/**"
      frontmatter.type: stream
    then:
      move_to: "Customers/{{ frontmatter.customer | unwikilink }}/Streams/{{ filename }}"

  - id: archive-daily
    when:
      path: "Daily/**"
      frontmatter.type: daily
    then:
      move_to: "General/Journal/{{ frontmatter.date | strftime('%Y/%m') }}/{{ filename }}"
```

### 9.3 Destination resolution contract

| Note shape | Destination |
|---|---|
| `type: meeting`, `meeting-kind: external`, customer `X` | `Customers/X/External Meetings/` |
| `type: meeting`, `meeting-kind: internal`, customer `X` | `Customers/X/Internal Meetings/` |
| `type: stream`, customer `X` | `Customers/X/Streams/` |
| `type: account-info`, `glossary`, or `milestones` | `Customers/X/Account Info/` |
| `type: customer` | `Customers/X/` |
| `type: daily` | `General/Journal/YYYY/MM/` |
| `type: note`, customer `X`, stream `S` | `Customers/X/Streams/` |
| `type: note`, customer `X`, no stream | `Customers/X/` |
| `type: note`, no customer | `General/` |

Done streams stay in place. Routing does not relocate them to `Archive/`.

### 9.4 Execution

- `notesmith route preview <path>` shows the matched rule and final path.
- `notesmith route apply <path>` moves the note atomically.
- `notesmith route apply <path>` routes a selected note once it is ready.
- Routing stamps `archived: true` and `archived-at: <now>`.

## 10. Task Engine

### 10.1 Canonical task syntax

```markdown
- [ ] Send updated SOW [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]] [owner:: me] 🔼 📅 2026-05-15
- [/] Draft pricing model [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]] 🛫 2026-05-10 📅 2026-05-12
- [b] Blocked on security review [customer:: [[Acme Corp]]] [stream:: [[SSO rollout]]]
- [w] Awaiting redlines [customer:: [[Acme Corp]]] [owner:: customer] ⏳ 2026-05-15
- [h] On hold until next quarter [customer:: [[Globex]]]
- [x] Sent intro email ✅ 2026-05-07 [customer:: [[Acme Corp]]]
- [-] Cancelled — superseded by v3 [customer:: [[Acme Corp]]]
```

### 10.2 Seven statuses

| Symbol | Name | Category | Next states |
|---|---|---|---|
| ` ` | To Do | open | `/`, `b`, `w`, `h`, `x` |
| `/` | In Progress | open | `x`, `b`, `w`, `h` |
| `b` | Blocked | open | ` `, `/`, `x` |
| `w` | Awaiting Customer | open | ` `, `/`, `x` |
| `h` | On Hold | open | ` `, `/`, `x` |
| `x` | Done | closed | ` ` |
| `-` | Cancelled | closed | ` ` |

### 10.3 Metadata extraction

The parser extracts:

- customer and stream links,
- owner,
- due / scheduled / start / done dates,
- recurrence,
- priority emoji,
- raw markdown for round-trip preservation.

### 10.4 Content-hash anchored toggling

Task updates are **not line-number based**.

1. Each parsed task gets a `task_hash = blake3(normalize(text + inline fields + heading path))`.
2. The UI and API submit `note_path + task_hash` when toggling status.
3. The mutator reparses the note, finds the matching normalized task content, and rewrites only that task line.
4. If multiple tasks collide, the operation returns a conflict instead of guessing.

This survives line insertions, formatting shifts, and most agent edits.

### 10.5 Querying tasks

Task lists are SQL projections over `v_tasks`. Convenience commands such as `notesmith task active` are wrappers that generate SQL, not a second query language.

## 11. Capture Workflow

### 11.1 First-class capture

Capture is a first-class workflow with dedicated API, CLI, URL, and GUI entry points.

### 11.2 Capture surfaces

| Surface | Form |
|---|---|
| HTTP | `POST /api/v/{vault-name}/capture` |
| CLI | `notesmith capture "text"` |
| URL scheme | `notesmith://app/capture/{vault-name}?text=...` |
| GUI | Quick Capture command + `⌘⇧N` hotkey |

### 11.3 Capture behavior

By default, capture creates a new note in the configured capture folder using the configured capture template and a timestamp-based filename. When `capture.folder = ""`, the note is created in the vault root:

```text
2026-05-08 08-15-00 - follow-up-with-acme.md
```

### 11.4 Capture backlog workflow

1. Capture quickly.
2. Enrich or rewrite the note as needed.
3. Use `Archive current note` or `route apply` once the note is ready for long-term placement.
4. Use the capture dashboard until the backlog returns to zero.

## 12. Daily Notes

### 12.1 Primary path: external agent

Daily note creation is **primarily agent-driven**.

The agent reads a saved prompt template at `.notesmith/prompts/daily-note.md`, runs SQL context queries, calls an LLM externally, and writes the resulting note back through the CLI or HTTP API.

Example prompt template:

```markdown
---
name: daily-note
output_path: "{{ date }}.md"
context_queries:
  overdue_tasks: |
    SELECT text, customer, stream, due
    FROM v_tasks
    WHERE status IN ('todo', 'in_progress')
      AND due IS NOT NULL
      AND date(due) < date('now')
    ORDER BY due;
  today_meetings: |
    SELECT title, customer, path
    FROM v_notes
    WHERE type = 'meeting' AND date = date('now')
    ORDER BY title;
  open_streams: |
    SELECT title, customer, status, updated_at
    FROM v_streams
    WHERE status != 'Done'
    ORDER BY updated_at DESC
    LIMIT 15;
---
Write today's daily note. Include: top priorities, overdue items, meeting prep, and a short scratch area.
```

### 12.2 Fallback path: daemon scheduler

The daemon includes a built-in fallback scheduler so the system still works when no agent runs.

```toml
[daily]
folder = ""
template = "daily-note"
generate_at = "06:30"
timezone = "America/Los_Angeles"
catch_up = true
```

### 12.3 CLI contract

- `notesmith daily agent-create` uses the saved prompt template and external agent path.
- `notesmith daily ensure --date 2026-05-08` uses the built-in fallback path.
- `notesmith daily open` opens today's note, creating it if required.

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

### 14.2 v1 events only

v1 ships exactly two vault-level hook events:

- `on_note_create`
- `on_daily_create`

### 14.3 Example config

```toml
[hooks]
on_note_create = "Assets/scripts/on-note-create.py"
on_daily_create = "Assets/scripts/on-daily-create.py"
```

### 14.4 Payload shape

```json
{
  "event": "on_note_create",
  "vault": "work",
  "path": "2026-05-08 08-15-00 - follow-up-with-acme.md",
  "frontmatter": {"type": "note"},
  "source": "cli"
}
```

The runner is extensible, but the shipped event list stays intentionally small in v1.

## 15. HTTP API Design

### 15.1 Principles

- REST over HTTP.
- SSE for real-time updates.
- Multi-vault addressing in the URL.
- Local daemon is unauthenticated by design.

### 15.2 Endpoint table

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/ping` | health check |
| `GET` | `/app/*` | compiled SvelteKit app |
| `GET` | `/api/v/{vault}/notes` | list notes |
| `POST` | `/api/v/{vault}/notes` | create note |
| `GET` | `/api/v/{vault}/notes/{path...}` | fetch note |
| `PUT` | `/api/v/{vault}/notes/{path...}` | replace note contents |
| `PATCH` | `/api/v/{vault}/notes/{path...}` | patch frontmatter/body segments |
| `DELETE` | `/api/v/{vault}/notes/{path...}` | delete note |
| `POST` | `/api/v/{vault}/notes/{path...}/append` | append markdown |
| `POST` | `/api/v/{vault}/notes/{path...}/move` | move note |
| `POST` | `/api/v/{vault}/route/preview` | preview destination |
| `POST` | `/api/v/{vault}/route/apply` | route one or more notes |
| `POST` | `/api/v/{vault}/capture` | quick capture |
| `GET` | `/api/v/{vault}/daily/{date}` | fetch daily note |
| `POST` | `/api/v/{vault}/daily/{date}` | create daily note fallback |
| `POST` | `/api/v/{vault}/daily/agent-create` | agent-driven daily workflow |
| `GET` | `/api/v/{vault}/tasks` | list tasks |
| `POST` | `/api/v/{vault}/tasks` | add task |
| `POST` | `/api/v/{vault}/tasks/toggle` | content-hash anchored toggle |
| `POST` | `/api/v/{vault}/query/sql` | execute read-only SQL |
| `GET` | `/api/v/{vault}/search` | Tantivy search |
| `GET` | `/api/v/{vault}/templates` | list templates |
| `POST` | `/api/v/{vault}/templates/{name}/render` | render template |
| `POST` | `/api/v/{vault}/templates/{name}/instantiate` | create note from template |
| `GET` | `/api/v/{vault}/sidebar-views` | resolved sidebar model |
| `GET` | `/api/v/{vault}/events` | SSE stream |
| `GET` | `/api/v/{vault}/assets/{path...}` | serve attachment / asset file |
| `POST` | `/api/v/{vault}/copy-html` | render note to HTML fragment/full doc |

### 15.3 SSE event types

```text
note.created
note.updated
note.moved
note.deleted
task.updated
note.captured
daily.created
cache.rebuilt
search.reindexed
```

SSE is chosen because it is simpler than WebSocket, easier to proxy, and completely adequate for Notesmith's one-way real-time update needs.

### 15.4 Example API call

```bash
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT title, state FROM v_customers ORDER BY title"}'
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

  note create|get|put|append|delete|move
  template list|render|instantiate
  route preview|apply
  task list|add|toggle|set-status
  capture
  daily open|ensure|agent-create
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
- `notesmith mcp start` remains independent of the HTTP daemon and serves stdio requests from its own in-memory indexes.

### 16.5 Pipe-friendly examples

```bash
notesmith query sql "SELECT title FROM v_customers WHERE state = 'Active'" --format json | jq '.[].title'

notesmith capture "Need follow-up with Acme" --vault work

notesmith task list --format json | jq '.[] | select(.status == "blocked")'

notesmith copy-html "Customers/Acme Corp/Account Info/Account Info.md" --stdout | pbcopy
```

## 17. URL Scheme

### 17.1 Namespaces

- `notesmith://app/...` — built-in Notesmith actions
- `notesmith://user/...` — reserved for user-defined shortcuts and custom actions

This namespacing is first-class in v1.

### 17.2 Built-in actions

| URL | Effect |
|---|---|
| `notesmith://app/open?vault=work&path=Customers/Acme%20Corp/Acme%20Corp.md` | Open a note |
| `notesmith://app/search?vault=work&query=sso` | Open search results |
| `notesmith://app/template/new?vault=work&name=external-meeting` | New note from template |
| `notesmith://app/capture/work?text=Follow%20up%20with%20Acme` | Quick capture |
| `notesmith://app/daily/today?vault=work` | Open today's daily note |
| `notesmith://app/copy-html?vault=work&path=Dashboards/Home.md` | Copy note as HTML |

### 17.3 User namespace

User actions are defined in `.notesmith/url-actions.yaml`.

```yaml
version: 1
actions:
  standup:
    run:
      - notesmith
      - daily
      - agent-create
      - --vault
      - work
```

`notesmith://user/standup` resolves through that table.

## 18. Agent Integration

### 18.1 Priority order

1. **CLI + skill file** — primary
2. **MCP** — fallback for GUI-only agent clients

There is **no ACP server**.

### 18.2 Why CLI + skill is primary

A skill file is more context-efficient than forcing an agent to rediscover the entire tool surface from an RPC schema. It can encode exact commands, vault shape, note conventions, and workflow recipes in the user's language.

### 18.3 Skill file contents

The canonical per-vault skill file is `.notesmith/skill.md`. It contains:

- command cheat sheet,
- vault folder structure,
- note type schema,
- stable SQL view contract,
- daily note workflow,
- capture triage workflow,
- routing rules summary,
- examples for common tasks.

Example excerpt:

```markdown
# Notesmith Skill

## Create a customer meeting note
notesmith template instantiate external-meeting --vault work --prompt customer="Acme Corp" --prompt topic="QBR"

## List active streams
notesmith query sql "SELECT title, customer FROM v_streams WHERE status != 'Done' ORDER BY title"

## Archive a prepared captured note
notesmith route apply "2026-05-08 - Acme - External - QBR.md"
```

### 18.4 MCP scope

The MCP adapter exposes only existing operations such as note read/write, SQL query, routing, capture workflows, and daily note creation. It exists for clients that cannot run the CLI directly, and it serves those operations from its own in-memory indexes rather than proxying through the HTTP daemon.

## 19. GUI Design

### 19.1 Core UI layout

The GUI is a SvelteKit app served by the daemon and wrapped by Tauri.

v1 layout:

- left sidebar,
- tab bar,
- primary editor/view tab,
- right rail for backlinks, fields, and note metadata,
- command palette as the primary navigation surface.

Tabs ship in v1. Split panes do not.

Theme assets are generated at build time from `ui/app/src/styles/theme-catalog.json` by the `theme-gen` workspace binary. It writes `ui/app/src/styles/themes/*.css` with 12-step neutral and ANSI hue ramps interpolated in OKLab/OKLCH space. Split-surface themes additionally emit a `[data-theme="..."] .editor-surface` block so the editor can use a light-paper ramp while the surrounding chrome stays dark. The frontend surfaces that catalog in two places: a grouped visual theme gallery in Settings → Appearance, and a command-palette theme picker that previews themes while the user arrows through results.

### 19.2 Sidebar views

Sidebar views are user-defined in `.notesmith/sidebar.yaml`. By default (no YAML file), the sidebar shows only the Files tab — a standard file/folder tree with no tab bar.

When ≥1 custom view is configured, a tab bar appears at the top of the sidebar. Files is always present and always first. Tabs use a **fixed 2-column grid** (icon + name), wrapping to additional rows as needed. Views support an optional `badge_query` for tab-level badge counts.

FileTree supports Obsidian-style folder notes through the same-name markdown convention: `Customers/Acme/Acme.md` represents `Customers/Acme/`. The folder name opens the folder note, the disclosure chevron expands/collapses children, and the duplicate child note is hidden only in that tree position. Folder context menus support opening/creating folder notes and renaming folders; Notesmith-initiated folder renames sync the same-name folder-note filename when present and block collisions instead of rewriting links.

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

Clicking a note in the middle pane opens it in the reading pane (respecting the user's current view mode). For query-backed items, the reading pane scrolls to the relevant line (requires `path` and `line` columns in the query).

#### Config schema

```yaml
views:
  - id: workflow
    name: "Workflow"
    icon: "⚡"
    badge_query: "SELECT count(*) FROM v_notes WHERE path LIKE 'Capture/%' AND archived = 0"
    sections:
      - type: recently-viewed
        label: "Recent"
        mode: both          # viewed | edited | both
        limit: 10

      - type: custom-folders
        label: "Projects"
        folders:
          - "Projects/Active"
          - "Customers"

      - type: custom-items
        label: "Triage"
        items:
          - name: "Capture"
            icon: "⚡"
            source:
              folder: "Capture"
              recursive: true
          - name: "Tasks"
            icon: "✅"
            source:
              query: "SELECT text as title, status, path, ordinal as line FROM v_tasks WHERE status IN ('todo','in_progress')"
              title_column: "title"
              subtitle_column: "status"
              badge_columns: ["status"]
```

#### Backend API

Two endpoints support the sidebar:

- `GET /api/v/{vault}/sidebar-config` — reads and parses `.notesmith/sidebar.yaml`, returns typed JSON. Returns empty config when file is absent.
- `GET /api/v/{vault}/folder-notes?path=...&recursive=true&limit=50` — returns notes in a folder with title and body snippet for middle pane rendering.

### 19.3 Editor experience

The editor is a **v1-critical feature**. It must support full OFM editing with live preview, not just raw markdown text.

Implementation stance:

- CodeMirror 6 is the source editor.
- Live preview is rendered through decorations and inline widgets.
- There is no separate split-preview mode in v1.
- Read mode uses the same tab shell, not a separate pane system.

### 19.4 Default hotkeys

| Action | Key | Equivalent command |
|---|---|---|
| Open today's daily note | `⌘D` | `notesmith daily open` |
| Archive current note | `⌘⇧A` | `notesmith route apply <current>` |
| Quick Capture | `⌘⇧N` | `notesmith capture` |
| Toggle current task status | `⌘⏎` | `notesmith task toggle ...` |
| Open Capture Triage | `⌘⇧I` | open `Dashboards/Capture Triage.md` |
| Quick switcher | `⌘O` | note switcher |
| Command palette | `⌘K` and `⌘P` | palette |
| Global search | `⌘⇧F` | search UI |

### 19.5 Passive notification stance

Notesmith has **no push notifications**. The home dashboard, capture view, task widgets, and daily note are the attention surfaces.

## 20. Dashboards

### 20.1 Product stance

Dashboards exist in two forms:

1. **Native Svelte components** — primary
2. **Markdown dashboard files with `notesmith sql` blocks** — secondary and compatibility-friendly

Native dashboards are the default shipped experience. Markdown dashboards remain important because they are editable, versionable, and easy for agents to inspect.

### 20.2 Example: `Dashboards/Home.md`

````markdown
# Home

## Top-of-mind tasks
```notesmith sql
SELECT customer, stream, text, due, priority, note_path
FROM v_tasks
WHERE status IN ('todo', 'in_progress')
  AND (
    (due IS NOT NULL AND date(due) <= date('now', '+7 day'))
    OR priority <= 1
  )
ORDER BY COALESCE(due, '9999-12-31'), priority, text
LIMIT 15;
```

## Active customers
```notesmith sql
SELECT title, state, updated_at
FROM v_customers
WHERE state = 'Active'
ORDER BY title;
```

## Streams in progress
```notesmith sql
SELECT title, customer, status, updated_at
FROM v_streams
WHERE status = 'In Progress'
ORDER BY updated_at DESC;
```
````

### 20.3 Example: `Dashboards/Capture Triage.md`

````markdown
# Capture triage

## Captured notes (oldest first)
```notesmith sql
SELECT path, type, customer, created_at
FROM v_notes
WHERE path LIKE 'Capture/%'
  AND archived = 0
ORDER BY created_at ASC;
```

## Capture tasks not yet routed
```notesmith sql
SELECT note_path, status, text
FROM v_tasks
WHERE note_path LIKE 'Capture/%'
  AND status IN ('todo', 'in_progress', 'blocked', 'awaiting_customer', 'on_hold')
ORDER BY note_path, ordinal;
```
````

### 20.4 Example: `Dashboards/Customers.md`

````markdown
# Customers

## All customers by state
```notesmith sql
SELECT state, GROUP_CONCAT(title, ', ') AS customers
FROM v_customers
GROUP BY state
ORDER BY state;
```

## Customers needing attention
```notesmith sql
SELECT title, state, updated_at
FROM v_customers
WHERE state = 'Active'
  AND datetime(updated_at) < datetime('now', '-30 day')
ORDER BY updated_at ASC;
```
````

## 21. Copy as HTML

### 21.1 Feature

Notesmith ships built-in **Copy as HTML** support in both the CLI and editor UI.

### 21.2 CLI contract

```bash
notesmith copy-html "Customers/Acme Corp/Account Info/Account Info.md"
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
| Per-vault | `.notesmith/` | vault-specific behavior, prompts, views, rules |

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

### 22.3 Per-vault config example

```toml
name = "work"
homepage = "Dashboards/Home.md"

[capture]
folder = ""
template = "generic-note"

[daily]
folder = ""
template = "daily-note"
generate_at = "06:30"
catch_up = true

[editor]
live_preview = true
default_mode = "source"
show_line_numbers = true
hide_duplicate_h1 = true
paste_url_image_whitelist = ""
```

### 22.4 Per-vault hidden directory

```text
.notesmith/
├── vault.toml
├── routing.yaml
├── sidebar-views.yaml
├── url-actions.yaml
├── prompts/
│   └── daily-note.md
└── skill.md
```

Templates and scripts remain visible vault assets under `Assets/templates/` and `Assets/scripts/`.

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
- Tauri can switch vaults without restarting the daemon.

## 24. Obsidian Compatibility

### 24.1 Compatibility stance

Notesmith is **read-only compatible** with Obsidian.

That means:

- the vault can still be opened in Obsidian,
- Notesmith does not write into `.obsidian/`,
- `notesmith sql` blocks remain inert code fences instead of corrupting notes,
- task emoji syntax stays on disk,
- attachments stay normal files.

### 24.2 Migration stance

Migration is **in-place evolution first**.

v1 workflow:

1. point Notesmith at an existing vault,
2. keep existing notes where practical,
3. gradually replace legacy Dataview/Templater workflows with Notesmith-native templates, SQL blocks, and routing,
4. add a migration CLI later if it proves worth building.

## 25. File Watching & Conflict Handling

### 25.1 File watching

`notify` watches all configured vault roots. The daemon debounces events, reparses only affected notes, and pushes updates over SSE.

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

## 27. Testing Strategy

### 27.1 Test mix

Notesmith uses:

- **unit tests** for parser, router, tasks, config, and query helpers,
- **integration tests** for CLI and HTTP contracts,
- **snapshot tests** using the `insta` crate.

There is **no Playwright and no browser E2E suite** in v1.

### 27.2 Golden vault fixture

A checked-in `golden-vault/` fixture represents the canonical note structure and workflows. It contains:

- customer notes,
- streams,
- meetings,
- daily notes,
- tasks with all seven statuses,
- dashboard notes,
- routing edge cases.

### 27.3 Snapshot targets

Snapshot tests cover:

- rendered SQL results,
- template instantiation,
- routing decisions,
- parsed task inventories,
- copy-as-HTML output,
- sidebar view resolution.

### 27.4 Integration targets

Integration tests run the real `notesmith` binary against the golden vault and hit the real HTTP endpoints. The goal is contract confidence, not UI click simulation.

## 28. Implementation Phases

| Phase | Scope | Exit criterion |
|---|---|---|
| Phase 0 | TurboVault evaluation spike (half day) | Decide keep vs swap behind `VaultEngine` |
| Phase 1 | Daemon + CLI foundation (1–2 weeks) | `notesmith daemon start`, vault detection, note read/write, HTTP skeleton |
| Phase 2 | Read-only core | Parser, watcher, SQLite cache, Tantivy, stable `v_notes` / `v_backlinks` |
| Phase 3 | Tasks + SQL + capture | `v_tasks`, SQL execution, capture workflow, task toggling |
| Phase 4 | Templates + routing + fallback daily | minijinja, route rules, `daily ensure`, hook runner |
| Phase 5 | Agent-first workflows | `.notesmith/skill.md`, `daily agent-create`, MCP fallback |
| Phase 6 | GUI shell | SvelteKit app, Tauri wrapper, tabs, command palette, sidebar views |
| Phase 7 | Editor polish | CodeMirror live preview, SQL block rendering, right rail, copy-as-HTML, URL scheme |
| Phase 8 | Git + packaging + docs | git timers, installer polish, operational docs, v1 hardening |

Phase 0 explicitly has **no GUI work**.

## 29. Open Questions & Deferrals

| Topic | Decision |
|---|---|
| Web-hosted deployment | Supported architecturally by HTTP-first design; Docker/proxy packaging is deferred past v1 |
| Mobile | Deferred |
| Notifications | Deliberately excluded; passive query surfaces only |
| Split panes | Deferred; tabs only in v1 |
| Migration CLI | Deferred until in-place adoption proves painful |
| Multi-user authz | Not part of the core plan |

This document is the definitive build blueprint for Notesmith.
