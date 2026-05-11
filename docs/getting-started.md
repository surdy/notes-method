# Getting Started with Notesmith

Notesmith is a markdown notes app for agentic workflows. It keeps notes as plain markdown files on disk, stays compatible with Obsidian Flavored Markdown (OFM), and adds built-in task management, routing, templates, search, SQL views, and a desktop app.

Use this guide to go from a fresh clone to a working vault quickly.

## What is Notesmith?

Notesmith is built around a few simple principles:

- Plain markdown files are the source of truth — no proprietary database
- The SQLite cache is rebuilt from files and is never authoritative
- Agent-first interfaces are built in: CLI, MCP adapter, and HTTP API
- It is also designed for daily manual use — the desktop app is the primary interface

If you want portable notes with first-class automation, Notesmith is the core idea.

## Prerequisites

Before you start, install:

- Rust 1.85+ (edition 2024)
- Node.js 22+
- pnpm 10+
- macOS

Linux should work, but macOS is the primary platform right now.

## Installation

Notesmith is currently build-from-source only.

### Build the CLI and daemon

```bash
git clone https://github.com/surdy/notes-method.git
cd notes-method
cargo build --release --workspace
# Binary: target/release/notesmith
```

### Build the desktop app (macOS)

```bash
cd ui/app && pnpm install && pnpm build
cd ../../crates/notesmith-tauri
cargo tauri build
# App bundle: target/release/bundle/dmg/
```

If you only want the CLI, the first build step is enough.

## Setting Up Your First Vault

A vault is just a folder of markdown files plus a small amount of Notesmith configuration.

### 1. Register a vault

Notesmith keeps the global vault registry in `~/.config/notesmith/config.toml`.

Create that file with your vault name and path:

```toml
default_vault = "work"

[vaults.work]
path = "/Users/you/Notes/work"
```

This tells Notesmith where your vault lives and which vault to use by default.

### 2. Initialize vault config

Inside your vault root, create `.notesmith/vault.toml`:

```toml
name = "work"

[inbox]
folder = "Inbox"

[daily]
folder = "Inbox/Daily"
template = "daily-note"
```

This is enough to get inbox capture and daily notes working.

### 3. Start the daemon

```bash
notesmith daemon start
```

The daemon runs on `127.0.0.1:27183` by default and provides:

- the HTTP API
- SSE events
- the browser app shell
- the backend used by most CLI workflows

### 4. Open the app

You can use Notesmith in three ways:

- **Desktop app:** launch `Notesmith.app`
- **Browser app:** open `http://127.0.0.1:27183/app/`
- **URL scheme:** `notesmith://app/open/work/Inbox/hello.md`

If you are starting out, the browser app is the fastest way to confirm the daemon and vault are working.

## Your First Notes

The basic Notesmith loop is:

1. capture into the inbox
2. work in markdown
3. route notes later

| Action | In the app | In the CLI |
|--------|------------|------------|
| Create a note | `⌘N` → enter a title → choose a folder | `notesmith note create "My First Note" --folder Inbox` |
| Quick capture to inbox | `⌘⇧I` → enter the note text | `notesmith inbox add "Remember to call Sarah"` |
| Create today's daily note | `⌘D` | `notesmith daily ensure` |
| Search notes | `⌘O` → type your query | `notesmith search "meeting notes"` |

Once the daemon is running, search covers note titles and note bodies.

## Key Features

### Task Management

Notes can contain tasks with seven statuses:

```markdown
- [ ] Todo item
- [/] In progress
- [x] Done
- [b] Blocked
- [w] Waiting
- [h] On hold
- [-] Cancelled
```

Tasks support inline fields too:

```markdown
- [ ] Follow up on renewal [customer:: Acme] [due:: 2025-06-15] [owner:: me]
```

That keeps tasks readable in markdown while still making them queryable.

### Inbox Routing

Notes in the inbox can be automatically filed to their destination based on frontmatter:

```bash
notesmith route apply --inbox
```

Rules live in `.notesmith/routing.yaml`.

### Templates

Templates let you create notes from reusable structures with prompted fields:

```bash
notesmith template instantiate external-meeting --prompt customer=Acme --prompt date=2025-06-15
```

Use templates for daily notes, meetings, customer records, and repeated workflows.

### SQL Queries

Notesmith lets you query your notes like a database:

```bash
notesmith query sql "SELECT title, status FROM v_tasks WHERE status = 'todo'"
```

This is useful for dashboards, reporting, and agent automation.

### View Modes

The editor supports three modes. Cycle them with `⌘E`:

- **Source**: raw markdown
- **Live Preview**: inline rendering with cursor-line editing
- **Reading View**: fully rendered HTML with interactive checkboxes

This makes it easy to move between precise editing and polished reading.

## Documentation

Use these docs as you go deeper:

| Document | Description |
|----------|-------------|
| [Desktop App Guide](app-guide.md) | Full guide to the desktop app interface |
| [View Modes](view-modes.md) | Source, Live Preview, and Reading View details |
| [Keyboard Shortcuts](keyboard-shortcuts.md) | All keyboard shortcuts |
| [CLI Reference](cli.md) | Command-line interface reference |
| [HTTP API](http-api.md) | REST API endpoints |
| [SQL Views](sql-views.md) | Queryable SQL views |
| [MCP Adapter](mcp.md) | Model Context Protocol tools |
| [Vault Configuration](vault-configuration.md) | Configuration files and options |

## Philosophy

Notesmith follows a practical method:

- **Inbox** is the entry point for all notes — capture first, organize later
- **Routing rules** automate filing notes to the right folder based on frontmatter
- **Templates** keep recurring note types consistent
- **SQL views** make notes queryable as structured data
- **Agents** can create, organize, and query notes through the CLI, MCP adapter, or HTTP API

For the larger notes methodology, see [notes-method.md](../notes-method.md).
