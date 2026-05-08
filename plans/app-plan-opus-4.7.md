# App Plan — Custom Notes Application (Opus 4.7)

A plan for building **Notesmith** (working name): a custom, file-based, Obsidian-flavored markdown
notes application that replaces Obsidian for the workflow specified in `notes-method.md` and the
reviewed plan, with **agentic automation as a first-class concern**.

This document is opinionated. Every "TBD" is a decision deferred deliberately.

---

## 0. Design principles

1. **Files are the database.** The on-disk vault is the single source of truth. Any index,
   cache, or graph is *derivable* and disposable. The app must survive being deleted and
   rebuilt from the vault alone.
2. **Plain markdown round-trips losslessly.** Anything the app writes must be readable in any
   text editor and re-openable in Obsidian without corruption. We *extend* OFM (Obsidian
   Flavored Markdown) but never break it.
3. **Agents are users.** Every UI action is a CLI/IPC command first. The GUI is a thin client
   over the same command surface that agents drive.
4. **No plugin system.** Every plugin in the reviewed plan (Templater, Tasks, Dataview,
   QuickAdd, Auto Note Mover, Periodic Notes, Linter, Homepage, Bookmarks, Hotkeys-for-files)
   is built in. Less surface area, fewer bugs, one mental model.
5. **Watch the disk, not the app.** External edits (agents, scripts, sync) must be reflected
   instantly. The file watcher is the primary input, not user input from the GUI.
6. **Deterministic where possible.** Same vault state → same indexes → same query results.
   Templates are pure functions of inputs. Routing is a pure function of frontmatter.

---

## 1. Technology stack

### 1.1 Recommended stack: **Rust core + Tauri v2 GUI**

| Layer | Choice | Rationale |
|---|---|---|
| **Core engine** | **Rust** | The vault parser/indexer/query engine must be fast and embeddable. TurboVault already exists in Rust and gives us 60% of what we need on day 1. Single binary distribution. |
| **Desktop shell** | **Tauri v2** | <10MB installer vs Electron's 100MB+. WebView UI keeps editor work cheap. Rust IPC means the GUI calls the same core API as the CLI. macOS-native menus, URL scheme handling, launchd/notifications already wired. |
| **GUI framework** | **SvelteKit** (in Tauri webview) | Smallest runtime, best ergonomics for a doc-centric app, excellent CodeMirror 6 integration story. (React is the safe fallback if Svelte expertise is missing.) |
| **Editor widget** | **CodeMirror 6** with custom Markdown extension | Tree-sitter-grade incremental parsing, decoration API for live-preview, source-of-truth-is-text (round-trips trivially). Milkdown is rejected: WYSIWYG ProseMirror trees fight wikilinks/inline-fields and complicate plain-text round-trip. |
| **Markdown parser (core)** | **`comrak`** (CommonMark + GFM) wrapped with custom OFM extensions | Comrak handles GFM tables, task lists, footnotes; we layer wikilinks, callouts, embeds, inline fields, dataview blocks on top. |
| **Frontmatter** | **`gray_matter`** crate (or `serde_yaml` directly) | Standard YAML frontmatter; `serde` for typed access. |
| **Search index** | **Tantivy** | BM25 full-text, faceted search, mmap-based, embedded. |
| **Link/graph index** | **`petgraph`** for in-memory + SQLite mirror for queries | Backlinks, orphan detection, graph queries. SQLite is rebuildable cache only. |
| **Query store (cache)** | **SQLite** (via `rusqlite`) | Materialized view of frontmatter + inline fields + tasks. Powers Dataview-equivalent queries with real SQL. Rebuilt from vault on demand. |
| **File watcher** | **`notify`** (cross-platform inotify/FSEvents/ReadDirectoryChanges) | Standard. |
| **Templating** | **`minijinja`** | Jinja2-compatible, sandboxed by default, fast, no JS eval. Replaces Templater's JS-execution-in-templates with safer expression language + explicit hook scripts. |
| **Scripting (user)** | **External processes**, not embedded | User scripts run as subprocesses with structured stdin/stdout. We do *not* embed Deno/QuickJS — keeps the binary small and the security model honest. |
| **Daemon IPC** | **Unix socket** with **JSON-RPC 2.0** | Same protocol GUI, CLI, ACP server, and URL-scheme handler all speak. |
| **Logging** | **`tracing`** + `tracing-subscriber` | Structured logs to file; ring buffer in memory. |
| **Build/CI** | `cargo` + `pnpm` (Tauri standard); GitHub Actions cross-compile for macOS arm64/x64. | macOS first, Linux second, Windows later. |

### 1.2 Library research summary

Researched and chosen:

- **TurboVault (Epistates/turbovault)** — Rust SDK + MCP server for OFM vaults. Has parser,
  graph, vault I/O, atomic writes, multi-vault support, and FTS. **Use as a foundation:**
  fork or depend on `turbovault-core`, `turbovault-parser`, `turbovault-vault`,
  `turbovault-graph`. Skip its MCP server — we'll write our own JSON-RPC daemon.
