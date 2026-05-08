# Custom Markdown Notes App Plan

> Goal: replace the Obsidian-dependent customer notes workflow with a native, file-based markdown application that remains compatible with Obsidian-flavored markdown while making agentic automation a first-class interface.

## 1. Product position and non-negotiables

Build a desktop-first app for managing the existing customer relationship notes method, not a general Obsidian clone. The app should open and modify the same vault structure described in the reviewed Obsidian plan, preserve plain markdown as the source of truth, and expose every workflow through UI, URL scheme, CLI, and agent protocols.

Non-negotiables:

- **Plain files are canonical.** Markdown files and folders are the database. The app may keep disposable in-memory indexes and optional rebuildable cache files, but no required persistent database.
- **Obsidian-flavored markdown compatibility.** Support YAML frontmatter, wikilinks, embeds, aliases, callouts, headings, tags, custom checkbox statuses, inline fields like `[customer:: [[Acme Corp]]]`, and fenced `dataview`/`tasks` code blocks.
- **Same vault layout.** Use `Inbox/`, `Tasks/`, `Customers/`, `General/`, `Dashboards/`, `Assets/`, and `Archive/` unchanged.
- **Native feature set.** Replace the required Obsidian plugins with built-in modules; do not design a plugin system for v1.
- **Agent-operable by design.** Any action available in the UI must also be available through CLI, URL scheme, MCP tools, and ACP sessions.
- **Safe automation.** Multi-file moves, generated notes, task rewrites, and archive actions must be atomic, conflict-aware, undoable, and easy for agents to dry-run.

## 2. Recommended technology stack

### Opinionated recommendation

Use **Tauri 2 + Rust core + React/TypeScript UI + CodeMirror 6**.

Why this stack:

- Rust gives safe filesystem operations, fast parsing/indexing, and a natural fit for TurboVault, MCP servers, and atomic batch edits.
- Tauri gives a small desktop shell, native URL scheme handling, macOS automation friendliness, and a clean bridge between UI and core commands.
- React/TypeScript gives fast product iteration for dashboards, command palette, forms, and agent status panels.
- CodeMirror 6 is the right editor layer for rich markdown editing, custom syntax extensions, inline task controls, and live preview widgets.

### Core stack

| Layer | Recommendation | Notes |
|---|---|---|
| Desktop shell | Tauri 2 | Smaller and more local-file-friendly than Electron; native custom protocol support. |
| Core language | Rust | Vault, parser, index, query engine, router, archive actions, MCP/ACP adapters. |
| UI language | TypeScript | React app, URL command dispatch, dashboard rendering, template forms. |
| UI framework | React 19 + Vite | Good ecosystem, fast local dev, easy component testing. |
| Editor | CodeMirror 6 | Use markdown language package plus custom OFM extensions. |
| Styling | CSS modules or Tailwind | Pick one. I recommend CSS modules initially to avoid framework churn. |
| State | Zustand | Simple local app state; durable data remains in files. |
| Tests | Rust unit/integration tests + Vitest + Playwright | Parser/query tests in Rust; UI workflow tests in Playwright. |
| Packaging | Tauri bundler | macOS first; Windows/Linux later. |

### Key libraries researched

| Need | Library/project | Current observed version/source | Recommendation |
|---|---|---:|---|
| Vault operations + OFM + MCP | `turbovault`, `turbovault-parser`, `turbovault-vault`, `turbovault-batch` | Cargo `1.5.0`; README describes Rust SDK, Obsidian-flavored markdown parsing, vault I/O, atomic batch operations, and MCP tools | **Use as primary Rust vault substrate if license/API fit.** Wrap behind `VaultEngine` so it can be replaced. |
| File watching | Rust `notify` or JS `chokidar` | Cargo `notify 9.0.0-rc.4`; npm `chokidar 5.0.0` | Prefer Rust `notify` inside core; avoid duplicate JS watcher. |
| Markdown editor | CodeMirror packages | `@codemirror/state 6.6.0`, `@codemirror/view 6.42.1`, `@codemirror/lang-markdown 6.5.0`, `@lezer/markdown 1.6.3` | Use for edit surface and syntax tree extensions. |
| Markdown parsing fallback | `pulldown-cmark`, `pulldown-cmark-to-cmark`, `micromark`, `remark` | Cargo `pulldown-cmark 0.13.3`; npm `micromark 4.0.2`, `remark 15.0.1` | Use TurboVault parser first; use pulldown/Lezer for gaps and tests. |
| YAML | Rust `serde_yaml`/`yaml-rust2` or TS `yaml` | npm `yaml 2.8.4`; `js-yaml 4.1.1` | Parse/write frontmatter in Rust with comment-preserving strategy where possible; TS only for UI previews. |
| Search | `tantivy`, `MiniSearch`, `FlexSearch` | Cargo `tantivy 0.26.1`; npm `minisearch 7.2.0`, `flexsearch 0.8.212` | For v1, use in-memory Rust index or MiniSearch. Add Tantivy only if vault scale requires it; do not require persistent index. |
| Template rendering | `liquidjs`, `handlebars`, `eta` | `liquidjs 10.25.7`, `handlebars 4.7.9`, `eta 4.6.0` | Use a restricted Liquid-like engine or Rust `liquid`, not arbitrary JS execution. |
| Dates | `date-fns`, Rust `chrono` | `date-fns 4.1.0`; Rust chrono is mature | Use Rust date logic for core; UI can use date-fns for display. |
| Natural dates | `chrono-node` | `2.9.1` | Optional for quick task due dates like “next Friday”. |
| URL/CLI parsing | `commander`, `oclif`, Rust `clap` | `commander 14.0.3`, `oclif 4.23.0` | Prefer Rust `clap` for one native CLI binary sharing core. |
| Schema validation | `zod` | `4.4.3` | Use in TypeScript for URL payloads/forms; Rust uses serde/validator. |
| MCP | `@modelcontextprotocol/sdk`, TurboVault MCP, Rust MCP libraries | `@modelcontextprotocol/sdk 1.29.0`; TurboVault has MCP server | Prefer Rust MCP endpoint backed by the same command bus; reuse TurboVault tools where useful. |
| ACP | `@agentclientprotocol/sdk` / Rust `agent-client-protocol` | TS SDK `0.21.0`; ACP README lists TypeScript, Rust, Python, Java, Kotlin SDKs | Implement ACP as an app/client adapter so coding agents can treat the vault like a workspace. |
| Syntax highlighting | `shiki` | `4.0.2` | Use for preview code fences if needed. |
| Formatting | `prettier` | `3.8.3` | Optional for markdown formatting; do not run on notes without explicit user opt-in. |

