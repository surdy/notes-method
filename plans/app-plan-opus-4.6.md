# Custom Notes App — Architecture & Implementation Plan

> **Goal:** Replace Obsidian with a custom, file-based markdown notes application that is Obsidian-flavored-markdown compatible, has all required plugin functionality built in, and treats agentic automation as a first-class concern.
>
> **Companion doc:** `reviewed-plan.md` defines the vault structure, frontmatter schema, task model, templates, dashboards, and workflows. This plan defines *how to build the app that implements that spec*.

---

## 1. Technology Stack

### 1.1 Recommendation

| Layer | Choice | Why |
|---|---|---|
| **Desktop shell** | **Tauri 2.x** (Rust backend + web frontend) | ~600KB binary vs Electron's 50–150MB. Native filesystem access via `tauri-plugin-fs`. Rust backend handles heavy parsing (markdown-rs), file watching (notify crate), and the MCP/CLI server. Security-audited. Supports macOS, Windows, Linux. |
| **Frontend framework** | **SolidJS** | Faster than React (no virtual DOM diffing), fine-grained reactivity ideal for a live-updating vault index. Smaller bundle. TypeScript-first. Mature enough for production (2+ years stable). |
| **Editor** | **CodeMirror 6** (`@codemirror/view` + `@codemirror/lang-markdown`) | What Obsidian uses. Incremental parsing via Lezer. Extension points for custom syntax (wikilinks, inline fields, custom task statuses). Decoration API for live preview. |
| **Markdown parser (render)** | **unified/remark pipeline** (frontend) + **markdown-rs** (Rust backend) | remark for rendering in the UI; markdown-rs for fast backend parsing during indexing. Both produce mdast ASTs. |
| **Frontmatter** | **gray-matter** (JS) + **serde_yaml** (Rust) | gray-matter for the editor; serde_yaml for the Rust indexer. Both battle-tested. |
| **Query engine** | **DuckDB-WASM** for structured queries + **FlexSearch** for full-text search | DuckDB replaces Dataview's DQL with real SQL over a JSON vault index. FlexSearch is the fastest JS search library for full-text. |
| **Template engine** | **Eta** | 3.5KB, zero deps, TypeScript, async, fastest among alternatives. EJS-like syntax maps cleanly to Templater's `<% %>` patterns. |
| **File watching** | **Tauri's notify-based watcher** (Rust) | Avoids JS↔Rust IPC overhead vs chokidar. Falls back to chokidar for the standalone CLI/MCP server mode. |
| **URL scheme** | **`notesapp://`** via Tauri's deep link plugin (`tauri-plugin-deep-link`) | Registered system-wide on install. x-callback-url support. |
| **Agent protocol** | **MCP server** (primary) + **local REST API** (secondary) | MCP for Claude/Cursor/VS Code Copilot. REST for scripts/automation. |

### 1.2 Key Dependencies

**Rust (Cargo.toml):**

| Crate | Purpose |
|---|---|
| `tauri = "2"` | Desktop shell |
| `tauri-plugin-fs` | File system access |
| `tauri-plugin-shell` | System integration |
| `tauri-plugin-deep-link` | URL scheme handler |
| `markdown` (markdown-rs) | CommonMark + GFM + frontmatter parser (mdast output) |
| `serde`, `serde_yaml`, `serde_json` | Frontmatter/config serialization |
| `notify = "8"` | Cross-platform file watcher |
| `walkdir` | Recursive directory traversal |
| `tokio` | Async runtime |
| `clap = "4"` | CLI argument parsing |
| `axum` | Local HTTP server (REST API + MCP HTTP transport) |
| `tower-http` | CORS, logging middleware |
| `regex` | Inline field / task status parsing |
| `tantivy` | Rust-native full-text search (alternative to JS FlexSearch for backend) |

**JavaScript (package.json):**

| Package | Purpose |
|---|---|
| `solid-js`, `@solidjs/router` | UI framework |
| `@codemirror/view`, `@codemirror/state`, `@codemirror/lang-markdown` | Editor |
| `@lezer/markdown` | CM6 markdown parser extensions |
| `unified`, `remark-parse`, `remark-gfm`, `remark-frontmatter`, `remark-wiki-link`, `remark-directive`, `remark-rehype`, `rehype-stringify` | Markdown rendering pipeline |
| `gray-matter` | Frontmatter parsing |
| `@duckdb/duckdb-wasm` | SQL queries over vault index |
| `flexsearch` | Full-text search |
| `eta` | Template engine |
| `@tauri-apps/api` | Tauri IPC bridge |
| `date-fns` | Date manipulation (template helpers) |

### 1.3 Why Not Electron?

Obsidian uses Electron. We're replacing Obsidian precisely because we want something lighter and more controllable. Tauri gives us:
- 100× smaller binary
- Rust backend for heavy lifting (parsing, indexing, MCP server)
- No bundled Chromium — uses system webview
- Better security model (allowlist-based IPC)
- The Rust ecosystem has `markdown-rs` (same author as remark/micromark), `tantivy` (search), and `notify` (file watching)

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Tauri Process                                │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                       Rust Backend                               │  │
│  │                                                                  │  │
│  │  ┌────────────┐  ┌──────────────┐  ┌──────────────────────────┐ │  │
│  │  │ Vault Core │  │ Query Engine │  │ Agent Integration Layer  │ │  │
│  │  │            │  │              │  │                          │ │  │
│  │  │ • Index    │  │ • DQL→SQL    │  │ • MCP Server (stdio)    │ │  │
│  │  │ • Watcher  │  │ • Task query │  │ • MCP Server (HTTP SSE) │ │  │
│  │  │ • Router   │  │ • Full-text  │  │ • REST API (localhost)  │ │  │
│  │  │ • Parser   │  │ • Tantivy    │  │ • URL Scheme Handler    │ │  │
│  │  │ • Mover    │  │              │  │ • CLI (clap)            │ │  │
│  │  └─────┬──────┘  └──────┬───────┘  └────────────┬─────────────┘ │  │
│  │        │                │                        │               │  │
│  │        └────────────────┴────────────────────────┘               │  │
│  │                         │                                         │  │
│  │              ┌──────────▼───────────┐                            │  │
│  │              │   Vault Operations   │                            │  │
│  │              │   (shared Rust API)  │                            │  │
│  │              └──────────┬───────────┘                            │  │
│  │                         │                                         │  │
│  └─────────────────────────┼─────────────────────────────────────────┘  │
│                            │ Tauri IPC (invoke/events)                  │
│  ┌─────────────────────────▼─────────────────────────────────────────┐  │
│  │                      Web Frontend (SolidJS)                       │  │
│  │                                                                   │  │
│  │  ┌──────────────┐  ┌───────────────┐  ┌───────────────────────┐  │  │
│  │  │ Editor Pane  │  │ File Explorer │  │ Dashboard Renderer    │  │  │
│  │  │ (CodeMirror) │  │ (tree view)   │  │ (DuckDB + remark)    │  │  │
│  │  └──────────────┘  └───────────────┘  └───────────────────────┘  │  │
│  │                                                                   │  │
│  │  ┌──────────────┐  ┌───────────────┐  ┌───────────────────────┐  │  │
│  │  │ Command      │  │ Search        │  │ Template Picker       │  │  │
│  │  │ Palette      │  │ (FlexSearch)  │  │ (Eta engine)          │  │  │
│  │  └──────────────┘  └───────────────┘  └───────────────────────┘  │  │
│  │                                                                   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  Filesystem: ~/NotesVault/ (plain markdown files)                      │
└─────────────────────────────────────────────────────────────────────────┘