- **comrak** — CommonMark + GFM parser in Rust. Active, fast.
- **Tantivy** — embedded search; used by Quickwit. BM25, mmap.
- **CodeMirror 6** — modular editor, well-documented Markdown extension; `@codemirror/lang-markdown`
  is extensible enough for OFM additions via custom node types.
- **minijinja** — sandboxed Jinja2 templating in Rust; trivial to expose custom functions.
- **notify** — battle-tested file watcher.
- **agent-client-protocol** crate (Zed) — reference implementation of ACP we can use as a
  client/server library.

Researched and rejected:

- **Electron + LangChain.js** — too heavy; doesn't match "single binary CLI + GUI" goal.
- **Milkdown / TipTap** — WYSIWYG ProseMirror schemas don't round-trip OFM cleanly.
- **Embedded JS runtime (QuickJS/Deno core) for templates** — security and binary-size cost
  outweigh the convenience; Templater's main use case is small expressions, which Jinja covers.

---

## 2. Architecture overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            VAULT (filesystem)                           │
│        Plain markdown — single source of truth, no app-specific files   │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │ notify (FS events)
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          notesmith-core (Rust lib)                      │
│                                                                         │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐   │
│  │  Parser  │ │  Vault   │ │  Index   │ │ Template │ │   Routing    │   │
│  │  (OFM)   │ │   I/O    │ │ (SQLite+ │ │ (Jinja)  │ │  (rule eval) │   │
│  │          │ │ (atomic) │ │ Tantivy) │ │          │ │              │   │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────────┘   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐   │
│  │  Tasks   │ │  Query   │ │  Graph   │ │  Hooks   │ │    Audit     │   │
│  │  engine  │ │  engine  │ │  (links) │ │ (events) │ │  (op log)    │   │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────────┘   │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │ in-process API
       ┌────────────────────────────┼─────────────────────────────┐
       ▼                            ▼                             ▼
┌─────────────┐            ┌─────────────────┐            ┌──────────────┐
│ notesmithd  │            │  notesmith CLI  │            │  Tauri GUI   │
│  (daemon)   │            │   (one-shot)    │            │              │
│ JSON-RPC on │            │                 │            │  CodeMirror6 │
│ unix sock   │            │                 │            │  + Svelte    │
└──────┬──────┘            └─────────────────┘            └──────┬───────┘
       │                                                         │
       ├──── ACP server (stdio JSON-RPC) ◀── agent ───┐          │
       ├──── MCP server (optional) ◀── Claude/etc ────┤          │
       ├──── URL scheme handler ◀── notesapp:// ──────┤          │
       └─────────────────────────────────────────────────────────┘
                                                                 │
                                              same JSON-RPC ─────┘
```

### 2.1 Crate layout

```
notesmith/
├── crates/
│   ├── notesmith-core/        # vault, parser, indexer, queries — pure lib, no I/O surprises
│   ├── notesmith-templates/   # Jinja env + builtins (today, prompt, file ops)
│   ├── notesmith-rules/       # routing rules + hooks DSL
│   ├── notesmith-rpc/         # JSON-RPC schema + server/client
│   ├── notesmith-acp/         # ACP server adapter
│   ├── notesmith-mcp/         # MCP server adapter (optional)
│   ├── notesmith-cli/         # `notesmith` binary
│   ├── notesmithd/            # background daemon binary
│   └── notesmith-tauri/       # Tauri shell + IPC bridge
├── ui/                        # SvelteKit app
└── vault-schema/              # shared TS/Rust types for frontmatter (codegen)
```

### 2.2 Core data model

Everything in core is built around a single typed `Note`:

```rust
pub struct Note {
    pub path: VaultPath,            // relative to vault root
    pub frontmatter: Frontmatter,   // typed YAML (serde)
    pub body: String,               // raw markdown after frontmatter
    pub ast: Option<Ast>,           // lazy, on-demand
    pub blocks: Vec<Block>,         // tasks, headings, callouts, dataview blocks, embeds
    pub links: Vec<Link>,           // wikilinks + md links resolved against vault
    pub inline_fields: Vec<InlineField>,  // [key:: value]
    pub mtime: SystemTime,
    pub hash: blake3::Hash,         // content hash for change detection
}
```

The `Frontmatter` type is **closed-schema for known kinds, open for unknown**:

```rust
pub enum Frontmatter {
    Daily(DailyMeta),
    Meeting(MeetingMeta),
    Stream(StreamMeta),
    Customer(CustomerMeta),       // state lives here
    AccountInfo(AccountInfoMeta),
    Note(NoteMeta),
    Other(serde_yaml::Mapping),
}
```

Unknown `type:` values are preserved verbatim — the app never deletes user metadata.

---

## 3. Feature mapping (Obsidian plugin → built-in)

| Obsidian feature / plugin | Built-in equivalent | Notes |
|---|---|---|
| **Templater** | `notesmith-templates` (Jinja2 via minijinja) | Folder→template mapping in `notesmithrc.toml`. JS execution replaced by (a) Jinja expressions for inline values, (b) hook scripts (subprocesses) for anything complex. Prompts handled by template `{% prompt "Topic" %}` → CLI/GUI both supply answers. |
| **Tasks plugin** | `notesmith-core::tasks` | Tracks 7 statuses (` /bwhx-`). Parses Tasks-plugin emoji syntax (📅 ⏳ 🛫 🔼 ✅ etc.) AND inline fields. Same query DSL exposed as CLI/RPC. |
| **Dataview** | `notesmith-core::query` | SQL over the SQLite cache (frontmatter + inline fields + tasks materialized). Live-rendered code blocks with language `notesmith` (alias `dataview` for compatibility). |
| **QuickAdd** | `notesmith create` CLI / GUI palette / URL scheme | Templates declare prompt schemas; any frontend can fulfill them. |
| **Auto Note Mover** | `notesmith-rules` routing engine | Declarative YAML rules in `Assets/rules/routing.yaml`. Trigger: file save in Inbox, or explicit `notesmith archive`. |
| **Periodic Notes + Calendar** | Daily note scheduler in `notesmithd` | launchd/cron not needed — daemon schedules generation at 06:30 from config. Calendar widget in GUI sidebar. |
| **Homepage** | `homepage` config key | GUI opens it on startup; URL scheme `notesapp://home` honors it. |
| **Linter** | Save-time hook in core | Standard hooks: stamp `created`/`updated`, sort frontmatter keys, trim trailing whitespace, normalize task syntax. Pure Rust — no shell-out. |
| **Hotkeys for specific files** | GUI: `Hotkeys` config maps key → vault path. CLI: aliases. | |
| **Bookmarks** | `Bookmarks/bookmarks.yaml` in vault Assets | Plain YAML; sidebar reads it. |
| **Bases / table view** | Built-in table view over query results | Same SQL; renderer differs. |
| **Graph view** | `notesmith-core::graph` exposes adjacency; GUI renders with d3 or sigma.js | |