Research conclusion: **TurboVault is the best fit to evaluate first** because it already targets Obsidian-flavored Markdown, vault operations, graph/search, atomic batch work, and MCP. The risk is maturity and API stability, so the app should depend on an internal `VaultEngine` trait rather than coupling the product directly to TurboVault types.

## 3. Architecture overview

```text
notesapp/
  src-tauri/                         Rust desktop shell + native services
    core/
      vault_engine/                  file I/O, atomic writes, watcher, path safety
      markdown/                      OFM parser, frontmatter, links, tasks, inline fields
      index/                         in-memory indexes and graph
      query/                         Dataview/Tasks-compatible query engine
      templates/                     safe template renderer
      workflows/                     archive, create, daily note, customer/stream actions
      command_bus/                   one command API used by UI, CLI, URL, agents
      protocols/
        url_scheme.rs
        cli.rs
        mcp.rs
        acp.rs
      audit/                         operation log, undo manifests, dry-run diffs
  app/                               React UI
    editor/                          CodeMirror extensions and preview
    dashboards/                      native dashboard components
    command_palette/
    forms/                           create note/task/customer/stream forms
    agent_console/                   agent session visibility and approvals
  cli/                               optional thin wrapper, if separate from Tauri binary
  tests/
```

### Core design principle: one command bus

Every operation must flow through a single typed command bus:

```text
UI click -> CommandBus
URL open -> CommandBus
CLI command -> CommandBus
MCP tool -> CommandBus
ACP request -> CommandBus
scheduled job -> CommandBus
```

This prevents drift between manual and agentic workflows. Commands return structured results:

```json
{
  "ok": true,
  "operationId": "op_20260508_145202_abc123",
  "changedFiles": ["Customers/Acme Corp/Streams/Migration to v2.md"],
  "warnings": [],
  "undoAvailable": true
}
```

### Main modules

#### `VaultEngine`

Responsibilities:

- Open a vault rooted at a local directory.
- Enforce path containment and reject path traversal.
- Normalize paths to vault-relative POSIX style.
- Read/write/move/delete markdown and asset files.
- Perform atomic writes: write replacement content, fsync where appropriate, rename into place.
- Execute multi-file batches with preflight conflict checks.
- Watch for external changes and trigger incremental reindex.
- Maintain undo manifests for app-initiated operations.

#### `MarkdownEngine`

Responsibilities:

- Parse YAML frontmatter.
- Parse headings, blocks, wikilinks, embeds, tags, callouts, tasks, inline fields, block IDs, and Dataview/Tasks code fences.
- Preserve source positions for surgical edits.
- Render preview HTML safely.
- Provide round-trip editing helpers: update frontmatter key, set task status, insert task, rewrite inline field.

#### `IndexEngine`

All indexes are derived from files and can be rebuilt:

- `NoteIndex`: path, title, frontmatter, aliases, type, customer, stream, date, tags, archived.
- `TaskIndex`: source path, line/range, status, text, due, scheduled, start, completed, priority, inline fields.
- `LinkIndex`: forward links, backlinks, unresolved links, embeds.
- `CustomerIndex`: customer name, folder, index note, state, last meeting, open tasks, active streams.
- `StreamIndex`: customer, status, note path, open task counts, stale age.
- `DailyIndex`: daily note dates and archive status.
- `SearchIndex`: in-memory full-text/fuzzy search.

#### `QueryEngine`

Provides native rendering for:

- Dataview-like `TABLE`, `LIST`, and `TASK` queries.
- Tasks-like task filters, grouping, sorting, and limits.
- Built-in dashboards that can either render query blocks or use first-class components.

Important: keep the query syntax as close as possible to Dataview/Tasks so files remain Obsidian-readable. The app can render more capable native UI without changing the source markdown.

#### `WorkflowEngine`

Encodes the reviewed method as first-class workflows:

- Create daily note.
- Create customer.
- Create account info/glossary/milestones.
- Create internal/external meeting.
- Create stream of work.
- Archive current note from Inbox.
- Move inactive customer to archive.
- Triage inbox.
- Generate task views.
- Normalize frontmatter timestamps.

#### `AgentBridge`

Adapters for CLI, URL scheme, MCP, ACP, and local JSON-RPC. These adapters should not contain business logic; they call the same command bus as the UI.

## 4. Feature mapping from Obsidian plugins to native features

