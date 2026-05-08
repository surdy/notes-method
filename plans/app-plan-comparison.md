# App Plan Comparison & Recommendations

Three AI models independently produced plans for a custom markdown notes app to replace Obsidian, based on the same reviewed plan. This document compares them and recommends a synthesis.

---

## 1. Plans at a glance

| Dimension | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Working name** | `notesapp` | `notesapp` | **Notesmith** |
| **Core language** | Rust | Rust | Rust |
| **Desktop shell** | Tauri 2 | Tauri 2.x | Tauri v2 |
| **UI framework** | React 19 + Vite | **SolidJS** | **SvelteKit** |
| **Editor** | CodeMirror 6 | CodeMirror 6 | CodeMirror 6 |
| **Markdown parser (Rust)** | pulldown-cmark + TurboVault | **markdown-rs** (wooorm) | **comrak** (GFM) |
| **Markdown render (JS)** | — (implied CodeMirror) | unified/remark pipeline | — (CodeMirror decorations) |
| **Query backing store** | In-memory indexes (no DB) | **DuckDB-WASM** (frontend SQL) | **SQLite** (Rust cache, rebuildable) |
| **Full-text search** | In-memory / MiniSearch | FlexSearch (JS) + tantivy (Rust) | **Tantivy** (Rust) |
| **Template engine** | Liquid-like (safe) | **Eta** (EJS-like) | **minijinja** (Jinja2, Rust) |
| **Agent surfaces** | CLI, URL, MCP, ACP, JSON-RPC | CLI, URL, MCP, REST API (4 surfaces) | CLI, URL, ACP, MCP, JSON-RPC/socket, HTTP (6 surfaces) |
| **Primary RPC protocol** | JSON command bus | REST API (Axum, `localhost:27183`) | **JSON-RPC 2.0 over Unix socket** |
| **Daemon** | Not explicit | Not explicit (REST from GUI) | **`notesmithd`** (explicit daemon) |
| **TurboVault usage** | Evaluate first, wrap behind trait | Not mentioned | Fork/depend on core crates |
| **Config format** | `.notesapp/config.toml` | `.notesapp/config.yaml` | `notesmithrc.toml` |
| **Phases** | 7 phases (spike → polish) | 7 phases (20 weeks) | **11 phases** (14–18 weeks) |
| **Estimated timeline** | Not explicit | ~20 weeks | ~14–18 weeks |

---

## 2. Detailed comparison by dimension

### 2.1 Architecture

| Aspect | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Central abstraction** | `CommandBus` — every surface routes through one typed command API | `VaultOps` trait — 4 integration surfaces backed by shared Rust ops | **JSON-RPC daemon** — GUI, CLI, ACP, MCP, URL all speak JSON-RPC to a Unix socket daemon |
| **Crate structure** | Monolithic `src-tauri/core/` with modules | Single `src-tauri/` with nested Rust modules | **Workspace of 9 crates** (`notesmith-core`, `notesmith-templates`, `notesmith-rules`, `notesmith-rpc`, `notesmith-acp`, `notesmith-mcp`, `notesmith-cli`, `notesmithd`, `notesmith-tauri`) |
| **Shared TS/Rust types** | Not mentioned | Not mentioned | **`vault-schema/`** codegen crate |
| **CLI binary** | Same Tauri binary or sibling `notesapp-cli` | `notesapp` with standalone + connected modes | `notesmith` (separate binary, ~12MB) |

**Verdict:** Opus 4.7's daemon architecture (`notesmithd`) is the strongest. It cleanly separates the long-running process (indexing, watching, scheduling) from one-shot CLI calls and the GUI. GPT 5.5's CommandBus pattern is good but less precisely specified. Opus 4.6's REST-first approach is simpler but tighter-coupled to the GUI process.

### 2.2 UI framework

