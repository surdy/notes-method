# Notesmith

A generic, programmable markdown workspace built for agentic workflows, compatible with Obsidian Flavored Markdown (OFM).

Notesmith keeps notes as plain markdown files on disk — no database of record — while providing a configurable workflow engine (routing, hooks, templates, periodic notes) and full-text search. Structure emerges from metadata (fields, tags, links) rather than rigid folder hierarchies.

## Architecture

- **Single binary** (`notesmith`) with subcommands for CLI usage and a `daemon start` mode
- **HTTP daemon** on `127.0.0.1:27183` (Axum) with REST API + Server-Sent Events
- **SvelteKit frontend** served by the daemon for browser use and embedded in Tauri for remote-desktop daemon connections
- **SQLite cache** (rebuildable from markdown files — never the source of truth)
- **Agent-first**: native CLI, MCP adapter, and URL scheme (`notesmith://app/...`) for external integration
- **Generic data model**: unified fields, separate tags, configurable task statuses — no hardcoded note types

## Key Concepts

| Concept | Description |
|---------|-------------|
| **Fields** | Key-value metadata from frontmatter or inline `[key:: value]` syntax. Queried uniformly. |
| **Tags** | Labels from `tags:` frontmatter or inline `#hashtags`. Separate table for ergonomic queries. |
| **Tasks** | Checkbox items with configurable status characters (user-defined in `vault.toml`). |
| **Routing** | YAML DSL with boolean combinators for rule-based note filing and mutation. |
| **Hooks** | 6 lifecycle events that trigger external commands (on_note_create, on_field_change, etc.). |
| **Periodic Notes** | Daily, weekly, monthly, quarterly, yearly — all configurable. |
| **Templates** | Minijinja-based with 3 context layers: static, SQL queries, hook enrichment. |
| **Field Registry** | `.notesmith/fields.toml` — advisory type/value hints for autocomplete. |
| **User Views** | `.notesmith/views.sql` — custom SQL views in the cache database. |

## Architecture

- **Single binary** (`notesmith`) with subcommands for CLI usage and a `daemon start` mode
- **HTTP daemon** on `127.0.0.1:27183` (Axum) with REST API + Server-Sent Events
- **SvelteKit frontend** served by the daemon for browser use and embedded in Tauri for remote-desktop daemon connections
- **SQLite cache** (rebuildable from markdown files — never the source of truth)
- **Agent-first**: native CLI, MCP adapter, and URL scheme (`notesmith://app/...`) for external integration

## Crate Layout

```
crates/
├── notesmith-core       # Note data model, parser traits, OFM extensions
├── notesmith-vault      # VaultEngine trait + native filesystem adapters
├── notesmith-index      # SQLite cache builder + Tantivy full-text search
├── notesmith-query      # Stable SQL views, query execution, dashboard helpers
├── notesmith-templates  # Minijinja template engine with SQL + hook context layers
├── notesmith-routing    # Expressive YAML routing DSL with boolean predicates
├── notesmith-tasks      # Task parsing, configurable status resolution
├── notesmith-hooks      # 6-event hook system for note lifecycle automation
├── notesmith-git        # Opt-in git timers and sync helpers
├── notesmith-html       # Comrak-based HTML rendering and clipboard helpers
├── notesmith-clip       # Server-side web clipping: SSRF-guarded fetch, readability extraction, HTML→Markdown
├── notesmith-config     # Global and per-vault configuration loading
├── notesmith-http       # Axum daemon, REST endpoints, SSE, daemon-hosted MCP
├── notesmith-ops        # Canonical vault operations (Ops trait, LocalOps, ReadOnlyOps)
├── notesmith-mcp        # MCP adapter: daemon HTTP/SSE endpoints + stdio↔HTTP bridge
├── notesmith-cli        # Clap command tree; produces the `notesmith` binary
├── theme-gen            # Build-time theme CSS generator from the catalog JSON
└── notesmith-tauri      # Thin Tauri desktop shell (excluded from default build)
```

## UI

```
ui/app/                  # SvelteKit frontend
ui/extension/            # Manifest V3 web-clipper browser extension
```

### Features