| Obsidian dependency | Native replacement | Design |
|---|---|---|
| Templater | Built-in template engine | Templates live in `Assets/templates/`; safe Liquid-like expressions, helpers, prompts, defaults, folder mappings, no arbitrary JS in templates. |
| Tasks plugin | Native task model/query engine | Parse custom statuses `[ ]`, `[/]`, `[b]`, `[w]`, `[h]`, `[x]`, `[-]`; support priorities, dates, inline fields, group/sort/filter views. |
| Dataview | Native metadata/query engine | Parse frontmatter and inline fields; render `dataview` blocks; provide native dashboard components backed by same indexes. |
| QuickAdd | Command palette + create workflows | Prompted commands for meeting/customer/stream/task/daily; available via UI, CLI, URL, MCP, ACP. |
| Auto Note Mover | Archive/router workflow | Explicit archive command reads frontmatter, computes destination, stamps archive fields, moves file, updates indexes. |
| Periodic Notes + Calendar | Built-in daily notes/calendar | Internal scheduler plus optional OS launch agent; calendar sidebar; prev/next navigation generated in template. |
| Homepage | Startup route | App preference `startupNote: Dashboards/Home.md`; URL/CLI can override. |
| Linter | Metadata maintenance | On save, update `updated`; on create, set `created`; optional YAML key ordering; never reformat body unexpectedly. |
| Hotkeys for specific files | Native hotkey map | Default `⌘1` Home, `⌘2` Active Tasks, `⌘3` Inbox Triage, etc.; user configurable JSON. |
| Bookmarks | Native pinned items | Pin notes/folders/searches/dashboards in sidebar; stored in app config, not notes. |

## 5. Vault compatibility model

### Folder structure

Use the reviewed structure exactly:

```text
Inbox/
  Daily/
Tasks/
Customers/
General/
  Journal/
Dashboards/
Assets/
  templates/
  scripts/
  data/
Archive/
```

The app should create missing standard folders on first open after user confirmation or via `notesapp vault init --apply`.

### Frontmatter schema

Support the reviewed keys exactly:

```yaml
type: daily | meeting | stream | customer | account-info | glossary | milestones | note
meeting-kind: internal | external
customer: "[[Acme Corp]]"
stream: "[[Migration to v2]]" # or null
state: Active | On Hold | Temp | Inactive
status: In Progress | Blocked | Done | Awaiting Customer | On Hold
date: 2026-05-08
archived: false
archived-at: null
created: 2026-05-08 14:52
updated: 2026-05-08 14:52
tags: [meeting, external]
```

Validation behavior:

- Warn, do not block, on unknown keys.
- Offer quick fixes for invalid enum values.
- Preserve key order where possible.
- Quote wikilinks in YAML when writing.
- Treat frontmatter as authoritative for routing.

### Task model

Supported statuses:

| Marker | Meaning | Active? | Terminal? |
|---|---|---:|---:|
| `[ ]` | To Do | yes | no |
| `[/]` | In Progress | yes | no |
| `[b]` | Blocked | no, separate blocked view | no |
| `[w]` | Awaiting Customer | no, separate awaiting view | no |
| `[h]` | On Hold | no, separate on-hold view | no |
| `[x]` | Done | no | yes |
| `[-]` | Cancelled | no | yes |

Parse and preserve:

```markdown
- [ ] Send updated SOW [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]] [owner:: me] 🔼 📅 2026-05-15
- [/] Drafting pricing model [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]] 🛫 2026-05-10 📅 2026-05-12
- [b] Blocked on Acme security review [customer:: [[Acme Corp]]] [stream:: [[SSO rollout]]]
- [w] Awaiting Acme legal redlines [customer:: [[Acme Corp]]] [owner:: customer] ⏳ 2026-05-15
- [h] On hold until next quarter [customer:: [[Globex]]]
- [x] Sent intro email ✅ 2026-05-07 [customer:: [[Acme Corp]]]
- [-] Cancelled — superseded by SOW v3 [customer:: [[Acme Corp]]]
```

Task edit operations must be line-preserving. Checking a task in the UI should rewrite only that task line, not the full note.

## 6. App UX design

### Primary screens

1. **Home**
   - Today’s daily note.
   - Top active tasks.
   - Blocked/awaiting counts.
   - Active customers.
   - Streams in progress.
   - Inbox count.

2. **Inbox Triage**
   - Unarchived inbox notes.
   - Unrouted tasks captured in daily/inbox notes.
   - One-click archive, assign customer, assign stream, convert to meeting, create stream.

3. **Customer workspace**
   - Customer index note.
   - Account info tabs.
   - Internal/external meetings.
   - Streams.
   - Open tasks and stale items.

4. **Stream workspace**
   - Stream note.
   - Stream status.
   - Related meetings.
   - Open tasks, blocked tasks, awaiting tasks.

5. **Tasks**
   - Active, Blocked, Awaiting Customer, On Hold, By Customer.
   - Each row links back to source note and stream.
   - Inline status changes rewrite source markdown.

6. **Daily notes/calendar**
   - Calendar navigation.
   - Daily note editor.
   - Archive daily note to `General/Journal/YYYY/MM/`.

7. **Agent console**
   - Shows active agent sessions.
   - Lists pending approvals, dry-run diffs, applied operations, undo links.

### Editor modes

- **Source mode:** direct markdown editing with CodeMirror, OFM highlighting, inline diagnostics.
- **Live preview mode:** markdown source plus rendered widgets for callouts, links, tasks, and query blocks.
- **Reading mode:** rendered note with editable task checkboxes and query results.

## 7. Agentic automation design

### Principles

- Agents operate on the vault, not on a hidden app state.
- Agents should prefer structured commands over raw text edits.
- Every mutating command supports `--dry-run` / `dryRun: true`.
- Multi-file operations produce unified diffs and undo manifests.
- Commands are idempotent when possible.
- Agents can query indexes without scraping rendered UI.
- Agent actions are auditable in a local operation log.

### Agent surfaces