| | GPT 5.5 (React) | Opus 4.6 (SolidJS) | Opus 4.7 (SvelteKit) |
|---|---|---|---|
| **Ecosystem** | Largest | Smallest | Medium |
| **Performance** | Virtual DOM overhead | No VDOM, fine-grained reactivity | No VDOM, compiled |
| **Bundle size** | Largest | Smallest | Small |
| **Agent-authoring friendliness** | Most AI training data | Less training data | Good training data |
| **Dev productivity** | Highest (ecosystem) | Steeper learning curve | High (concise syntax) |

**Verdict:** React is the safe choice for rapid iteration and agent-authored code. SolidJS and Svelte are technically faster but have smaller ecosystems. For a single-developer project where agents will help write code, React's ecosystem advantage matters. However, Svelte's simplicity is compelling for a desktop app.

### 2.3 Query engine

| | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Approach** | In-memory indexes, no DB | DuckDB-WASM (frontend SQL) | SQLite (Rust, rebuildable cache) |
| **DQL support** | Subset for the method | DQL → SQL translation | **NDQL** (Dataview-flavored) + Tasks DSL + raw SQL |
| **DataviewJS** | Not in v1 | Sandboxed `dv.*` API | Not in v1 |
| **No-database policy** | Strict (in-memory only) | DuckDB-WASM is in-process | SQLite is a file, but rebuildable = cache |

**Verdict:** Opus 4.7's SQLite approach is the pragmatic winner. It's fast, battle-tested in Rust (rusqlite), and `rebuildable cache != database`. DuckDB-WASM from Opus 4.6 is powerful but adds JS complexity. GPT 5.5's pure in-memory approach is cleanest conceptually but may struggle with complex queries.

### 2.4 Template engine

| | GPT 5.5 (Liquid-like) | Opus 4.6 (Eta) | Opus 4.7 (minijinja) |
|---|---|---|---|
| **Language** | JS (or Rust `liquid`) | JS (TypeScript) | **Rust** |
| **Safety** | No arbitrary JS | Full JS in templates (`await`) | Sandboxed by default |
| **Syntax familiarity** | `{{ var }}` | `<%= var %>` (EJS/Templater-like) | `{{ var }}` (Jinja2) |
| **CLI usability** | Needs JS runtime | Needs JS runtime | **Native Rust binary** |
| **User escape hatch** | Whitelisted helpers | User scripts (JS modules) | Hook scripts (subprocesses) |

**Verdict:** Opus 4.7's minijinja is the best fit. It runs in pure Rust (no JS runtime needed for CLI), is sandboxed, and Jinja2 is widely known. The hook-script escape hatch (subprocess model) is cleanest for security. Opus 4.6's Eta is closest to Templater syntax which aids migration, but requires a JS runtime even in headless mode.

### 2.5 Agent integration

| | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **MCP** | ✅ MCP server (stdio) | ✅ MCP server (stdio + HTTP SSE) | ✅ MCP server (optional, wraps JSON-RPC) |
| **ACP** | ✅ App as ACP client/editor | ✅ (mentioned but less detailed) | ✅ **ACP as primary agent surface** (inverted: app is "editor" agents drive) |
| **CLI** | ✅ 30+ commands | ✅ 40+ commands | ✅ Full command tree |
| **REST API** | Not explicit (JSON-RPC internal) | ✅ `localhost:27183` (Axum) | ✅ Opt-in HTTP gateway |
| **URL scheme** | ✅ Comprehensive | ✅ With x-callback-url | ✅ Comprehensive |
| **Event subscriptions** | Not mentioned | ✅ SSE event stream | ✅ **JSON-RPC subscriptions** (killer feature) |
| **Agent permissions** | Approval modes (read-only, draft, apply, trusted) | Token-authenticated | ✅ **Capability-scoped YAML agent configs** |
| **Dry-run** | ✅ All mutations | ✅ All mutations | ✅ All mutations |
| **Audit log** | ✅ Operation log | ✅ (mentioned) | ✅ JSON-lines audit log |
| **Idempotency keys** | Not mentioned | Not mentioned | ✅ |