---

## 4. Markdown extensions (OFM superset)

Compatible with Obsidian; nothing here breaks if the file is opened in Obsidian.

| Syntax | Meaning | Notes |
|---|---|---|
| `[[Wiki Link]]`, `[[Wiki Link\|alias]]`, `[[Note#Heading]]`, `[[Note#^block-id]]` | Wikilinks | Resolved by best-match (shortest unique path; folder-scoped fallback). |
| `![[Embed]]` | Embed | Renders inline in preview. |
| `> [!note] Title` callouts | Obsidian callouts | Full set of types preserved; rendered in preview. |
| `- [ ]` … `- [-]` task statuses | 7 statuses ` /bwhx-` | Parsed by tasks engine. |
| `[key:: value]` inline fields | Dataview-style inline fields | Indexed in SQLite cache. |
| ` ```notesmith ` (alias ` ```dataview `) code blocks | Live query | DSL described in §9. |
| `%% comment %%` | Obsidian comments | Stripped in preview, preserved in source. |
| `^block-id` | Block references | Indexed for `[[Note#^id]]` resolution. |
| `==highlight==` | Highlight | Standard OFM. |

---

## 5. Agentic automation design

The non-negotiable rule: **anything a human can do in the GUI, an agent can do via JSON-RPC**.
The GUI itself is built on top of that JSON-RPC surface, so feature parity is enforced by
construction.

### 5.1 Agent integration surfaces

1. **Native CLI** (`notesmith`) — for shell scripts, launchd, ad-hoc agents, CI.
2. **JSON-RPC over Unix socket** (`notesmithd`) — for long-running agents that need
   subscriptions (file changes, task updates, prompt fulfillment).
3. **ACP server** (stdio JSON-RPC) — for editor-embedded agents (Zed, Neovim, JetBrains).
   The "editor" being controlled is `notesmithd`. Sessions map to vault contexts.
4. **MCP server** (optional, behind a flag) — for Claude Desktop / Cursor / other MCP
   clients. Wraps the same JSON-RPC surface in MCP tool definitions.
5. **URL scheme** (`notesapp://`) — for Raycast/Alfred/Nimble/launchers and
   inter-app deep-linking.
6. **HTTP API** (opt-in, localhost-bound by default) — when an agent can't speak Unix sockets
   (e.g., browser extensions). Same JSON-RPC, just over HTTP.

### 5.2 Capabilities surfaced to agents

Every operation is a JSON-RPC method. Methods are grouped:

- `vault.*` — list, read, write, move, delete, stat, watch
- `note.*` — create, get, patch (diff-based), append, set_frontmatter, archive
- `task.*` — list, complete, transition, create, attach
- `query.*` — sql, dql (dataview-style), graph
- `template.*` — list, render, instantiate (with prompt fulfillment over RPC)
- `route.*` — preview (dry run), apply
- `link.*` — backlinks, outlinks, orphans, broken
- `customer.*` — list (state filter), set_state, summary
- `stream.*` — list (status filter), set_status, tasks
- `daily.*` — today, ensure, archive
- `system.*` — reindex, watch, subscribe, capabilities, version