| Surface | Purpose | Best for |
|---|---|---|
| Native CLI | Scriptable local automation | launchd, shell scripts, desktop launchers, CI validation. |
| URL scheme | External app integration | Raycast/Nimble/Shortcuts/calendar links. |
| MCP server | Tool-using LLM agents | Search, read, create, edit, archive, task operations. |
| ACP adapter | Editor-agent collaboration | Agents that expect a workspace/editor protocol. |
| Local JSON-RPC over stdio/socket | Stable internal protocol | Testing, future integrations, advanced automations. |
| File watcher | External edits | Detect changes from other editors/agents and refresh indexes. |

### Safety model

Operation modes:

1. **Read-only:** search/read/list/render only.
2. **Draft:** agent can create proposed files under `Inbox/` or `.notesapp/proposals/` if enabled.
3. **Apply with approval:** app displays diff before applying.
4. **Trusted automation:** selected tools can apply directly, still logged and undoable.

Each mutating command accepts:

```json
{
  "dryRun": true,
  "ifHash": "sha256-of-current-file",
  "conflictPolicy": "fail|merge|overwrite",
  "approvalMode": "none|required|defer",
  "actor": "cli|url|mcp|acp|ui|scheduler"
}
```

### Agent-optimized command categories

- Vault: `vault.open`, `vault.status`, `vault.validate`, `vault.health`.
- Notes: `note.read`, `note.write`, `note.patch`, `note.move`, `note.delete`, `note.open`.
- Metadata: `frontmatter.get`, `frontmatter.set`, `frontmatter.validate`.
- Tasks: `task.list`, `task.add`, `task.set_status`, `task.assign`, `task.reschedule`.
- Customers: `customer.list`, `customer.create`, `customer.set_state`, `customer.summary`.
- Streams: `stream.list`, `stream.create`, `stream.set_status`, `stream.summary`.
- Meetings: `meeting.create`, `meeting.list`, `meeting.link_stream`.
- Workflows: `daily.create`, `archive.route`, `inbox.triage`, `dashboard.render`.
- Search/query: `search.full_text`, `query.dataview`, `query.tasks`, `link.backlinks`.

## 8. URL scheme design

Register `notesapp://` for desktop integration. All paths are vault-relative unless otherwise stated. URL commands should parse into the same command bus as CLI/MCP.

### General rules

- Percent-encode paths and query values.
- `vault=` is optional when only one vault is configured.
- Mutating operations default to opening a confirmation UI unless `apply=true` is allowed by trust settings.
- `dryRun=true` returns preview UI.
- `x-callback-url` style callbacks are optional: `x-success=`, `x-error=`, `x-cancel=`.

### Open/read/navigation

```text
notesapp://open/Customers/Acme%20Corp/Acme%20Corp.md
notesapp://open?path=Customers/Acme%20Corp/Streams/Migration%20to%20v2.md
notesapp://reveal?path=Customers/Acme%20Corp
notesapp://search?q=security%20review&customer=Acme%20Corp
notesapp://dashboard/home
notesapp://dashboard/inbox
notesapp://dashboard/tasks?view=active
notesapp://customer/Acme%20Corp
notesapp://stream?customer=Acme%20Corp&stream=Migration%20to%20v2
notesapp://daily/today
notesapp://daily/2026-05-08
```

### Create operations

```text
notesapp://create?template=note&title=Follow%20up&customer=Acme%20Corp
notesapp://create?template=meeting&kind=external&customer=Acme%20Corp&topic=QBR&date=2026-05-08
notesapp://create?template=meeting&kind=internal&customer=Acme%20Corp&topic=Account%20strategy
notesapp://create?template=stream&customer=Acme%20Corp&name=Migration%20to%20v2&status=In%20Progress
notesapp://create?template=customer&customer=Globex&state=Temp
notesapp://create?template=daily&date=2026-05-08
```

### Task operations

```text
notesapp://task/add?text=Send%20updated%20SOW&customer=Acme%20Corp&stream=Migration%20to%20v2&due=2026-05-15
notesapp://task/list?status=blocked&customer=Acme%20Corp
notesapp://task/status?id=task_abc123&status=Awaiting%20Customer
notesapp://task/status?path=Customers/Acme%20Corp/Streams/Migration%20to%20v2.md&line=42&status=Done
notesapp://task/reschedule?id=task_abc123&due=2026-05-20
```

Task IDs should be stable derived IDs based on source path plus source range plus content hash. If line numbers drift, resolve by hash and nearby context.

### Workflow operations

```text
notesapp://archive?path=Inbox/Acme%20QBR.md
notesapp://archive/current
notesapp://inbox/triage
notesapp://customer/state?customer=Acme%20Corp&state=On%20Hold
notesapp://stream/status?customer=Acme%20Corp&stream=Migration%20to%20v2&status=Blocked
notesapp://lint?path=Inbox/Acme%20QBR.md
notesapp://vault/validate
```

### Agent operations

```text
notesapp://agent/session/new?mode=read-only
notesapp://agent/session/new?mode=apply-with-approval&task=Triage%20inbox
notesapp://agent/approve?operationId=op_20260508_145202_abc123
notesapp://agent/reject?operationId=op_20260508_145202_abc123
notesapp://agent/open-diff?operationId=op_20260508_145202_abc123
```

## 9. CLI design

Ship a native binary named `notesapp`. If the Tauri app binary cannot cleanly provide CLI subcommands, ship a sibling `notesapp-cli` that links the same Rust core.

### Global flags

```text
notesapp --vault /Users/surdy/Notes <command>
notesapp --json <command>
notesapp --dry-run <command>
notesapp --yes <command>
notesapp --actor cli <command>
```

### Commands