**Verdict:** Opus 4.7 is the most thoroughly designed for agentic use. The capability-scoped agent tokens, subscriptions/event streams, idempotency keys, and the daemon architecture make it truly agent-first. GPT 5.5 has the best approval-mode design (4 tiers). Opus 4.6 has the cleanest REST API for simple HTTP integrations.

### 2.6 URL scheme

All three plans use `notesapp://` and cover the same core operations (open, create, search, archive, daily, task operations). Key differentiators:

| Feature | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Agent session URLs** | ✅ `notesapp://agent/session/new` | ❌ | ❌ |
| **Generic RPC tunnel** | ❌ | ❌ | ✅ `notesapp://rpc?method=...` |
| **Shorthand paths** | ❌ | ✅ `notesapp:///path` | ✅ `notesapp://open/path` |
| **x-callback-url** | Mentioned | ✅ Explicit support | Not mentioned |
| **Auth tokens** | Confirmation UI | ✅ Required for writes | Signed URLs (off by default) |

**Verdict:** Merge the best: Opus 4.6's shorthand syntax + x-callback-url, GPT 5.5's agent session URLs, Opus 4.7's RPC tunnel escape hatch.

### 2.7 Routing/archive engine

| | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Rules format** | Hard-coded in WorkflowEngine | `.notesapp/router-rules.yaml` | **`Assets/rules/routing.yaml`** (declarative YAML with Jinja expressions) |
| **Hooks** | Not mentioned | Not mentioned | ✅ Pre/post hooks (subprocess) |
| **Bulk archive** | Not mentioned | ✅ `notesapp note list | xargs` | ✅ `notesmith route apply --inbox` |

**Verdict:** Opus 4.7's declarative YAML routing rules with hooks are the most flexible and inspectable.

### 2.8 Daily note scheduler

| | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Approach** | Internal scheduler + optional LaunchAgent | Auto-create on startup + launchd CLI | **Built into `notesmithd`** with catch-up |
| **Catch-up** | Not mentioned | Not mentioned | ✅ Generates missed days |

**Verdict:** Opus 4.7's daemon-integrated scheduler with catch-up is the most robust.

### 2.9 Testing strategy

| | GPT 5.5 | Opus 4.6 | Opus 4.7 |
|---|---|---|---|
| **Golden vault** | ✅ Fixture vault | ✅ With specific test categories | ✅ With snapshot + property tests |
| **Round-trip tests** | ✅ | ✅ | ✅ (explicit OFM parse→serialize fixed-point) |
| **Agent tests** | ✅ MCP schema + ACP session | ✅ MCP schema snapshots | ✅ |
| **Performance targets** | ✅ (5k notes < 2s, incremental < 100ms) | ✅ (1k notes < 2s index) | ✅ (10k notes < 5s, incremental < 50ms, startup < 500ms) |

---

## 3. What each plan does best

### GPT 5.5 strengths
1. **Most thorough safety model** — 4-tier approval modes (read-only → draft → apply-with-approval → trusted automation), explicit `conflictPolicy`, `ifHash`, and `approvalMode` per mutation.
2. **Best agent session concept** — Agent Console UI with pending approvals, dry-run diffs, applied operations, and undo links.
3. **Most conservative no-database stance** — strictly in-memory indexes, no SQLite.
4. **Clearest UX screen design** — 7 screens described (Home, Inbox Triage, Customer workspace, Stream workspace, Tasks, Daily/calendar, Agent console).
5. **Editor modes** — Source, live preview, and reading mode clearly distinguished.

### Opus 4.6 strengths
1. **Most actionable implementation plan** — 7 phases with specific week ranges and task tables with deliverables per subtask.
2. **Best library gap analysis** — explicitly cataloged what exists vs. must be built (with line-count estimates for custom extensions).
3. **Cleanest REST API design** — full HTTP endpoint table with SSE events.
4. **DQL → SQL translation** is most explicitly specified with examples.
5. **Success criteria** — 10 concrete checkpoints for "ready for daily use."