### 5.3 Subscriptions (the agent-friendly killer feature)

Agents can subscribe to streams of events:

```json
{"method": "system.subscribe", "params": {"events": ["note.changed", "task.completed", "inbox.new"]}}
```

Server pushes notifications. Use cases:
- An agent watches `inbox.new` and triages on the user's behalf.
- An agent listens for `task.completed` and updates an external tracker.
- An agent watches `customer.state_changed` to sync to CRM.

### 5.4 Idempotency and safety

- All mutating operations accept `idempotency_key`.
- All write operations log to `vault/.notesmith/audit.log` (JSON lines: ts, op, args, hash before/after, agent_id).
- Agents can request **dry-run mode**: `route.preview`, `note.patch?dry_run=true`.
- Atomic writes: write to temp file in same dir, fsync, rename. Never partial files.
- Optional **patch review**: when `confirm_destructive: true` is in the agent's session, the
  daemon emits an approval request to any subscribed UI before executing.

### 5.5 Permissions / capabilities

Agents authenticate to `notesmithd` with a token tied to a **capability set**:

```yaml
# ~/.config/notesmith/agents/triage-bot.yaml
agent_id: triage-bot
token: ${env:TRIAGE_BOT_TOKEN}
capabilities:
  - vault.read
  - note.read
  - note.create:in=Inbox/**
  - note.move:from=Inbox/**,to=Customers/**
  - task.transition
  deny:
  - note.delete
  - vault.write:path=Archive/**
```

CLI uses an implicit "owner" token with all capabilities.

---

## 6. URL scheme design

Scheme: `notesapp://` (registered with macOS `LSHandlers` via Tauri).

General form: `notesapp://<verb>[/<path>][?<params>]`. All paths and param values are URL-encoded.

| URL | Behavior |
|---|---|
| `notesapp://open/Customers/Acme%20Corp/Acme%20Corp.md` | Open note in GUI; focus cursor at top. |
| `notesapp://open/Customers/Acme%20Corp/Acme%20Corp.md#Active%20Streams` | Open at heading. |
| `notesapp://open?id=20260508143000` | Open by note id (frontmatter `id` field). |
| `notesapp://search?q=Acme%20pricing` | Open search palette pre-filled. |
| `notesapp://create?template=meeting&customer=Acme%20Corp&kind=external&topic=Pricing` | Run template with prompt values pre-filled; open the new note. |
| `notesapp://create?template=stream&customer=Acme%20Corp&name=SSO%20rollout` | |
| `notesapp://daily` | Open today's daily note (create if missing). |
| `notesapp://daily?date=2026-05-08` | Open that day's note (create if missing). |
| `notesapp://archive?path=Inbox/foo.md` | Run archive routing on a single note. |
| `notesapp://archive?scope=inbox` | Archive all routable inbox notes. |
| `notesapp://task/new?text=Send%20SOW&customer=Acme%20Corp&due=2026-05-15` | Append a task to today's daily. |
| `notesapp://task/complete?id=<task-id>` | Mark task done. |
| `notesapp://customer/Acme%20Corp` | Open customer index. |
| `notesapp://customer/Acme%20Corp/dashboard` | Open with dashboard view active. |
| `notesapp://stream/Acme%20Corp/Migration%20to%20v2` | Open stream note. |
| `notesapp://home` | Open configured homepage. |
| `notesapp://command/<cmd-id>?<args>` | Run any GUI command by id (parity escape hatch). |
| `notesapp://rpc?method=query.sql&params=...` | Generic RPC tunnel (signed; off by default). |

URL handling lives in `notesmith-cli`'s URL parser → translates to JSON-RPC → executes against
`notesmithd`. Same code path whether the URL came from Raycast or from `xdg-open`.

---

## 7. CLI design

Binary: `notesmith` (single static binary, ~12MB stripped).

### 7.1 Conventions

- All commands accept `--vault <path>` (defaults to `$NOTESMITH_VAULT` or
  `~/.config/notesmith/vault`).
- All commands accept `--json` for machine-readable output (default for non-tty).
- All commands accept `--dry-run` where mutation applies.
- Stdin/stdout are first-class: `notesmith note read X | jq` works.

### 7.2 Command tree