External consumers:
  Claude / Cursor / Copilot ──► MCP stdio server (subprocess)
  Shell scripts / cron      ──► CLI (`notesapp vault query ...`)
  Alfred / Raycast          ──► URL scheme (`notesapp://...`)
  Keyboard Maestro          ──► URL scheme or CLI
  Other agents              ──► REST API (localhost:27183)
  launchd / cron            ──► CLI (`notesapp daily create`)
```

### 2.1 Core Modules (Rust)

| Module | Responsibility |
|---|---|
| **`vault_core`** | Vault discovery, configuration, path resolution, vault lock management |
| **`vault_index`** | In-memory index of all notes' metadata (frontmatter, inline fields, links, tasks). Rebuilt on startup, incrementally updated via file watcher. Serialized to `.notesapp/index.json` for fast cold start. |
| **`vault_watcher`** | File system watcher (notify crate). Debounces changes, triggers re-index of affected files, emits events to the frontend. |
| **`parser`** | Markdown parsing using markdown-rs. Extracts frontmatter (serde_yaml), inline fields (`[key:: value]`), task items with custom statuses, wikilinks, tags. |
| **`query_engine`** | Translates DQL-like queries and task queries to SQL, runs against the vault index. Wraps tantivy for full-text search on the backend. |
| **`router`** | Archive/move logic from `reviewed-plan.md` §6.2. Computes destination from frontmatter fields. |
| **`template_engine`** | Bridge to the Eta template engine in JS, or a native Rust template engine (Tera) for CLI-only mode. Handles variable injection (date, customer list, etc.). |
| **`mcp_server`** | MCP protocol implementation (JSON-RPC 2.0 over stdio and HTTP+SSE). Exposes vault operations as MCP tools/resources/prompts. |
| **`rest_api`** | Local HTTP REST API on `localhost:27183`. Axum-based. Token-authenticated. |
| **`cli`** | Clap-based CLI. Can operate headlessly without the GUI running. Communicates with the running app via REST API, or operates directly on files when the app is closed. |
| **`url_handler`** | URL scheme handler. Parses `notesapp://` URLs, dispatches to vault operations. |

### 2.2 Frontend Modules (SolidJS)

| Module | Responsibility |
|---|---|
| **`editor/`** | CodeMirror 6 integration. Custom extensions for wikilinks, inline fields, task statuses, callouts. Live preview decorations. |
| **`explorer/`** | File tree view. Bookmarks panel. Vault navigation. |
| **`dashboard/`** | Renders dashboard notes. Executes embedded queries (dataview blocks → DuckDB SQL, task blocks → task queries). Caches results and live-updates on vault index changes. |
| **`search/`** | Global search powered by FlexSearch. Filters by type, customer, tags, date range. |
| **`command-palette/`** | `⌘K` command palette. All operations available as commands. Extensible. |
| **`template-picker/`** | Template selection UI for QuickAdd-style note creation. Customer/stream suggesters. |
| **`settings/`** | App configuration, vault management, hotkey binding. |

---

## 3. Feature Mapping — Obsidian Plugin → Built-in Feature

| Obsidian Plugin | Built-in Feature | Implementation |
|---|---|---|
| **Templater** | **Template Engine** | Eta-based template system. Templates in `Assets/templates/`. Template variables: `date.*`, `file.*`, `vault.*`, `prompt()` (interactive), `system.*`. User scripts in `Assets/scripts/` as JS/TS modules. Folder-to-template mapping in vault config. |
| **Tasks** | **Task Engine** | Custom task parser recognizes all 7 statuses (`[ ]`, `[/]`, `[b]`, `[w]`, `[h]`, `[x]`, `[-]`). Inline fields extracted at parse time. Task query language with `not done`, `status.symbol`, `group by`, `sort by`, `hide`, `limit`. Rendered inline in notes. |
| **Dataview** | **Query Engine** | DQL-compatible query language parsed to SQL and executed against DuckDB-WASM (frontend) or tantivy + in-memory index (backend). Supports `TABLE`, `LIST`, `TASK` output formats. `FROM`, `WHERE`, `SORT`, `GROUP BY`, `LIMIT`, `FLATTEN`. Inline expressions `= this.field`. DataviewJS via sandboxed JS execution. |
| **QuickAdd** | **Quick Create** | Command palette macros that chain: select template → prompt for variables → create note → optionally open. Configurable via YAML in `.notesapp/quickadd.yaml`. |
| **Auto Note Mover** | **Auto Router** | Rule-based file routing engine. Rules defined in `.notesapp/router-rules.yaml`. Triggered by frontmatter changes, tag additions, or explicit archive command. The archive-note logic from `reviewed-plan.md` §6.3 is built in. |
| **Periodic Notes + Calendar** | **Daily Notes + Calendar** | Daily note auto-creation (on startup or via `launchd`/CLI). Calendar sidebar widget. Configurable format, folder, template. |
| **Homepage** | **Startup View** | Configurable startup note. Default: `Dashboards/Home.md`. |
| **Linter** | **Auto-Format** | On-save hooks: maintain `created`/`updated` timestamps, sort YAML keys, trim whitespace, ensure consistent frontmatter structure. Configurable rules. |
| **Hotkeys for specific files** | **Quick Open Bindings** | Bind `⌘1`–`⌘9` to specific notes. Configurable in settings. |
| **Bookmarks** | **Pinned Items** | Pin notes, folders, and searches to the sidebar. Persisted in `.notesapp/bookmarks.json`. |
| **Obsidian Git** | **Git Integration** | Built-in git sync. Auto-commit on interval. Push/pull commands. Status indicator. Uses `git2` Rust crate (libgit2 bindings). |

---

## 4. Agentic Automation Design