```text
notesapp vault init [--apply]
notesapp vault status
notesapp vault validate [--fix]
notesapp vault watch
notesapp vault reindex

notesapp open <path>
notesapp read <path> [--format markdown|html|json]
notesapp write <path> --stdin [--if-hash <hash>]
notesapp patch <path> --search <text> --replace <text> [--if-hash <hash>]
notesapp move <from> <to> [--update-links]
notesapp delete <path>

notesapp create note --title <title> [--customer <name>] [--stream <name>]
notesapp create customer --name <name> [--state Active|On Hold|Temp|Inactive]
notesapp create meeting --customer <name> --kind internal|external --topic <topic> [--date YYYY-MM-DD]
notesapp create stream --customer <name> --name <name> [--status <status>]
notesapp create daily [--date YYYY-MM-DD]

notesapp archive <path> [--apply]
notesapp archive current
notesapp inbox list [--json]
notesapp inbox triage [--interactive|--json]

notesapp task list [--status active|blocked|awaiting|on-hold|done|cancelled] [--customer <name>] [--stream <name>] [--json]
notesapp task add --text <text> [--customer <name>] [--stream <name>] [--due YYYY-MM-DD] [--to <path>]
notesapp task status --id <id> --status todo|in-progress|blocked|awaiting|on-hold|done|cancelled
notesapp task status --path <path> --line <n> --status done
notesapp task reschedule --id <id> --due YYYY-MM-DD

notesapp customer list [--state Active]
notesapp customer show <name> [--json]
notesapp customer set-state <name> <state>
notesapp customer create-missing-files <name>

notesapp stream list [--customer <name>] [--status <status>]
notesapp stream show --customer <name> --name <stream>
notesapp stream set-status --customer <name> --name <stream> --status <status>

notesapp query dataview --file <dashboard.md> [--block <n>] [--json]
notesapp query tasks --file <tasks.md> [--block <n>] [--json]
notesapp search <query> [--customer <name>] [--json]
notesapp links backlinks <path> [--json]

notesapp render <path> --to html
notesapp dashboard render home --json

notesapp mcp serve --transport stdio|http --port 37421
notesapp acp serve --transport stdio|socket
notesapp jsonrpc serve --stdio

notesapp undo list
notesapp undo apply <operationId>
```

### CLI examples

```bash
notesapp create meeting \
  --customer "Acme Corp" \
  --kind external \
  --topic "Security review" \
  --date 2026-05-08 \
  --json

notesapp task list --status active --customer "Acme Corp" --json

notesapp archive "Inbox/Daily/2026-05-08.md" --apply

notesapp query tasks --file "Tasks/Tasks - Active.md" --json
```

## 10. ACP / agent protocol design

Use ACP as the app's editor-agent protocol layer. The best current ecosystem fit is Zed's **Agent Client Protocol**, which standardizes communication between editors and coding agents; if the project later chooses a different **Agent Communication Protocol** variant, keep it behind the same adapter boundary. For this app, ACP should make the notes vault appear to agents as an editable workspace with domain-specific tools.

### ACP roles

- **App as ACP client/editor:** The notes app hosts the UI, displays files/diffs, and lets agents propose changes.
- **Agent as ACP server:** Existing agents connect and receive workspace context.
- **Bridge to command bus:** Domain operations are exposed as tool calls or editor actions.

### Capabilities to expose

Workspace capabilities:

- List vault files.
- Read file content with version hash.
- Apply text edits with conflict detection.
- Show diagnostics: invalid frontmatter, broken wikilinks, malformed tasks.
- Show diff/approval UI.

Domain capabilities:

- `notes.search`
- `notes.query`
- `notes.create_from_template`
- `notes.archive`
- `notes.task_list`
- `notes.task_update`
- `notes.customer_summary`
- `notes.stream_summary`

### Session lifecycle

1. User starts an agent session from the Agent Console or URL/CLI.
2. App sends vault context: folder structure, schema, task statuses, current note, selected text, open dashboard.
3. Agent uses read/query tools to inspect the vault.
4. Agent proposes command-bus operations or file edits.
5. App displays dry-run diffs for approval unless the session is trusted.
6. Applied operations are logged with operation ID and undo metadata.

### MCP alongside ACP

Implement MCP too. ACP is best for editor-like collaboration; MCP is best for external LLM tools that need structured note operations. Both should expose the same underlying commands. TurboVault's existing MCP server/tools can be used as a starting point, but the app should add customer-method-specific tools.

Recommended MCP tools:

```text
read_note
write_note
patch_note
move_note
search_notes
query_notes
list_tasks
add_task
update_task_status
create_customer
create_meeting
create_stream
archive_note
render_dashboard
validate_vault
get_backlinks
```

## 11. Template engine design

### Goals

Replace Templater and QuickAdd without allowing arbitrary JavaScript execution in notes.

### Template location

Keep templates in `Assets/templates/`:

```text
Assets/templates/
  T - Daily Note.md
  T - Internal Meeting.md
  T - External Meeting.md
  T - Customer Index.md
  T - Account Info.md
  T - Glossary.md
  T - Dates and Milestones.md
  T - Stream of Work.md
  T - Generic Note.md
```

### Template syntax

Use a safe Liquid/Handlebars-style syntax:

```markdown
---
type: meeting
meeting-kind: {{ meeting_kind }}
customer: "[[{{ customer }}]]"
stream: {{ wikilink_or_null(stream) }}
date: {{ date | date: "%Y-%m-%d" }}
created: {{ now | date: "%Y-%m-%d %H:%M" }}
updated: {{ now | date: "%Y-%m-%d %H:%M" }}
archived: false
tags: [meeting, {{ meeting_kind }}]
---

# {{ date }} - {{ customer }} - {{ meeting_kind | title }} - {{ topic }}

## Attendees

## Notes

## Tasks

- [ ]
```