```
notesmith
├── vault
│   ├── init <path>
│   ├── status
│   ├── reindex
│   └── watch                       # tail file events as JSON lines
├── note
│   ├── create --template <t> [--var key=val]...
│   ├── read <path>                  # stdout: full file
│   ├── show <path>                  # parsed view: frontmatter + summary
│   ├── write <path> [--from-stdin | --body <text>] [--frontmatter <yaml>]
│   ├── patch <path> --diff <unified-diff>
│   ├── append <path> [--body <text> | --from-stdin]
│   ├── set <path> <key> <value>     # set a frontmatter key
│   ├── move <src> <dst>
│   ├── archive <path> [--rule <rule-id>]
│   └── delete <path> [--force]
├── task
│   ├── list [--status TODO,IP] [--customer X] [--due-before 2026-06-01] [--owner me]
│   ├── new "Task text" [--customer X] [--stream Y] [--due ...] [--in <note-path>]
│   ├── complete <task-ref>
│   └── transition <task-ref> <status>      # status: todo|ip|blocked|awaiting|hold|done|cancelled
├── query
│   ├── sql "SELECT path, frontmatter->>'state' FROM notes WHERE type='customer'"
│   ├── dql "FROM #customer WHERE state = 'Active'"
│   └── tasks <preset>               # active|blocked|awaiting|hold|by-customer
├── template
│   ├── list
│   ├── render <name> [--var k=v]... # render to stdout
│   └── apply  <name> --to <path>
├── route
│   ├── preview <path>               # show where it would go
│   └── apply [<path> | --inbox]
├── customer
│   ├── list [--state Active]
│   ├── show <name>
│   ├── set-state <name> <state>
│   └── new <name>
├── stream
│   ├── list [--customer X] [--status In Progress]
│   ├── show <customer> <name>
│   ├── set-status <customer> <name> <status>
│   └── new <customer> <name>
├── daily
│   ├── today                        # ensure + open
│   ├── ensure [--date 2026-05-08]
│   └── archive [--before 2026-05-01]
├── link
│   ├── backlinks <path>
│   ├── orphans
│   └── broken
├── search "query string" [--type meeting] [--limit 20]
├── url <notesapp://...>             # resolve URL → run command
├── daemon
│   ├── start | stop | status | restart
│   └── logs [--follow]
├── agent                            # agent management
│   ├── token issue --name X --capabilities ...
│   ├── token revoke <name>
│   └── list
├── acp serve                        # speak ACP on stdio (for editor embedding)
└── mcp serve                        # speak MCP on stdio (for Claude Desktop etc.)
```

### 7.3 Examples

```bash
# Create a meeting note from template, fully scripted
notesmith note create --template meeting \
  --var customer="Acme Corp" --var kind=external --var topic="Pricing review"

# Pipe a transcribed call into the body
pbpaste | notesmith note create --template meeting --var topic="Standup" --from-stdin

# Find every active task awaiting a customer, due this week
notesmith task list --status awaiting --due-before "$(date -v+7d +%F)" --json

# Drop everything in inbox to its rightful home
notesmith route apply --inbox

# Long-running agent watching for triage
notesmith vault watch | jq -c 'select(.path | startswith("Inbox/"))'
```

---

## 8. ACP / agent protocol design

### 8.1 Why ACP

ACP (the Zed-driven Agent Client Protocol) standardizes how *editors* talk to *agents*.
We invert it: **`notesmithd` exposes itself as an ACP-compatible "editor"** so that any
ACP-aware agent (Claude, Codex, Gemini CLI, whatever Zed users plug in) can drive the vault
without bespoke integration.

Practically: `notesmith acp serve` speaks JSON-RPC 2.0 over stdio per ACP spec.

### 8.2 Mapping our domain to ACP

| ACP concept | Notesmith mapping |
|---|---|
| Session | A vault context (vault root + optional active note + permission token). |
| ContentBlock | Markdown chunks (full notes, snippets, query results rendered as markdown). |
| `fs/read_text_file` | `vault.read` |
| `fs/write_text_file` | `note.write` (atomic, with audit log entry) |
| `terminal/*` (shell) | **Disabled by default** — replace with `notesmith.run_command`, an allow-listed RPC call. |
| `session/cancel` | Cancel any in-flight long-running query/reindex. |
| `session/update` (streaming) | Push `note.changed`, `task.completed`, `inbox.new` events to the agent. |

Agents see a clean filesystem-scoped surface but with our semantic operations layered on
top via ACP's extension mechanism (`session/extension/*`).

### 8.3 MCP as a complementary surface

MCP is better for "tool-using chat assistants" (Claude Desktop). The MCP server is a thin
wrapper that exposes our JSON-RPC methods as tool definitions. We don't want to maintain MCP
*and* a custom protocol — both wrap the same RPC schema, generated from one OpenRPC document
(`vault-schema/rpc.openrpc.json`). Build once, generate adapters.

### 8.4 Native JSON-RPC (preferred)

For agents we control, the simplest path is **direct JSON-RPC over Unix socket**. Discovery:

```
$XDG_RUNTIME_DIR/notesmith.sock         # Linux
~/Library/Application Support/notesmith/notesmith.sock   # macOS
```

Handshake:

```json
{"method": "system.hello", "params": {"client": "triage-bot/1.0", "token": "..."}}
→ {"capabilities": [...], "vault": "/path", "version": "..."}
```

---

## 9. Template engine design

Replaces Templater. Goals: deterministic, sandboxed, human-readable, agent-driveable.

### 9.1 Template files

Live in `Assets/templates/*.md.j2`. They are markdown with Jinja2 expressions. Frontmatter
of the *template* declares prompts and routing:

```yaml
---
notesmith:
  name: meeting
  description: Customer meeting (internal or external)
  output_path: "Inbox/{{ today }} - {{ customer }} - {{ kind | title }} - {{ topic }}.md"
  prompts:
    - { name: customer, type: customer-picker, required: true }
    - { name: kind, type: choice, choices: [internal, external], required: true }
    - { name: topic, type: text, required: true }
---
---
type: meeting
meeting-kind: {{ kind }}
customer: "[[{{ customer }}]]"
date: {{ today }}
created: {{ now }}
tags: [meeting, {{ kind }}]
---

# {{ today }} — {{ customer }} — {{ kind | title }} — {{ topic }}

## Attendees
- 

## Notes
- 

## Action items
- [ ] 
```

### 9.2 Built-in template functions

Jinja `globals`:

- `today`, `now`, `tomorrow`, `yesterday` — formatted dates/times
- `customer(name)` — looks up customer; returns frontmatter as object
- `query(dql)` — runs a query at template render time
- `prompt(name, default=None)` — reference a prompt value (also injected as plain variable)
- `slug(s)`, `title_case(s)` — string helpers
- `next_id()` — unique id (timestamp-based, monotonic)
- `pick_one(list)` — for randomized templates (rarely needed)

Filters: `as_wikilink`, `as_inline_field`, `escape_yaml`, `truncate`, standard Jinja.

### 9.3 Replacing Templater's JS escape hatch

Templater's killer feature is "drop into JS". We replace it with **hooks**:

```yaml
# In the template:
notesmith:
  hooks:
    pre_render:  Assets/scripts/lookup_account.sh   # gets prompt JSON on stdin, prints JSON on stdout
    post_create: Assets/scripts/notify_slack.sh
```

Hooks are subprocesses. They get a structured payload on stdin
(`{prompts, vault_path, target_path, env}`) and may emit additional variables on stdout. This
is a sharper tool than embedded JS: anything a hook can do is also doable from the CLI.

### 9.4 Prompt fulfillment over RPC

When `notesmith template apply meeting` is invoked without all prompts on the command line,
the daemon emits `template.prompt.required` events on subscriptions. The GUI shows a modal;
an agent answers via `template.prompt.fulfill`. CLI in tty mode falls back to `readline`.

---

## 10. Query engine design

Replaces Dataview + Tasks queries with a single, well-typed pipeline.

### 10.1 The cache

`vault/.notesmith/cache.sqlite` is rebuildable from the vault. Schema (essentials):

```sql
CREATE TABLE notes (
  path TEXT PRIMARY KEY,
  type TEXT,
  hash TEXT,
  mtime INTEGER,
  frontmatter JSON,
  body_excerpt TEXT
);
CREATE TABLE links (src TEXT, dst TEXT, kind TEXT);  -- kind: wiki, embed, md
CREATE TABLE tasks (
  id TEXT PRIMARY KEY,
  note_path TEXT,
  status TEXT,           -- todo|ip|blocked|awaiting|hold|done|cancelled
  text TEXT,
  customer TEXT,
  stream TEXT,
  owner TEXT,
  due DATE, scheduled DATE, start DATE,
  priority INTEGER,
  done_at DATE,
  block_id TEXT,
  raw TEXT
);
CREATE TABLE inline_fields (note_path TEXT, key TEXT, value TEXT);
CREATE VIRTUAL TABLE notes_fts USING fts5(path, title, body, frontmatter);
```

### 10.2 Three query languages

We support **three** front-ends that all compile to SQL on this cache:

1. **SQL passthrough** — for power users and agents:
   ```sql
   SELECT path, json_extract(frontmatter,'$.state')
   FROM notes
   WHERE type='customer' AND json_extract(frontmatter,'$.state')='Active';
   ```

2. **NDQL (Notesmith Data Query Language)** — Dataview-flavored, drop-in for most users:
   ```
   TABLE state, file.mtime AS "Updated"
   FROM #customer
   WHERE state = "Active"
   SORT file.name ASC
   ```
   The NDQL parser produces SQL. Dataview-syntax queries are *re-parsed* to NDQL on import for
   compatibility.

3. **Tasks DSL** — Tasks-plugin-compatible filters:
   ```
   not done
   path includes Customers
   due before in 7 days
   group by customer
   ```

### 10.3 Live query blocks

Code blocks rendered at preview/view time:

```
` ``notesmith
TABLE status, file.mtime
FROM #stream
WHERE status = "In Progress" AND customer = [[Acme Corp]]
` ``
```

(Alias `dataview` accepted — guarantees Obsidian compatibility.)

In the editor, the rendered table is a *decoration* over the still-text source. Editing the
source updates the render in place. Saved files contain only the original code block — no
hidden rendered output.

### 10.4 Saved queries / dashboards

Dashboards (`Dashboards/Home.md` etc.) are just notes with query blocks. Same as Obsidian. No
new mechanism.

---

## 11. Routing engine design