The app exposes **four integration surfaces**, all backed by the same `Vault Operations` core:

```
┌──────────────────────────────────────────────┐
│              Integration Surfaces            │
│                                              │
│  1. MCP Server     ──── LLM agents           │
│  2. CLI            ──── Scripts, cron, CI     │
│  3. REST API       ──── HTTP clients, agents  │
│  4. URL Scheme     ──── macOS apps, launchers │
│                                              │
│  All four call ──► Vault Operations Core     │
└──────────────────────────────────────────────┘
```

### 4.1 Design Principles

1. **Every GUI action has a CLI/API equivalent.** If you can do it in the UI, an agent can do it programmatically.
2. **Structured output by default.** CLI and API return JSON. Agents don't parse human text.
3. **Atomic operations.** Each operation is a single transaction — no partial states.
4. **Event streaming.** The REST API and MCP server support Server-Sent Events for real-time vault change notifications.
5. **Headless mode.** The CLI can operate without the GUI running, directly on the filesystem + index.

### 4.2 Vault Operations API (internal)

Every integration surface calls these operations:

```rust
// Core vault operations — all integration surfaces map to these
trait VaultOps {
    // Notes
    fn create_note(&self, opts: CreateNoteOpts) -> Result<Note>;
    fn get_note(&self, id: NoteRef) -> Result<Note>;
    fn update_note(&self, id: NoteRef, content: String) -> Result<Note>;
    fn append_to_note(&self, id: NoteRef, content: String) -> Result<Note>;
    fn prepend_to_note(&self, id: NoteRef, content: String) -> Result<Note>;
    fn delete_note(&self, id: NoteRef) -> Result<()>;
    fn move_note(&self, id: NoteRef, dest: VaultPath) -> Result<Note>;
    fn archive_note(&self, id: NoteRef) -> Result<Note>;  // the ⌘⇧A equivalent

    // Frontmatter
    fn get_frontmatter(&self, id: NoteRef) -> Result<Frontmatter>;
    fn set_frontmatter(&self, id: NoteRef, key: &str, value: Value) -> Result<()>;
    fn update_frontmatter(&self, id: NoteRef, patch: Map) -> Result<()>;

    // Tasks
    fn list_tasks(&self, query: TaskQuery) -> Result<Vec<Task>>;
    fn update_task_status(&self, id: TaskRef, status: TaskStatus) -> Result<()>;
    fn add_task(&self, note_id: NoteRef, task: NewTask) -> Result<Task>;

    // Queries
    fn query_dataview(&self, dql: &str) -> Result<QueryResult>;
    fn query_sql(&self, sql: &str) -> Result<QueryResult>;
    fn search(&self, query: &str, filters: SearchFilters) -> Result<Vec<SearchHit>>;

    // Templates
    fn create_from_template(&self, template: &str, vars: Map) -> Result<Note>;
    fn list_templates(&self) -> Result<Vec<Template>>;

    // Vault management
    fn list_notes(&self, filters: NoteFilters) -> Result<Vec<NoteSummary>>;
    fn get_daily_note(&self, date: Option<NaiveDate>) -> Result<Note>;
    fn create_daily_note(&self, date: Option<NaiveDate>) -> Result<Note>;
    fn list_customers(&self) -> Result<Vec<Customer>>;
    fn get_customer(&self, name: &str) -> Result<Customer>;
    fn set_customer_state(&self, name: &str, state: CustomerState) -> Result<()>;
    fn list_streams(&self, customer: Option<&str>) -> Result<Vec<Stream>>;

    // Events
    fn subscribe(&self) -> EventStream;
}
```

---

## 5. URL Scheme Design

**Scheme:** `notesapp://`

Supports `x-callback-url` convention. Write operations require an auth token (generated in settings) to prevent URL injection attacks.

### 5.1 Actions

| Action | URL | Parameters | Returns (x-success) |
|---|---|---|---|
| **Open note** | `notesapp://open` | `path=<vault-path>` or `title=<name>`, `vault=<name>`, `heading=<h>`, `block=<id>` | `id`, `path`, `url` |
| **Create note** | `notesapp://new` | `title=<t>`, `content=<c>`, `template=<name>`, `customer=<c>`, `tags=<t1,t2>`, `folder=<path>`, `silent=<bool>`, `token=<auth>` | `id`, `path`, `url` |
| **Append to note** | `notesapp://append` | `path=<vault-path>` or `title=<name>`, `content=<text>`, `token=<auth>` | `id`, `path` |
| **Daily note** | `notesapp://daily` | `date=<YYYY-MM-DD>`, `content=<c>`, `append=<bool>`, `token=<auth>` | `id`, `path`, `url` |
| **Archive note** | `notesapp://archive` | `path=<vault-path>`, `token=<auth>` | `id`, `dest`, `url` |
| **Search** | `notesapp://search` | `query=<q>`, `tag=<t>`, `customer=<c>`, `type=<type>` | `results` (JSON) |
| **Open dashboard** | `notesapp://dashboard` | `name=<home\|inbox\|customers\|streams\|tasks>` | — |
| **Set frontmatter** | `notesapp://set` | `path=<vault-path>`, `key=<k>`, `value=<v>`, `token=<auth>` | `id`, `path` |
| **Run query** | `notesapp://query` | `dql=<encoded-DQL>` or `sql=<encoded-SQL>` | `results` (JSON) |
| **Choose vault** | `notesapp://choose-vault` | — | — |

### 5.2 Examples

```bash
# Open a specific note
notesapp://open?path=Customers/Acme%20Corp/Acme%20Corp.md

# Create an external meeting note from template
notesapp://new?template=External%20Meeting&customer=Acme%20Corp&title=2026-05-08%20-%20Acme%20Corp%20-%20External%20-%20Kickoff&silent=true&token=abc123

# Append to today's daily note
notesapp://daily?content=Met%20with%20Acme%20about%20migration&append=true&token=abc123

# Archive the current inbox note
notesapp://archive?path=Inbox/draft-sow.md&token=abc123

# Search for all Acme streams
notesapp://search?customer=Acme%20Corp&type=stream

# Open home dashboard
notesapp://dashboard?name=home
```

### 5.3 Shorthand Forms

```
notesapp:///<vault-path>                    → open (e.g., notesapp:///Customers/Acme Corp/Acme Corp.md)
notesapp://vault/<vault-name>/<path>        → open in specific vault
```

---

## 6. CLI Design

**Binary:** `notesapp` (installed to PATH by the Tauri installer or `cargo install`)

The CLI operates in two modes:
1. **Connected mode** — the GUI app is running; CLI sends commands via the local REST API.
2. **Standalone mode** — no GUI; CLI operates directly on files + the cached index (`.notesapp/index.json`).

