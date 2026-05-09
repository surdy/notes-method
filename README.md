# Notesmith

A markdown notes app built for agentic workflows, compatible with Obsidian Flavored Markdown (OFM).

Notesmith replaces Obsidian with a custom-built tool that keeps notes as plain markdown files on disk — no database of record — while providing all the plugin functionality the [notes method](notes-method.md) depends on as built-in features.

## Architecture

- **Single binary** (`notesmith`) with subcommands for CLI usage and a `daemon start` mode
- **HTTP daemon** on `127.0.0.1:27183` (Axum) with REST API + Server-Sent Events
- **SvelteKit frontend** served by the daemon, wrapped in Tauri for the desktop app
- **SQLite cache** (rebuildable from markdown files — never the source of truth)
- **Agent-first**: native CLI, MCP adapter, and URL scheme (`notesmith://app/...`) for external integration

## Crate Layout

```
crates/
├── notesmith-core       # Note data model, parser traits, OFM extensions
├── notesmith-vault      # VaultEngine trait + native filesystem adapters
├── notesmith-index      # SQLite cache builder + Tantivy full-text search
├── notesmith-query      # Stable SQL views, query execution, dashboard helpers
├── notesmith-templates  # Minijinja template engine and prompt specs
├── notesmith-routing    # YAML-driven routing rules and archive workflow
├── notesmith-tasks      # Task parsing, status transitions, content-hash matching
├── notesmith-hooks      # Subprocess hook runner for note lifecycle events
├── notesmith-git        # Opt-in git timers and sync helpers
├── notesmith-html       # Comrak-based HTML rendering and clipboard helpers
├── notesmith-config     # Global and per-vault configuration loading
├── notesmith-http       # Axum daemon, REST endpoints, SSE
├── notesmith-mcp        # MCP adapter wrapping VaultOps
├── notesmith-cli        # Clap command tree; produces the `notesmith` binary
└── notesmith-tauri      # Thin Tauri desktop shell (excluded from default build)
```

## UI

```
ui/app/                  # SvelteKit frontend
```

## Golden Vault

`golden-vault/` contains representative markdown notes used as the canonical test fixture across all integration and snapshot tests.

## Development

### Prerequisites

- Rust 1.85+ (edition 2024)
- Node.js 22+ and pnpm 10+

### Build

```sh
cargo build --workspace     # Rust crates
cd ui/app && pnpm build     # SvelteKit app
```

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