### Opus 4.7 strengths
1. **Strongest architecture** — daemon-based, 9-crate workspace, JSON-RPC as the single RPC surface, all surfaces are thin adapters.
2. **Best agentic design** — capability-scoped tokens, event subscriptions, idempotency keys, audit log, agent permission YAML configs.
3. **Most opinionated and coherent** — clear design principles, explicit rejection list, every choice justified.
4. **Routing engine** — declarative YAML rules with Jinja expressions and hooks.
5. **Template escape hatch** — hook scripts (subprocess) instead of embedded JS. Cleanest security model.
6. **Best Obsidian compatibility story** — vault stays openable in Obsidian, `dataview` alias, `.notesmith/` directory separate from `.obsidian/`.

---

## 4. Recommendations

### 4.1 Recommended synthesis

Take **Opus 4.7 as the base plan** and incorporate specific strengths from the other two:

| Decision | Source | Choice |
|---|---|---|
| **Architecture** | Opus 4.7 | Daemon (`notesmithd`) + CLI + GUI all over JSON-RPC 2.0 on Unix socket |
| **App name** | Opus 4.7 | **Notesmith** (good name — evocative, unique, memorable) |
| **UI framework** | GPT 5.5 or Opus 4.7 | **SvelteKit** (Opus 4.7's pick). React (GPT 5.5) is the fallback if Svelte expertise is lacking. Both are reasonable. |
| **Query engine** | Opus 4.7 | **SQLite cache** (rebuildable, fast, `rusqlite` is rock-solid) |
| **Full-text search** | Opus 4.7 | **Tantivy** (Rust-native, avoids JS dependency for search) |
| **Template engine** | Opus 4.7 | **minijinja** + hook scripts (sandboxed, pure Rust, no JS runtime needed) |
| **Markdown parser** | Opus 4.7 | **comrak** + custom OFM extensions |
| **Agent approval model** | GPT 5.5 | 4-tier approval modes (read-only, draft, apply-with-approval, trusted) |
| **Agent permissions** | Opus 4.7 | Capability-scoped YAML agent configs |
| **Event subscriptions** | Opus 4.7 | JSON-RPC subscriptions (agents watch for `inbox.new`, `task.completed`, etc.) |
| **REST API** | Opus 4.6 | Keep as opt-in HTTP gateway for simple HTTP clients |
| **MCP server** | All three | Thin wrapper over JSON-RPC surface. stdio primary, HTTP SSE secondary |
| **ACP server** | Opus 4.7 | App as "editor" that agents drive via ACP |
| **URL scheme** | Merge | Opus 4.6 shorthand + x-callback-url + GPT 5.5 agent session URLs + Opus 4.7 RPC tunnel |
| **Routing engine** | Opus 4.7 | Declarative YAML rules with Jinja expressions and hooks |
| **Daily scheduler** | Opus 4.7 | Built into daemon with catch-up |
| **Implementation phases** | Opus 4.7 (structure) + Opus 4.6 (detail) | 11 phases from Opus 4.7 with Opus 4.6's task-level detail and explicit deliverables |
| **Testing** | All three | Golden vault fixtures + property tests + agent test fixtures |
| **Config format** | Opus 4.7 | TOML (`notesmithrc.toml`) — more conventional for Rust CLI tools |
| **Agent console UI** | GPT 5.5 | Include in GUI: shows sessions, pending approvals, diffs, undo |
| **DataviewJS** | Opus 4.7 | Skip in v1. Hooks + raw SQL cover 90% of use cases. |
| **No-database policy** | Opus 4.7 | SQLite is allowed as a *rebuildable cache*, not a canonical store. Delete it and rebuild from vault at any time. |

### 4.2 Key risk: TurboVault dependency

GPT 5.5 and Opus 4.7 both recommend TurboVault. Opus 4.6 doesn't mention it. The recommendation:
- **Spike TurboVault first** (Phase 0). Evaluate its parser, vault I/O, and atomic batch operations.
- **Wrap behind a `VaultEngine` trait** (GPT 5.5's advice) so TurboVault can be swapped out if it's too immature or its API changes.
- If TurboVault covers 60%+ of vault I/O needs, depend on it. Otherwise, build on comrak + custom extensions.

### 4.3 Key risk: DQL/Tasks parser complexity

All three plans acknowledge this. The recommendation:
- Build only the DQL/Tasks subset needed by the reviewed plan's dashboards.
- Offer raw SQL as the power-user escape hatch.
- Document unsupported syntax clearly.
- Keep the source code blocks unchanged in markdown so Obsidian can still render them.

### 4.4 What to skip in v1

All three agree on these deferrals:
- ❌ Mobile (use Obsidian Mobile with same vault)
- ❌ Plugin system
- ❌ DataviewJS (full arbitrary JS)
- ❌ Multi-user / real-time collaboration
- ❌ Encryption at rest
- ❌ AI features baked in (agents provide AI via MCP/ACP)
- ❌ Graph view (nice-to-have, not critical for the workflow)

### 4.5 What the synthesis adds that no single plan covers

1. **CLI `notesmith` binary that works without the GUI** and without a daemon — direct file access for simple operations, daemon connection for indexed queries. (Opus 4.7 comes closest but ties more operations to the daemon.)
2. **`notesapp://agent/session/new?mode=apply-with-approval`** URL scheme for launching agent sessions from launchers/Raycast. (Only GPT 5.5 had this.)
3. **Explicit REST API alongside JSON-RPC** for HTTP-only consumers. (Only Opus 4.6 detailed this.)
4. **Agent Console in the GUI** showing all connected agents, pending approvals, and undo history. (Only GPT 5.5 described this as a screen.)

---

## 5. Recommended implementation order (synthesized)

| Phase | Scope | Weeks |
|---|---|---|
| **0 — Spike** | TurboVault evaluation, Tauri scaffold, parse sample vault, prototype one MCP tool | 1–2 |
| **1 — Read-only core** | Vault parser, SQLite cache, file watcher, `notesmith vault status`, `note read` | 1–2 |
| **2 — Tasks & queries** | Task engine (7 statuses), NDQL parser, Tasks DSL, `notesmith task list`, `notesmith query` | 1–2 |
| **3 — Templates** | minijinja, prompt schema, all 9 templates, `notesmith template apply` | 1 |
| **4 — Routing** | Declarative rules engine, `route preview/apply`, `route apply --inbox` | 1 |
| **5 — Daemon & RPC** | `notesmithd` (Unix socket), subscriptions, agent tokens, daily scheduler | 1–2 |
| **6 — CLI maturity** | Full `notesmith` command tree, standalone + connected modes, pipe-friendly JSON output | 1 |
| **7 — GUI shell** | Tauri + Svelte/React, file tree, CodeMirror editor, OFM extensions, palette, quick-switcher | 2–3 |
| **8 — Live queries & dashboards** | Query block rendering, callouts, embeds, wikilink navigation, backlinks panel | 2 |
| **9 — Agent integration** | MCP server, ACP server, URL scheme, REST gateway, Agent Console UI | 1–2 |
| **10 — Workflow polish** | Bookmarks, hotkeys, homepage, linter hooks, git auto-commit | 1–2 |
| **11 — Migration & hardening** | Obsidian vault import, compatibility report, golden vault tests, performance tuning | 1 |

**Total: ~14–20 weeks** for a single full-time developer to reach v1 dogfood-ready.

---

## 6. Final recommendation

**Use the Opus 4.7 plan as the primary blueprint.** It has the strongest architecture, the most coherent agentic design, and the clearest separation of concerns. Supplement it with:

- GPT 5.5's **safety/approval model** and **Agent Console UI**
- Opus 4.6's **task-level implementation detail**, **REST API design**, and **library gap analysis**
- Opus 4.6's **x-callback-url** support in the URL scheme

The result is a focused, agent-first notes app that replaces the 10+ Obsidian plugins with a single native binary, keeps the vault fully Obsidian-compatible, and makes every operation available to both humans and AI agents through a unified RPC surface.