### 6.1 Command Structure

```
notesapp <command> [subcommand] [options]

Global options:
  --vault <path>           Vault directory (default: current dir or configured default)
  --format <json|text|md>  Output format (default: text for terminal, json when piped)
  --silent                 Suppress non-essential output
  --token <auth-token>     Auth token for write operations

Commands:

  # Note operations
  notesapp note create <title> [--content <c>] [--template <name>] [--customer <c>] [--tags <t1,t2>] [--folder <path>]
  notesapp note get <path-or-title> [--format json|md]
  notesapp note edit <path-or-title>                          # opens $EDITOR
  notesapp note append <path-or-title> <content>
  notesapp note prepend <path-or-title> <content>
  notesapp note delete <path-or-title> [--confirm]
  notesapp note move <path-or-title> <dest-folder>
  notesapp note archive <path-or-title>                       # the ⌘⇧A equivalent
  notesapp note list [--type <t>] [--customer <c>] [--tag <t>] [--folder <p>] [--limit <n>]
  notesapp note set <path-or-title> <key> <value>             # set frontmatter field

  # Daily notes
  notesapp daily [--date <YYYY-MM-DD>]                        # open/create today's daily note
  notesapp daily create [--date <YYYY-MM-DD>]                 # create without opening
  notesapp daily list [--since <date>] [--until <date>]

  # Tasks
  notesapp task list [--status <s>] [--customer <c>] [--stream <s>] [--due-before <d>]
  notesapp task active                                        # shortcut: not done, not blocked/waiting/hold
  notesapp task blocked
  notesapp task awaiting
  notesapp task add <note-path> <description> [--customer <c>] [--stream <s>] [--due <d>] [--priority <p>]
  notesapp task done <task-ref>
  notesapp task status <task-ref> <new-status>

  # Queries
  notesapp query <DQL-string>                                 # Dataview-style query
  notesapp query --sql <SQL-string>                           # Raw SQL against vault index
  notesapp search <query> [--tag <t>] [--customer <c>] [--type <t>] [--limit <n>]

  # Customers
  notesapp customer list [--state <s>]
  notesapp customer get <name>
  notesapp customer create <name> [--state <s>] [--tier <t>]
  notesapp customer set-state <name> <state>

  # Streams
  notesapp stream list [--customer <c>] [--status <s>]
  notesapp stream get <name>
  notesapp stream create <name> --customer <c> [--priority <p>] [--target <d>]
  notesapp stream set-status <name> <status>

  # Templates
  notesapp template list
  notesapp template render <name> [--var key=value ...]

  # Vault management
  notesapp vault info                                         # vault path, note count, index age
  notesapp vault reindex                                      # force full re-index
  notesapp vault init [<path>]                                # create new vault with folder skeleton
  notesapp vault open                                         # open GUI app for this vault

  # Server (for agent integration)
  notesapp serve [--port <n>] [--socket <path>]               # start REST API server
  notesapp mcp                                                # start MCP stdio server (for Claude/Cursor)

  # Config
  notesapp config get <key>
  notesapp config set <key> <value>
  notesapp config token generate                              # generate auth token
```

### 6.2 Pipe-Friendly Design

```bash
# Create a note from stdin
echo "Meeting notes from today" | notesapp note create "Quick Meeting" --customer "Acme Corp"

# Pipe task list to jq
notesapp task active --format json | jq '.[].description'

# Query and pipe to another tool
notesapp query "TABLE customer, status FROM Customers WHERE type = 'stream'" --format json | jq '.rows'

# Bulk archive all done daily notes
notesapp note list --type daily --folder Inbox --format json | jq -r '.[].path' | xargs -I{} notesapp note archive "{}"
```

---

## 7. MCP Server Design

The app ships an MCP server that any MCP-compatible client (Claude Desktop, Cursor, VS Code Copilot, etc.) can connect to.

### 7.1 Transport

- **stdio** (primary): The MCP client spawns `notesapp mcp` as a subprocess. JSON-RPC 2.0 messages over stdin/stdout.
- **HTTP+SSE** (secondary): When the GUI app is running, it exposes MCP over `localhost:27183/mcp` for network-accessible clients.

### 7.2 Claude Desktop Configuration

```json
{
  "mcpServers": {
    "notes": {
      "command": "notesapp",
      "args": ["mcp", "--vault", "/Users/surdy/NotesVault"]
    }
  }
}
```

### 7.3 Tools

| Tool | Description | Parameters |
|---|---|---|
| `create_note` | Create a new note | `title`, `content?`, `template?`, `customer?`, `tags?`, `folder?` |
| `get_note` | Read a note's content | `path` or `title` |
| `update_note` | Replace a note's content | `path`, `content` |
| `append_to_note` | Append content to a note | `path`, `content` |
| `set_frontmatter` | Set a frontmatter field | `path`, `key`, `value` |
| `archive_note` | Archive a note (compute destination, move) | `path` |
| `search_notes` | Full-text search | `query`, `tag?`, `customer?`, `type?`, `limit?` |
| `query_notes` | Run a DQL or SQL query | `dql?`, `sql?` |
| `list_notes` | List notes with filters | `type?`, `customer?`, `tag?`, `folder?`, `limit?` |
| `get_daily_note` | Get today's (or a specific date's) daily note | `date?` |
| `create_daily_note` | Create daily note if it doesn't exist | `date?` |
| `list_tasks` | Query tasks | `status?`, `customer?`, `stream?`, `due_before?` |
| `add_task` | Add a task to a note | `note_path`, `description`, `customer?`, `stream?`, `due?`, `priority?` |
| `update_task_status` | Change a task's status | `task_ref`, `status` |
| `list_customers` | List all customers | `state?` |
| `get_customer` | Get customer details | `name` |
| `set_customer_state` | Change customer state | `name`, `state` |
| `list_streams` | List streams of work | `customer?`, `status?` |
| `create_from_template` | Create note from template | `template`, `variables` (object) |

### 7.4 Resources

| URI Pattern | Description |
|---|---|
| `note:///{vault-path}` | Individual note content (text/markdown) |
| `note:///daily/{date}` | Daily note for a specific date |
| `note:///customer/{name}` | Customer index note |
| `note:///stream/{customer}/{stream}` | Stream note |
| `note:///vault/index` | Full vault metadata index (application/json) |
| `note:///vault/structure` | Vault folder tree (application/json) |

**Resource Templates (RFC 6570):**
```json
{"uriTemplate": "note:///{path}", "name": "Note by path"}
{"uriTemplate": "note:///daily/{date}", "name": "Daily note by date"}
{"uriTemplate": "note:///customer/{name}", "name": "Customer index"}
```