Replaces Auto Note Mover. Declarative rules; predictable; agent-inspectable.

### 11.1 Rule file: `Assets/rules/routing.yaml`

```yaml
version: 1
rules:
  - id: archive-meeting
    when:
      path: "Inbox/**"
      frontmatter.type: meeting
      frontmatter.archived: true     # only archive notes explicitly marked archived
    then:
      move_to: "Customers/{{ frontmatter.customer | unwikilink }}/{% if frontmatter['meeting-kind'] == 'internal' %}Internal Meetings{% else %}External Meetings{% endif %}/{{ filename }}"

  - id: archive-stream
    when: { path: "Inbox/**", frontmatter.type: stream }
    then: { move_to: "Customers/{{ frontmatter.customer | unwikilink }}/Streams/{{ filename }}" }

  - id: archive-daily
    when: { path: "Inbox/Daily/**", frontmatter.archived: true }
    then: { move_to: "General/Journal/{{ frontmatter.date | strftime:'%Y/%m' }}/{{ filename }}" }

  - id: archive-customer-asset
    when: { path: "Inbox/**", frontmatter.type: { in: [account-info, glossary, milestones] } }
    then: { move_to: "Customers/{{ frontmatter.customer | unwikilink }}/Account Info/{{ filename }}" }
```

### 11.2 Execution