- **Web clipper** — Turn a web page into a clean Markdown note. Extraction runs **server-side** in the daemon (SSRF-guarded, bounded fetch → readability extraction → HTML→Markdown), keyed by canonical `source_url` for dedup, filed to the inbox for routing. Triggers: `POST /api/v/{vault}/clip`, `notesmith clip <url>`, and a Manifest V3 [browser extension](ui/extension/). Optional image download + per-domain minijinja templates via `[clip]` config. See [ADR 0020](docs/adr/0020-web-clipper.md) and [`plans/web-clipper-plan.md`](plans/web-clipper-plan.md)
- **Catalog-backed themes** — Ramp CSS generated from `ui/app/src/styles/theme-catalog.json`, selected at runtime from a flat visual settings gallery with optional follow-system dark/light pairing and a high-contrast overlay
- **Design tokens** — Ramp-backed semantic tokens (`--bg-default`, `--text-default`, …) generated from `ui/app/src/styles/theme-catalog.json`; the legacy `--ns-*` tokens have been fully removed
- **Command palette** — Fuzzy-searchable command runner (⌘K) with keyboard hints and an in-palette theme browser
- **Input palette** — Sequential multi-step inputs for note creation and templates
- **Toast notifications** — Non-blocking success/error/warning alerts
- **Status bar** — Per-window connection badge (local/remote, live status), vault name, cursor position, word count, save state
- **Per-window connections** — Desktop windows each connect to their own daemon, so a local vault and a remote self-hosted vault can be open side by side; switch via the **New Window** menu (vaults grouped by server)
- **Unified right dock** — One collapsible right panel (⌘\\) that switches between **Context** (Metadata, Links, and live TOC with click-to-scroll) and **Chat** (the AI agent), persisted per vault
- **Per-note icons** — `_icon` frontmatter for custom emoji in file tree and tabs
- **Folder notes** — Same-name markdown folder notes with create/open/rename support in the file tree
- **Quick switcher** — Fuzzy note search (⌘O)
- **Tabbed editor** — Source, Live Preview, and Reading View modes with persistence
- **AI agent chat** — Embedded ACP chat panel that auto-discovers external agent CLIs (Copilot, Claude, Codex, Gemini, OpenCode), with manual `[agents]` config overrides, custom agents, and Settings-driven discovery diagnostics
- **Custom agent personas** — Auto-discovered persona/skill/instruction markdown files (vault `.notesmith/{agents,skills,instructions}/` + global `~/.config/notesmith/`); pick a persona from the chat panel or route to it with a leading `@persona-id` mention (ADR 0016)
- **MCP server management** — Settings surface (and global `[mcp]` config) to add external MCP servers (stdio command or HTTP url, args, env) the agent sees alongside the always-on built-in vault tools (ADR 0016)
- **Semantic / hybrid search** — Optional local embeddings (ADR 0018): a colocated `notesmith embed` worker builds a per-vault vector index and the `vault_search` MCP tool fuses Tantivy lexical + vector results via reciprocal-rank fusion. Enabled **per vault, off by default** (`[embed] enabled`); the desktop ships embed-capable with a **Settings → Semantic Search** toggle, and servers offer lean or `*-embed` container images. Local/offline (real `bge-small-en-v1.5` model behind the `local-embed` feature); cloud embedders deferred. `GET /api/v/{vault}/embeddings/stats` exposes index size and search-latency percentiles. See [Semantic & Hybrid Search](docs/ai-semantic-search.md) and [Embeddings: Operating & Monitoring](docs/embeddings-operations.md)
- **AI integration roadmap** — Forward plan for AI features (slash commands, inline editor commands, retrieval, fact memory, headless `notes ai`) built on the principle that the daemon exposes MCP tools and the user's ACP agent orchestrates. Fact memory is defined as atomic Markdown notes with provenance and lifecycle—not a separate database—and is being dogfooded before specialized tools are built. See [Using Fact Memory](docs/fact-memory.md), [ADR 0015](docs/adr/0015-ai-agent-integration-roadmap.md), [ADR 0021](docs/adr/0021-fact-memory-over-markdown-notes.md), and [`plans/ai-integration-roadmap.md`](plans/ai-integration-roadmap.md)

## Golden Vault

`golden-vault/` contains representative markdown notes used as the canonical test fixture across all integration and snapshot tests.

## Development

### Prerequisites

- Rust 1.85+ (edition 2024)
- Node.js 22+ and pnpm 10+

### Build

```sh
cargo build --workspace     # Rust crates
cargo run --bin theme-gen -- --catalog ui/app/src/styles/theme-catalog.json --output ui/app/src/styles/themes
cd ui/app && pnpm build     # SvelteKit app
```

### Run the desktop app (dev)

`notesmith-tauri` is excluded from the default workspace because it pulls in
the Tauri toolchain. It also bundles the `notesmith` CLI as a sidecar — so
launching the desktop with `cargo run` directly will pick up whichever
`crates/notesmith-tauri/binaries/notesmith-<target-triple>` happens to be on
disk, which is often stale.

Use the helper script, which rebuilds the CLI, refreshes the sidecar, and then
runs the desktop in one shot:

```sh
./crates/notesmith-tauri/dev-launch.sh           # debug build (default)
./crates/notesmith-tauri/dev-launch.sh --release # release build
```

The `binaries/notesmith-*` files are gitignored — they are generated artifacts.

### Container images

The default GHCR image is the full app flavor:

```sh
docker pull ghcr.io/surdy/notesmith:latest
```

`latest` includes the `notesmith` binary plus the built SvelteKit frontend so the
daemon can serve `/app/` to browsers. The `api-latest` flavor is binary-only for
CLI, MCP, API-only deployments, and Tauri desktop clients that provide their own
embedded frontend when connected to the server (configured in the desktop app's
**Settings → Connection**; see [deploy/README.md](deploy/README.md) §4).

A local `notesmith` CLI can drive such a remote daemon by setting the global
`--url` flag or the `NOTESMITH_URL` environment variable (e.g.
`NOTESMITH_URL=https://notes.example.com notesmith search ...`); the same target
applies to `notesmith mcp start`, so stdio MCP clients reach the remote vault.

See [deploy/README.md](deploy/README.md) for Docker Compose, Quadlet, and tag
details.

### Test

```sh
cargo test --workspace
```

### Lint

```sh
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Plan

See [plans/notesmith-plan.md](plans/notesmith-plan.md) for the full architectural blueprint.

## License

MIT