### 7.5 Prompts

| Prompt | Description | Arguments |
|---|---|---|
| `summarize_customer` | Generate summary of a customer's activity | `customer` (required) |
| `daily_briefing` | Generate today's briefing from tasks and recent notes | — |
| `draft_meeting_notes` | Pre-fill meeting note structure | `customer`, `meeting_kind`, `topic` |
| `review_streams` | Summarize stream status across customers | `customer?` |

### 7.6 Example MCP Interaction

```json
// Client: What are Acme Corp's active tasks?
{"jsonrpc":"2.0","id":1,"method":"tools/call",
 "params":{"name":"list_tasks","arguments":{"customer":"Acme Corp","status":"active"}}}

// Server response:
{"jsonrpc":"2.0","id":1,
 "result":{
   "content":[{"type":"text","text":"Found 5 active tasks for Acme Corp"}],
   "structuredContent":{
     "tasks":[
       {"description":"Send updated SOW","status":"To Do","due":"2026-05-15","stream":"Migration to v2","priority":"high"},
       {"description":"Drafting pricing model","status":"In Progress","due":"2026-05-12","stream":"Migration to v2"}
     ]
   }
 }}
```

---

## 8. REST API Design

Local HTTP server on `localhost:27183`. Token-authenticated via `Authorization: Bearer <token>` header.

### 8.1 Endpoints

```
# Health
GET  /ping                                          → {"status":"ok","vault":"...","notes":1234}

# Notes
POST   /api/notes                                   → create note
GET    /api/notes                                    → list notes (?type=&customer=&tag=&folder=&limit=&offset=)
GET    /api/notes/:path                              → get note content + metadata
PUT    /api/notes/:path                              → update note content
PATCH  /api/notes/:path                              → partial update (frontmatter fields)
DELETE /api/notes/:path                              → delete note
POST   /api/notes/:path/append                       → append content
POST   /api/notes/:path/prepend                      → prepend content
POST   /api/notes/:path/archive                      → archive note (compute dest, move)
POST   /api/notes/:path/move                         → move to specified folder

# Daily notes
GET    /api/daily                                    → get today's daily note
GET    /api/daily/:date                              → get daily note for date
POST   /api/daily                                    → create today's daily note
POST   /api/daily/:date                              → create daily note for date

# Tasks
GET    /api/tasks                                    → list tasks (?status=&customer=&stream=&due_before=&limit=)
POST   /api/tasks                                    → add task to a note
PATCH  /api/tasks/:ref                               → update task status

# Queries
POST   /api/query                                    → execute DQL or SQL query (body: {dql:...} or {sql:...})
GET    /api/search?q=...&tag=...&customer=...        → full-text search

# Customers
GET    /api/customers                                → list customers (?state=)
GET    /api/customers/:name                          → get customer details
POST   /api/customers                                → create customer (scaffolds folder + index + account info)
PATCH  /api/customers/:name                          → update customer fields

# Streams
GET    /api/streams                                  → list streams (?customer=&status=)
GET    /api/streams/:customer/:name                  → get stream details
POST   /api/streams                                  → create stream
PATCH  /api/streams/:customer/:name                  → update stream fields

# Templates
GET    /api/templates                                → list available templates
POST   /api/templates/:name/render                   → render template with variables

# Events (SSE)
GET    /api/events                                   → Server-Sent Events stream of vault changes

# Vault
GET    /api/vault/info                               → vault metadata
POST   /api/vault/reindex                            → force re-index
```

### 8.2 Event Stream

The `/api/events` endpoint streams real-time vault changes:

```
event: note_created
data: {"path":"Inbox/new-note.md","type":"note","customer":null}

event: note_updated
data: {"path":"Customers/Acme Corp/Acme Corp.md","fields_changed":["state"]}

event: note_moved
data: {"from":"Inbox/meeting.md","to":"Customers/Acme Corp/External Meetings/meeting.md"}

event: note_deleted
data: {"path":"Inbox/scratch.md"}

event: index_rebuilt
data: {"notes":1234,"duration_ms":450}
```

---

## 9. Template Engine Design

### 9.1 Template Syntax

Templates use Eta syntax (`<% %>` for logic, `<%= %>` for output), which maps naturally from Templater's syntax.

**Templater → Eta mapping:**

| Templater | Eta (ours) | Notes |
|---|---|---|
| `<% tp.date.now("YYYY-MM-DD") %>` | `<%= date.now("YYYY-MM-DD") %>` | `date` is a built-in helper |
| `<% tp.file.title %>` | `<%= file.title %>` | `file` is injected per-template |
| `<% tp.frontmatter.customer %>` | `<%= fm.customer %>` | `fm` is current note's frontmatter |
| `<% await tp.user.list_customers() %>` | `<%= await vault.listCustomers() %>` | `vault` exposes vault operations |
| `<% tp.system.suggester(...) %>` | `<%= await prompt.suggest(items, labels) %>` | `prompt` provides interactive prompts |
| `<% tp.date.now("YYYY-MM-DD", 7) %>` | `<%= date.add(7, "days").format("YYYY-MM-DD") %>` | Chainable date helpers |
| `<% tp.file.folder(true) %>` | `<%= file.folder %>` | Full folder path |

### 9.2 Built-in Template Helpers

```typescript
// Available in all templates as `it.*` (Eta convention) or destructured
interface TemplateContext {
  date: {
    now(format?: string): string;
    today(): string;                          // YYYY-MM-DD
    yesterday(format?: string): string;
    tomorrow(format?: string): string;
    add(n: number, unit: string): DateHelper;
    format(date: string, format: string): string;
  };
  file: {
    title: string;                            // filename without extension
    name: string;                             // filename with extension
    path: string;                             // full vault-relative path
    folder: string;                           // parent folder path
    createdAt: string;
    ext: string;
  };
  fm: Record<string, any>;                    // current note's frontmatter
  vault: {
    name: string;
    path: string;
    listCustomers(): Promise<string[]>;
    listStreams(customer?: string): Promise<string[]>;
    listTemplates(): Promise<string[]>;
    getNote(path: string): Promise<Note>;
  };
  prompt: {
    text(label: string, defaultValue?: string): Promise<string>;
    suggest(items: string[], labels?: string[]): Promise<string>;
    confirm(message: string): Promise<boolean>;
    date(label: string, defaultDate?: string): Promise<string>;
  };
  system: {
    clipboard(): Promise<string>;
    env(key: string): string | undefined;
  };
}
```

### 9.3 Template File Format

Templates are plain markdown files in `Assets/templates/` with Eta delimiters:

```markdown
---
type: meeting
meeting-kind: external
customer: "<%= await prompt.suggest(await vault.listCustomers(), null) %>"
stream:
date: <%= date.today() %>
attendees: []
created: <%= date.now("YYYY-MM-DD HH:mm") %>
updated: <%= date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: [meeting, external]
---

# <%= date.today() %> — [[<%= fm.customer %>]] — External: <%= await prompt.text("Topic") %>

**Customer:** [[<%= fm.customer %>]]
**Stream:** <%= fm.stream ? `[[${fm.stream}]]` : "_n/a_" %>

## Agenda

## Notes

## Action items (ours)
- [ ] Example task [customer:: [[<%= fm.customer %>]]] [owner:: me] 📅 <%= date.add(7, "days").format("YYYY-MM-DD") %>

## Action items (theirs)
- [w] Awaiting ... [customer:: [[<%= fm.customer %>]]] [owner:: customer] ⏳ <%= date.add(7, "days").format("YYYY-MM-DD") %>
```

### 9.4 Folder-to-Template Mapping

Configured in `.notesapp/config.yaml`:

```yaml
templates:
  folder: Assets/templates
  scripts: Assets/scripts
  mappings:
    "Inbox/Daily": T - Daily Note.md
    "Inbox": T - Generic Note.md
    "Customers/*/External Meetings": T - External Meeting.md
    "Customers/*/Internal Meetings": T - Internal Meeting.md
    "Customers/*/Streams": T - Stream of Work.md
    "Customers/*/Account Info": T - Account Info.md
```

### 9.5 User Scripts

User scripts in `Assets/scripts/` are JavaScript modules that export functions:

```javascript
// Assets/scripts/list-customers.js
export default async function(vault) {
  const customers = await vault.listCustomers();
  return customers.filter(c => c.state === 'Active').map(c => c.name);
}
```

Used in templates:
```
<%= await scripts.run("list-customers") %>
```

---

## 10. Query Engine Design

### 10.1 Architecture

The query engine has three layers:

1. **Vault Index** — in-memory JSON array of all notes' metadata, rebuilt incrementally.
2. **DQL Parser** — parses Dataview Query Language to SQL.
3. **Execution** — DuckDB-WASM (frontend) or tantivy + in-memory filtering (backend).

### 10.2 Vault Index Schema

Every note is parsed into a JSON record:

```json
{
  "file": {
    "path": "Customers/Acme Corp/Streams/Migration to v2.md",
    "name": "Migration to v2.md",
    "folder": "Customers/Acme Corp/Streams",
    "ext": "md",
    "cday": "2026-04-01",
    "mday": "2026-05-08",
    "ctime": "2026-04-01T09:00:00",
    "mtime": "2026-05-08T14:30:00",
    "size": 2048,
    "link": "[[Migration to v2]]",
    "outlinks": ["[[Acme Corp]]", "[[Account Info]]"],
    "inlinks": ["[[2026-05-07]]", "[[Tasks - Active]]"],
    "tags": ["stream"],
    "tasks": [
      {
        "text": "Send updated SOW",
        "status": " ",
        "status_name": "To Do",
        "line": 42,
        "fields": {"customer": "[[Acme Corp]]", "stream": "[[Migration to v2]]", "owner": "me"},
        "due": "2026-05-15",
        "priority": "high"
      }
    ]
  },
  "type": "stream",
  "customer": "Acme Corp",
  "status": "In Progress",
  "priority": "P2",
  "started": "2026-04-01",
  "target": "2026-06-30",
  "archived": false,
  "created": "2026-04-01T09:00:00",
  "updated": "2026-05-08T14:30:00"
}
```

### 10.3 DQL Compatibility

The engine parses Dataview Query Language blocks and translates to SQL:

**Supported DQL syntax:**

```
TABLE [field1 [AS "Alias"], field2, ...] FROM "folder" [AND/OR #tag] [AND/OR [[link]]]
WHERE <condition>
SORT <field> [ASC|DESC]
GROUP BY <field>
LIMIT <n>
FLATTEN <field> AS <alias>

LIST [expression] FROM ...
TASK FROM ...

Inline: `= this.field`
```

**Translation example:**

```
TABLE status, target
FROM "Customers/Acme Corp/Streams"
WHERE type = "stream" AND status != "Done"
SORT status ASC, target ASC
```

Becomes:

```sql
SELECT file_path, file_link, status, target
FROM vault_index
WHERE file_folder LIKE 'Customers/Acme Corp/Streams%'
  AND type = 'stream'
  AND status != 'Done'
ORDER BY status ASC, target ASC
```

### 10.4 Task Query Compatibility

Task query blocks are parsed similarly:

```
not done
status.symbol does not include b
status.symbol does not include w
status.symbol does not include h
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by priority, due
hide backlink
short mode
```

Becomes a structured task query that filters the tasks array across all indexed notes.

### 10.5 DataviewJS Support

DataviewJS blocks (`dataviewjs`) execute in a sandboxed JavaScript environment with access to:

```typescript
// Available in DataviewJS blocks
const dv = {
  pages(source?: string): Page[];           // query pages
  page(path: string): Page;                 // get single page
  current(): Page;                          // current note
  table(headers: string[], rows: any[][]): void;  // render table
  list(items: any[]): void;                 // render list
  taskList(tasks: Task[]): void;            // render task list
  paragraph(text: string): void;            // render text
  header(level: number, text: string): void;
};
```

### 10.6 Rendering

Query results render inline in the note view:
- **TABLE** → HTML table with sortable columns
- **LIST** → Bullet list with links
- **TASK** → Interactive task list (checkboxes are clickable, status toggles write back to source file)

---

## 11. Implementation Phases

### Phase 1 — Foundation (weeks 1–3)

**Goal:** Basic app that opens a vault, shows files, and edits markdown.

| # | Task | Details |
|---|---|---|
| 1.1 | **Tauri scaffold** | `cargo create-tauri-app` with SolidJS template. Configure `tauri.conf.json`. |
| 1.2 | **Vault core** | Vault discovery, config loading (`.notesapp/config.yaml`), path resolution. |
| 1.3 | **File explorer** | Tree view of vault folders. Click to open. Bookmarks sidebar. |
| 1.4 | **CodeMirror editor** | Basic markdown editing. Syntax highlighting. YAML frontmatter highlighting. Save to disk via Tauri IPC. |
| 1.5 | **File watcher** | Rust `notify` crate. Detect external changes, reload open editors. |
| 1.6 | **Vault index (v1)** | Walk all `.md` files on startup. Parse frontmatter with serde_yaml. Build in-memory index. Persist to `.notesapp/index.json`. |

**Deliverable:** An app that opens a vault, browses files, edits markdown, and auto-saves.

### Phase 2 — Obsidian Markdown Extensions (weeks 4–6)