### Built-in helpers

- `now`, `today`, `yesterday`, `tomorrow`.
- `date_add(date, days)`.
- `slug(value)`.
- `safe_filename(value)`.
- `wikilink(value)`.
- `wikilink_or_null(value)`.
- `customer_folder(customer)`.
- `meeting_filename(date, customer, kind, topic)`.
- `daily_path(date)`.
- `journal_path(date)`.

### Template manifest

Each template can have a sidecar manifest or frontmatter block declaring prompts:

```yaml
template-id: meeting-external
output:
  path: "Inbox/{{ meeting_filename(date, customer, 'External', topic) }}"
prompts:
  - key: customer
    type: customer
    required: true
  - key: topic
    type: text
    required: true
  - key: stream
    type: stream
    required: false
  - key: date
    type: date
    default: today
```

Agents can call `template.describe` to know required inputs, then `template.render` or `create_from_template`.

### Folder mappings

Native folder mappings replace Templater mappings:

| Folder pattern | Template |
|---|---|
| `Inbox/Daily` | `T - Daily Note.md` |
| `Inbox` | `T - Generic Note.md` |
| `Customers/*/External Meetings` | `T - External Meeting.md` |
| `Customers/*/Internal Meetings` | `T - Internal Meeting.md` |
| `Customers/*/Streams` | `T - Stream of Work.md` |
| `Customers/*/Account Info` | `T - Account Info.md` |

## 12. Query engine design

### Scope

Replace Dataview and Tasks for this method. Do not attempt to implement every DataviewJS feature in v1. Support the subset needed by the reviewed plan plus a clear compatibility story.

### Parsed data model

Every markdown file becomes:

```ts
type NoteRecord = {
  path: string
  basename: string
  title: string
  frontmatter: Record<string, unknown>
  inlineFields: Record<string, InlineValue[]>
  tags: string[]
  links: Link[]
  tasks: TaskRecord[]
  headings: Heading[]
  created?: Date
  updated?: Date
}

type TaskRecord = {
  id: string
  sourcePath: string
  line: number
  statusMarker: " " | "/" | "b" | "w" | "h" | "x" | "-"
  status: "To Do" | "In Progress" | "Blocked" | "Awaiting Customer" | "On Hold" | "Done" | "Cancelled"
  text: string
  customer?: Wikilink
  stream?: Wikilink
  owner?: string
  priority?: "highest" | "high" | "medium" | "low" | "lowest"
  start?: string
  scheduled?: string
  due?: string
  completed?: string
}
```

### Dataview-compatible subset

Support fenced blocks:

````markdown
```dataview
TABLE state, updated
FROM "Customers"
WHERE type = "customer" AND archived != true
SORT state ASC, file.name ASC
```
````

Supported v1 grammar:

- Query types: `TABLE`, `LIST`, `TASK`.
- Sources: `FROM "path"`, `FROM #tag`, `FROM [[link]]`.
- Filters: `WHERE` with `=`, `!=`, `<`, `>`, `<=`, `>=`, `contains`, `startswith`, `endswith`, boolean `AND`/`OR`/`NOT`.
- Sort: `SORT field ASC|DESC`.
- Group: `GROUP BY field`.
- Limit: `LIMIT n`.
- Fields: frontmatter keys, inline fields, `file.name`, `file.path`, `file.link`, `file.mtime`, `file.ctime`, `file.tags`, computed task fields.

Unsupported in v1:

- Arbitrary DataviewJS execution.
- Complex custom functions beyond a documented safe set.
- Mutating queries.

### Tasks-compatible subset

Support fenced blocks:

````markdown
```tasks
not done
status.type is TODO
path does not include Archive
sort by due
sort by priority
```
````

Add native status aliases:

```text
status is todo
status is in-progress
status is blocked
status is awaiting-customer
status is on-hold
status is done
status is cancelled
customer is [[Acme Corp]]
stream is [[Migration to v2]]
group by customer
group by stream
sort by due
sort by scheduled
sort by priority
limit 20
```

### Built-in dashboard queries

The app should ship the reviewed dashboards as source markdown plus native renderers:

- `Dashboards/Home.md`
- `Dashboards/Inbox Triage.md`
- `Dashboards/Customers.md`
- `Dashboards/Streams.md`
- `Tasks/Tasks - Active.md`
- `Tasks/Tasks - Blocked.md`
- `Tasks/Tasks - Awaiting Customer.md`
- `Tasks/Tasks - On Hold.md`
- `Tasks/Tasks - By Customer.md`

If a query block fails, show:

- error message,
- source range,
- suggested fix,
- option to copy JSON of parsed indexes for agent debugging.

## 13. Workflow details

### Archive note workflow

Input: current note or explicit path.

Algorithm:

1. Read note and parse frontmatter.
2. Validate `type`, `customer`, `meeting-kind`, `date`, `stream` where needed.
3. Compute destination:
   - `type: daily` -> `General/Journal/YYYY/MM/YYYY-MM-DD.md`.
   - `type: meeting`, `meeting-kind: internal` -> `Customers/<Customer>/Internal Meetings/<filename>.md`.
   - `type: meeting`, `meeting-kind: external` -> `Customers/<Customer>/External Meetings/<filename>.md`.
   - `type: stream` -> `Customers/<Customer>/Streams/<stream>.md`.
   - `type: account-info|glossary|milestones` -> `Customers/<Customer>/Account Info/<name>.md`.
   - `type: customer` -> `Customers/<Customer>/<Customer>.md`.
   - unknown -> ask user or move to `Archive/Inbox/` only if explicit.