- `notesmith route preview <path>` shows the destination and which rule fired.
- `notesmith route apply <path>` moves the file (atomic), stamps `archived: true` and
  `archived-at: <now>` if not present, updates backlinks if any pointed at the old path
  (configurable: by default we don't rewrite, since wikilinks resolve by name).
- Bulk: `notesmith route apply --inbox`.
- Conflict policy is per-rule (`on_exists: skip|overwrite|rename`).

### 11.3 Hooks

Pre/post hooks may run: `pre_route`, `post_route`. Same subprocess model as templates.

---

## 12. Editor / GUI features

The GUI is intentionally minimal — most "features" come from the core surface and the
template/routing engines. The GUI's job is to render the vault and wire shortcuts.

### 12.1 Panes & navigation

- **File tree** with quick filter, drag-and-drop move, customer state badges.
- **Editor** (CodeMirror 6) with live preview decorations (callouts, embeds, query blocks).
- **Right rail** with: backlinks, outgoing links, inline fields summary, tasks inside the note.
- **Command palette** (⌘P) — every JSON-RPC command is a palette entry.
- **Quick-switcher** (⌘O) — fuzzy on path + frontmatter title.
- **Calendar** widget for daily-note navigation.
- **Bookmarks** sidebar reading `Bookmarks/bookmarks.yaml`.

### 12.2 Hotkeys (defaults; config in `notesmithrc.toml`)

| Action | Key | Equivalent CLI/RPC |
|---|---|---|
| Open today's daily | ⌘D | `daily today` |
| Archive current note | ⌘⇧A | `route apply <current>` |
| New from template | ⌘⇧N | `note create --template <pick>` |
| Toggle task status | ⌘⏎ | `task transition` |
| Open inbox triage dashboard | ⌘⇧I | `note open Dashboards/Inbox\ Triage.md` |
| Quick switcher | ⌘O | n/a |
| Command palette | ⌘P | n/a |
| Open URL scheme handler | (system) | `url <url>` |

### 12.3 Live preview vs source

CodeMirror 6 with decoration widgets. We do *not* implement a separate "preview pane" — the
source is annotated in place (callouts render, query blocks render, wikilinks become
clickable). Toggle with ⌘E for raw source view.

---

## 13. Daily-note scheduler

`notesmithd` includes a simple cron-like scheduler:

```toml
# ~/.config/notesmith/notesmithrc.toml
[daily_note]
template = "daily"
generate_at = "06:30"
timezone = "America/Los_Angeles"
into = "Inbox/Daily/"
filename = "{{ today }}.md"
catch_up = true   # if daemon was down, generate any missed days on next start
```

The scheduler is internal — no launchd plist needed. Daemon startup is handled by Tauri's
auto-start (or `notesmith daemon start` from a login item).

---

## 14. Implementation phases

Eleven phases. Each ends with a usable, dogfoodable build.

### Phase 0 — Foundations (1–2 weeks)
- Repo, CI, cross-compile to macOS arm64/x64.
- `notesmith-core` skeleton: `Note`, `Frontmatter`, vault path types.
- TurboVault dependency wired in for parsing/IO.
- Unit-test corpus: 50 representative notes from the reviewed plan.

### Phase 1 — Read-only core (1 week)
- Parse vault, build SQLite cache, basic `notesmith vault status`, `note read`, `note show`.
- File watcher → incremental cache update.
- `notesmith query sql` working.

### Phase 2 — Tasks & queries (1–2 weeks)
- Tasks engine with all 7 statuses; Tasks-plugin emoji parser.
- NDQL parser; Tasks DSL parser.
- `notesmith task list`, `notesmith query dql`.

### Phase 3 — Templates & note creation (1 week)
- minijinja env, prompt schema, `notesmith template apply`.
- Daily/meeting/stream/customer templates ported from reviewed plan.

### Phase 4 — Routing (1 week)
- Routing rules engine, `route preview/apply`.
- `notesmith route apply --inbox` reaches inbox-zero on the dogfood vault.

### Phase 5 — Daemon & RPC (1–2 weeks)
- `notesmithd` JSON-RPC over Unix socket.
- Subscriptions for file-change events.
- `system.hello`, `system.capabilities`, agent-token CLI.

### Phase 6 — URL scheme & first GUI (2–3 weeks)
- Tauri shell, file tree, CodeMirror 6 editor with OFM extension.
- URL scheme registration (macOS).
- Minimal palette + quick-switcher.

### Phase 7 — Live query/preview decorations (2 weeks)
- Inline rendering of query blocks, callouts, embeds, wikilink resolution.
- Right-rail backlinks pane.

### Phase 8 — ACP server (1 week)
- `notesmith acp serve`; integrate with Zed as a smoke test.

### Phase 9 — MCP + HTTP adapters (1 week)
- `notesmith mcp serve`; localhost HTTP gateway.

### Phase 10 — Daily-note scheduler, bookmarks, hotkeys-for-files, homepage, linter hooks (1–2 weeks)
- Round out parity with the Obsidian plugins we replaced.

### Phase 11 — Migration tooling (1 week)
- `notesmith vault import-obsidian <path>` — copies vault, transforms `.obsidian/` settings
  where meaningful (hotkeys, homepage), discards plugin configs we've replaced.
- Compatibility report: which Dataview/Tasks queries parsed cleanly, which need attention.

**Total estimate:** ~14–18 weeks for a single full-time dev to reach v1.0 dogfood-ready.

---

## 15. Cross-cutting concerns

### 15.1 Sync & concurrency

- Vault is just files; iCloud / Dropbox / Syncthing / git all work.
- File watcher debounces external writes (≥250 ms quiescence) before reindexing.
- Optimistic concurrency on writes: `note.write` accepts `expected_hash`; mismatch returns a
  conflict that the caller must resolve. Agents must handle conflict-or-retry explicitly.
- Optional **git auto-commit** mode (`vault.git.auto_commit = true`): every successful
  routing/template/agent op commits with a structured message. Cheap, human-auditable history.

### 15.2 Performance targets

- Cold reindex of 10k notes: < 5 s.
- Incremental update on single note save: < 50 ms.
- Query cold-cache: < 100 ms for typical dashboards.
- GUI startup to first paint: < 500 ms.

### 15.3 Testing

- `golden-vault/` fixture: a full reviewed-plan vault checked in. Snapshot tests for query
  outputs, routing decisions, template renders.
- Property tests for OFM round-trip (parse → serialize → parse must be a fixed point).
- Integration tests run the actual `notesmith` binary against the golden vault.

### 15.4 Telemetry

None. App is local-only by design. Crash reports are written to `~/Library/Logs/notesmith/`
and surfaced via `notesmith daemon logs`.

### 15.5 Backwards compatibility with Obsidian

- Vault remains a valid Obsidian vault — opening it in Obsidian must continue to work.
- We do not write into `.obsidian/` (we have our own `.notesmith/` directory).
- `dataview` code-block alias is honored so Obsidian users see queries unchanged.
- Tasks-plugin emoji syntax is the canonical task format on disk.

---

## 16. Open questions / explicit deferrals

1. **Mobile.** No plan for v1. Tauri Mobile is not yet stable enough; Obsidian Mobile remains
   a fine companion since the vault stays compatible.
2. **Real-time multi-user.** Out of scope; conflict resolution is left to the file-sync layer.
3. **Encryption at rest.** Out of scope; let the OS / sync provider handle it.
4. **AI features inside the app** (summarize, draft, etc.). Deliberately not built in. Agents
   talk to the app via JSON-RPC; that's the seam.
5. **Plugin system later?** Possibly. The hook-script mechanism + RPC surface covers ~90% of
   what plugins do; a sandboxed plugin system can be added in v2 if needed.

---

## 17. TL;DR

- **Stack:** Rust core (built on TurboVault) + Tauri v2 + SvelteKit + CodeMirror 6.
- **Single source of truth:** the markdown vault. SQLite + Tantivy are caches, rebuildable.
- **One RPC surface** (JSON-RPC 2.0) feeds the CLI, GUI, URL handler, ACP server, and MCP server.
- **Templates** = minijinja + hooks (no embedded JS).
- **Queries** = NDQL/Tasks-DSL/SQL → SQLite materialized view.
- **Routing** = declarative YAML rules.
- **Agents** are first-class peers; every UI action is an RPC method with audit logs and
  capability-scoped tokens.
- **URL scheme** (`notesapp://`) and **CLI** (`notesmith`) are complete; GUI is a thin
  client over the same surface.
- **Phased build** to v1 in ~3–4 months of focused work; each phase is dogfoodable.