**Goal:** Full OFM rendering and editing support.

| # | Task | Details |
|---|---|---|
| 2.1 | **Wikilink support** | Lezer extension for `[[...]]` parsing. Decoration to render as clickable links. Navigation on click. Backlink tracking in vault index. |
| 2.2 | **Custom task statuses** | Lezer extension for `[/]`, `[b]`, `[w]`, `[h]`, `[-]`. Checkbox rendering with status-specific icons/colors. Click to cycle status. |
| 2.3 | **Inline fields** | Lezer extension for `[key:: value]`. Decoration rendering. Extraction to vault index. |
| 2.4 | **Callouts** | Remark plugin for `> [!TYPE]` blockquote callouts. Render with appropriate styling (tip, warning, note, etc.). |
| 2.5 | **YAML frontmatter** | Properties panel (like Obsidian's). Visual editor for frontmatter fields. Type-aware inputs (date picker, dropdown for `state`/`status`). |
| 2.6 | **Markdown preview** | Rendered preview pane using the full remark pipeline. Toggle between source/preview/live modes. |

**Deliverable:** Notes render identically to Obsidian. All OFM extensions work.

### Phase 3 — Query Engine & Dashboards (weeks 7–9)

**Goal:** Dataview and Tasks plugin replacement.

| # | Task | Details |
|---|---|---|
| 3.1 | **Vault index (v2)** | Full metadata extraction: frontmatter, inline fields, tasks (with all fields), tags, links, outlinks/inlinks. Incremental re-index on file change. |
| 3.2 | **DuckDB integration** | Load vault index into DuckDB-WASM. Create `vault_index` and `vault_tasks` tables. |
| 3.3 | **DQL parser** | Parse `TABLE`/`LIST`/`TASK` queries. Translate to SQL. Handle `FROM`, `WHERE`, `SORT`, `GROUP BY`, `LIMIT`, `FLATTEN`. |
| 3.4 | **Task query parser** | Parse task query blocks (`not done`, `status.symbol includes`, `group by`, `sort by`, `hide`, `limit`). |
| 3.5 | **Inline queries** | Parse and render `` `= this.field` `` inline expressions. |
| 3.6 | **Dashboard rendering** | Detect `dataview` and `tasks` code blocks in notes. Execute queries. Render results (tables, lists, task lists). Live-update on vault changes. |
| 3.7 | **Full-text search** | FlexSearch index built from note content. Global search UI (`⌘⇧F`). |

**Deliverable:** All dashboard notes from `reviewed-plan.md` render correctly with live data.

### Phase 4 — Templates & Quick Create (weeks 10–11)

**Goal:** Templater and QuickAdd replacement.

| # | Task | Details |
|---|---|---|
| 4.1 | **Eta integration** | Template rendering with all helpers (date, file, fm, vault, prompt, system). |
| 4.2 | **Template picker** | UI for selecting template. Folder-to-template mapping. |
| 4.3 | **Interactive prompts** | Suggester (customer picker, stream picker). Text input. Date picker. Used within template rendering. |
| 4.4 | **Quick Create macros** | Configurable macros: New External Meeting, New Internal Meeting, New Stream, New Customer (scaffolds entire folder). |
| 4.5 | **User scripts** | Load and execute JS modules from `Assets/scripts/`. Expose `vault` context. |

**Deliverable:** All nine templates from `reviewed-plan.md` work. QuickAdd-style macros create notes from templates.

### Phase 5 — Inbox & Auto-Router (weeks 12–13)

**Goal:** Inbox workflow and archive automation.

| # | Task | Details |
|---|---|---|
| 5.1 | **Archive command** | `⌘⇧A` triggers archive. Reads frontmatter, computes destination per `reviewed-plan.md` §6.2. Stamps `archived: true` + `archived-at`. Moves file. Updates vault index. Shows notification. |
| 5.2 | **Router rules** | Configurable routing rules in `.notesapp/router-rules.yaml`. Tag-based fallback. |
| 5.3 | **Auto-format on save** | Maintain `updated` timestamp. Sort YAML keys. Trim whitespace. Stamp `created` if missing. |
| 5.4 | **Daily note automation** | Auto-create daily note on startup (if configured). `launchd` plist for pre-creation. |
| 5.5 | **Homepage** | Configurable startup note. Open `Dashboards/Home.md` on launch. |

**Deliverable:** Full Inbox→Archive workflow works. Daily notes auto-generate.

### Phase 6 — Agent Integration (weeks 14–16)

**Goal:** MCP, CLI, REST API, and URL scheme.

| # | Task | Details |
|---|---|---|
| 6.1 | **CLI** | Clap-based CLI with all commands from §6.1. Standalone mode (direct file access) + connected mode (REST API). |
| 6.2 | **REST API** | Axum server on `localhost:27183`. All endpoints from §8.1. Token auth. SSE event stream. |
| 6.3 | **MCP server** | JSON-RPC 2.0 over stdio. All tools, resources, and prompts from §7. `notesapp mcp` command. |
| 6.4 | **URL scheme** | Register `notesapp://` via `tauri-plugin-deep-link`. Parse and dispatch all actions from §5.1. x-callback-url support. |
| 6.5 | **Auth token** | Generate/manage auth tokens for write operations via CLI and URL scheme. |

**Deliverable:** An agent (Claude, Cursor, or a script) can fully manage the vault via MCP, CLI, REST, or URL scheme.

### Phase 7 — Polish & Parity (weeks 17–20)

| # | Task | Details |
|---|---|---|
| 7.1 | **Git integration** | Auto-commit, push/pull, status indicator. `git2` crate. |
| 7.2 | **Command palette** | `⌘K` palette with all commands. Fuzzy search. Recent commands. |
| 7.3 | **Hotkeys** | Configurable keybindings. Defaults for common operations. |
| 7.4 | **Graph view** | Note link graph visualization. Backlink panel. |
| 7.5 | **Theme engine** | CSS-based themes. Support Obsidian CSS snippet format. |
| 7.6 | **DataviewJS** | Sandboxed JS execution for `dataviewjs` blocks. `dv.*` API. |
| 7.7 | **Mobile (stretch)** | Tauri 2.x supports iOS/Android. Basic viewing/editing on mobile. |
| 7.8 | **Vault migration tool** | Import script that validates an Obsidian vault is compatible. Converts `.obsidian/` config to `.notesapp/`. |

---

## 12. Configuration

### 12.1 `.notesapp/` Directory

```
.notesapp/
  config.yaml              ← main app config
  index.json               ← cached vault index (rebuilt on startup)
  bookmarks.json           ← pinned items
  quickadd.yaml            ← Quick Create macro definitions
  router-rules.yaml        ← auto-routing rules
  keybindings.json         ← custom hotkeys
  auth-tokens.json         ← API/URL scheme auth tokens (hashed)
  themes/                  ← custom CSS themes
```

### 12.2 `config.yaml` Example

```yaml
vault:
  name: "Notes"
  default_note_location: "Inbox"
  default_attachment_location: "Assets/data"
  link_format: "shortest"
  use_wikilinks: true
  auto_update_links: true

editor:
  font_family: "JetBrains Mono"
  font_size: 14
  line_numbers: false
  vim_mode: false
  spell_check: true
  auto_save: true
  auto_save_interval: 1000  # ms

startup:
  homepage: "Dashboards/Home.md"
  create_daily_note: true

templates:
  folder: "Assets/templates"
  scripts: "Assets/scripts"
  mappings:
    "Inbox/Daily": "T - Daily Note.md"
    "Inbox": "T - Generic Note.md"
    "Customers/*/External Meetings": "T - External Meeting.md"
    "Customers/*/Internal Meetings": "T - Internal Meeting.md"
    "Customers/*/Streams": "T - Stream of Work.md"
    "Customers/*/Account Info": "T - Account Info.md"

daily_notes:
  folder: "Inbox/Daily"
  format: "YYYY-MM-DD"
  template: "T - Daily Note.md"

linter:
  auto_created_updated: true
  sort_yaml: true
  trim_whitespace: true

task_statuses:
  - { symbol: " ", name: "To Do", type: "TODO" }
  - { symbol: "/", name: "In Progress", type: "IN_PROGRESS" }
  - { symbol: "b", name: "Blocked", type: "TODO" }
  - { symbol: "w", name: "Awaiting Customer", type: "TODO" }
  - { symbol: "h", name: "On Hold", type: "TODO" }
  - { symbol: "x", name: "Done", type: "DONE" }
  - { symbol: "-", name: "Cancelled", type: "CANCELLED" }

api:
  port: 27183
  bind: "127.0.0.1"

git:
  enabled: false
  auto_commit: false
  commit_interval: 300  # seconds
  commit_message: "vault backup: {{date}}"
```

---

## 13. Library Research Summary

### 13.1 What Exists (use directly)

| Need | Library | Maturity |
|---|---|---|
| Markdown parsing | `unified` + `remark-parse` + `remark-gfm` + `remark-frontmatter` | Production (150+ plugins) |
| Wikilinks in remark | `remark-wiki-link` (`landakram/remark-wiki-link`) | Stable |
| Markdown editor | CodeMirror 6 (`@codemirror/lang-markdown`) | Production (Obsidian uses it) |
| Frontmatter parsing | `gray-matter` (JS), `serde_yaml` (Rust) | Production |
| Rust markdown | `markdown` crate (`wooorm/markdown-rs`) | Stable (same author as micromark) |
| File watching | `notify` crate (Rust), `chokidar` v5 (JS) | Production |
| SQL over JSON | `@duckdb/duckdb-wasm` | Production |
| Full-text search | `flexsearch` (JS), `tantivy` (Rust) | Production |
| Template engine | `eta` | Stable, 3.5KB |
| Desktop framework | Tauri 2.x | Production (security-audited) |
| CLI framework | `clap` 4 (Rust) | Production |
| HTTP server | `axum` (Rust) | Production |

### 13.2 What Must Be Built (custom extensions)

| Need | Approach | Effort |
|---|---|---|
| **Custom task statuses** (`[b]`, `[w]`, `[h]`) | Lezer markdown extension + remark plugin. ~300 lines. | 2–3 days |
| **Inline fields** (`[key:: value]`) | Lezer inline extension + remark text-node transformer. Reference: Obsidian Dataview source. ~500 lines. | 3–4 days |
| **Obsidian callouts** (`> [!NOTE]`) | Remark blockquote transformer plugin. ~200 lines. | 1–2 days |
| **DQL parser** | PEG parser or hand-written recursive descent. Translate to SQL. ~1500 lines. | 1–2 weeks |
| **Task query parser** | Simpler than DQL. ~500 lines. | 3–4 days |
| **Archive router** | Port of `archive-note.js` to Rust. ~200 lines. | 1 day |
| **MCP server** | JSON-RPC 2.0 implementation over stdio + HTTP SSE. ~1000 lines. | 1 week |

### 13.3 What Does NOT Exist (confirmed gaps)

1. **No standalone OFM parser.** Obsidian's parser is proprietary. You assemble from remark + custom extensions.
2. **No `[key:: value]` parser on npm.** Must build from Dataview's source as reference.
3. **No DQL parser on npm.** Must build from Dataview's source as reference.
4. **No remark plugin for `> [!NOTE]` callouts.** Must build (remark-directive handles `:::` syntax only).
5. **No library for custom checkbox statuses beyond `[x]`.** Must extend Lezer/remark-gfm.

---

## 14. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **DQL compatibility gaps** | High | Medium | Start with the subset used in `reviewed-plan.md` dashboards. Document unsupported DQL features. Offer raw SQL as escape hatch. |
| **CodeMirror extension complexity** | Medium | High | Wikilinks, inline fields, and custom checkboxes each need Lezer extensions. Budget 2 weeks. Study Obsidian's CM6 usage (it's documented in their API docs). |
| **Tauri webview inconsistencies** | Medium | Medium | Test on macOS (WKWebView), Windows (WebView2), Linux (WebKitGTK). Safari quirks in WKWebView are the main concern. |
| **Performance at scale (10k+ notes)** | Low | Medium | The vault index + DuckDB + FlexSearch approach should handle 50k+ notes. Incremental indexing prevents startup lag. |
| **Obsidian ecosystem lock-in** | Low | Low | Files remain plain markdown on disk. Users can switch back to Obsidian at any time — just open the same folder. |

---

## 15. Success Criteria

The app is ready for daily use when:

1. ✅ All nine templates from `reviewed-plan.md` create valid notes.
2. ✅ All five dashboard notes render with live data.
3. ✅ All five task aggregation views (`Active`, `Blocked`, `Awaiting Customer`, `On Hold`, `By Customer`) work.
4. ✅ `⌘⇧A` archives a note to the correct destination.
5. ✅ Daily notes auto-create on startup.
6. ✅ An MCP client (Claude Desktop) can search notes, create notes, list tasks, and archive notes.
7. ✅ The CLI can perform all operations the GUI can.
8. ✅ Opening the same vault folder in Obsidian still works — files are not corrupted or incompatible.
9. ✅ Vault with 1000+ notes indexes in under 2 seconds.
10. ✅ App binary is under 20MB (vs Obsidian's ~300MB with Electron).