4. Stamp `archived: true`, `archived-at: <now>`, `updated: <now>`.
5. Preflight destination collisions.
6. Apply atomic move/write batch.
7. Reindex changed files.
8. Return operation result and undo manifest.

### Daily note workflow

- Internal scheduler checks at app startup and while running.
- Optional OS integration installs a LaunchAgent that runs:

```bash
notesapp create daily --date today --vault /path/to/vault --yes
```

- Daily note starts in `Inbox/Daily/YYYY-MM-DD.md`.
- Same-day archive command moves it to `General/Journal/YYYY/MM/`.
- Template includes previous/today/next navigation.

### Customer state workflow

- Customer state lives only on `Customers/<Customer>/<Customer>.md` frontmatter key `state`.
- State changes update only that file.
- Sidebar and dashboards filter from the index.
- If `state: Inactive`, app can offer a separate `customer archive` operation that moves the whole folder to `Archive/Customers/` after confirmation.

### Stream workflow

- Streams live in `Customers/<Customer>/Streams/` for their entire lifecycle.
- Done streams stay in place and are filtered out of active dashboards.
- Stream status lives in stream note frontmatter `status`.
- Top open task is treated as implicit next action; no `next:` field.

## 14. Data integrity, sync, and conflict handling

### File integrity

- Store file hashes in memory for open buffers.
- Mutating commands require matching hash unless `conflictPolicy=overwrite`.
- Detect external edits and prompt to reload/merge.
- For task status updates, use source range plus content hash to relocate tasks after nearby edits.

### Undo

For each app-initiated operation, write a small local undo manifest under app config or `.notesapp/operations/` if the user permits app metadata in the vault:

```json
{
  "operationId": "op_20260508_145202_abc123",
  "actor": "mcp",
  "timestamp": "2026-05-08T14:52:02-07:00",
  "changes": [
    { "type": "move", "from": "Inbox/X.md", "to": "Customers/Acme Corp/X.md" },
    { "type": "write", "path": "Customers/Acme Corp/X.md", "beforeHash": "...", "afterHash": "..." }
  ]
}
```

Do not require these manifests for vault readability. They are app metadata only.

### No-database policy

Allowed:

- In-memory indexes.
- Rebuildable JSON cache files if the user opts in.
- Operation logs/undo manifests as plain JSON files.
- App preferences as plain JSON/TOML.

Not allowed:

- Required SQLite/Postgres/etc. store.
- Hidden canonical task database.
- Metadata that must be present outside markdown for the vault to work.

## 15. Configuration files

Keep app configuration outside the notes when possible, but support a vault-local config for portable behavior.

Recommended vault-local config: `.notesapp/config.toml`.

```toml
[vault]
schema_version = 1
startup_note = "Dashboards/Home.md"
default_new_note_folder = "Inbox"
attachment_folder = "Assets/data"

[tasks]
active_statuses = [" ", "/"]
blocked_status = "b"
awaiting_status = "w"
on_hold_status = "h"
done_status = "x"
cancelled_status = "-"

[folders]
inbox = "Inbox"
daily_inbox = "Inbox/Daily"
journal = "General/Journal"
customers = "Customers"
tasks = "Tasks"
dashboards = "Dashboards"
archive = "Archive"
templates = "Assets/templates"

[hotkeys]
home = "Cmd+1"
active_tasks = "Cmd+2"
inbox = "Cmd+3"
```

The app should work without this file by using the reviewed-plan defaults.

## 16. Implementation phases

### Phase 0: Technical spike and decisions

Deliverables:

- Build a tiny Tauri shell that opens a vault and displays markdown source.
- Evaluate TurboVault crates for parsing, vault operations, graph, and batch writes.
- Confirm license compatibility and API stability.
- Decide whether TurboVault can be the primary core or should be used only as reference/optional integration.
- Prototype parsing of the provided task examples.
- Prototype one MCP tool: `list_tasks`.

Exit criteria:

- Can parse frontmatter, wikilinks, inline fields, and custom task statuses from a sample vault.
- Can safely write one note atomically.
- Can run the app and CLI against the same vault.

### Phase 1: Vault core and parser

Deliverables:

- `VaultEngine` trait and implementation.
- Path safety and atomic write/move/delete.
- File watcher and full/incremental reindex.
- `MarkdownEngine` parser for frontmatter, links, tasks, inline fields, tags, callouts.
- Basic source editor.
- JSON command bus.

Exit criteria:

- `notesapp vault validate` reports schema issues.
- `notesapp task list --json` returns tasks from the vault.
- UI opens notes and displays parsed metadata.

### Phase 2: Templates and create workflows

Deliverables:

- Safe template engine.
- Built-in template prompts.
- Create daily/customer/meeting/stream commands.
- Command palette forms.
- URL `create` routes.

Exit criteria:

- Create each reviewed note type from UI, CLI, and URL.
- Generated files are Obsidian-readable and match the schema.

### Phase 3: Tasks and dashboards

Deliverables:

- Task status editor.
- Active/Blocked/Awaiting/On Hold/By Customer task views.
- Query engine subset for needed Dataview/Tasks blocks.
- Home, Inbox Triage, Customers, Streams dashboards.

Exit criteria:

- Native dashboards replace the reviewed Dataview dashboards.
- Task checkbox/status changes rewrite source markdown correctly.

### Phase 4: Archive/router and daily automation

Deliverables:

- Archive command with dry-run and apply.
- Collision handling.
- Undo manifests.
- Daily note scheduler and optional LaunchAgent installer.
- Customer state and stream status workflows.

Exit criteria:

- Inbox-to-destination routing matches the reviewed plan.
- Daily notes are created at 06:30 through OS automation or app scheduler.
- Inbox zero workflow works from UI, hotkey, CLI, and URL.

### Phase 5: Agent integrations

Deliverables:

- MCP server with note/search/task/customer/stream/archive tools.
- ACP adapter with workspace/file/diff support.
- Agent Console UI.
- Approval and trusted-session policies.
- Operation logs.

Exit criteria:

- An MCP agent can list tasks, create a meeting, archive a note, and render a dashboard.
- An ACP agent can inspect the vault, propose edits, and apply approved changes.
- All agent changes are auditable and undoable.

### Phase 6: Polish, packaging, and migration

Deliverables:

- macOS signed/notarized build.
- Import/open existing vault without migration.
- Settings UI.
- Keyboard shortcuts.
- Performance profiling on large vaults.
- Documentation for URL scheme, CLI, MCP, and ACP.

Exit criteria:

- Existing reviewed-plan vault opens without destructive changes.
- Obsidian can still open the vault after notesapp edits.
- All critical workflows pass Playwright end-to-end tests.

## 17. Testing strategy

### Golden vault tests

Maintain fixture vaults with:

- All note types.
- Valid and invalid frontmatter.
- Every task status.
- Wikilinks with spaces, aliases, and missing targets.
- Meeting notes with/without streams.
- Daily notes pre/post archive.
- Query blocks for each dashboard.

### Parser tests

- Round-trip frontmatter updates.
- Inline field extraction.
- Task date/priority parsing.
- Callout and wikilink rendering.
- Source range stability.

### Workflow tests

- Create meeting.
- Create stream.
- Archive each note type.
- Change customer state.
- Change stream status.
- Update task status by ID and by path/line.
- Undo archive.

### Agent tests

- MCP tool schema snapshots.
- ACP session fixtures.
- Dry-run diff approval flow.
- Conflict detection when file hash changes.

### Compatibility tests

- Ensure generated markdown remains readable in Obsidian.
- Ensure Dataview/Tasks code fences remain syntactically intact even if the app renders them natively.
- Ensure app ignores unsupported Obsidian plugin metadata rather than deleting it.

## 18. Performance targets

Initial targets for a realistic customer vault:

- App launch with 5,000 notes: under 2 seconds to interactive with progressive indexing.
- Full reindex 5,000 notes: under 10 seconds.
- Incremental update after one file save: under 100 ms for indexes, under 250 ms for dashboard refresh.
- Task list render 10,000 tasks: under 500 ms with virtualization.
- Search query: under 150 ms for common terms after index is warm.

If these are missed, add Tantivy or another Rust search index as a rebuildable cache, not as canonical storage.

## 19. Risks and mitigations

| Risk | Mitigation |
|---|---|
| TurboVault API/maturity risk | Wrap behind `VaultEngine`; spike early; keep fallback parser path. |
| Full Dataview compatibility is too large | Support the subset required by this method; keep source blocks unchanged; document unsupported syntax. |
| Arbitrary JS templates create security problems | Use safe template engine with whitelisted helpers; no JS execution in v1. |
| Agents corrupt files | Use dry-run, hashes, approvals, atomic batches, operation logs, undo. |
| File watcher race conditions | Debounce, hash files, and re-read before applying mutations. |
| Obsidian compatibility drift | Golden tests open/parse generated notes with OFM fixtures; never store canonical data outside markdown. |
| URL scheme abuse | Require user trust settings for mutating URL commands; default to confirmation. |
| Query performance | Derived indexes, incremental updates, virtualization; optional rebuildable search cache. |

## 20. Open product decisions

Recommended defaults are included so implementation can proceed without blocking.

| Decision | Recommendation |
|---|---|
| App name | Use placeholder `notesapp` until branding is needed. |
| Persistent cache | Start with in-memory only; add optional `.notesapp/cache/` JSON/Tantivy cache later. |
| Template syntax | Use Liquid-style safe templates. |
| Agent approval default | Read-only by default; mutating MCP/ACP actions require approval until trusted. |
| DataviewJS | Do not support in v1. Add explicit native commands instead. |
| Multi-vault | Support configured vault list, but optimize UX for one primary work vault. |
| Mobile | Defer. Preserve plain markdown so other mobile editors can read files. |

## 21. First build milestone

The first useful private alpha should do only this, end to end:

1. Open the reviewed-plan vault.
2. Parse notes, customer states, stream statuses, and all task statuses.
3. Show Home, Inbox Triage, and task dashboards.
4. Create daily, meeting, stream, and customer notes from templates.
5. Archive a note from Inbox with dry-run and undo.
6. Expose the same operations through CLI and URL scheme.
7. Provide a minimal MCP server with `search_notes`, `read_note`, `list_tasks`, `create_meeting`, and `archive_note`.

If that works, the app has already replaced the critical Obsidian plugin chain for the reviewed workflow.

## 22. Source links checked during planning

- TurboVault README: `https://github.com/Epistates/turbovault`
- TurboVault crates: `turbovault`, `turbovault-parser`, `turbovault-vault`, `turbovault-batch` on crates.io
- Tauri packages: `@tauri-apps/api`, `@tauri-apps/cli`
- CodeMirror packages: `@codemirror/state`, `@codemirror/view`, `@codemirror/lang-markdown`, `@lezer/markdown`
- Markdown packages: `micromark`, `remark`, `remark-gfm`, `markdown-it`, `pulldown-cmark`
- Template packages: `liquidjs`, `handlebars`, `eta`
- Search packages: `MiniSearch`, `FlexSearch`, `tantivy`
- Protocol packages: `@modelcontextprotocol/sdk`, `@agentclientprotocol/sdk`, ACP README at `https://github.com/zed-industries/agent-client-protocol`
