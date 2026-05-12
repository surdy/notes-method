# ADR-0006: One Crate Per Domain Concept

**Status**: Accepted  
**Date**: 2025 (initial workspace design)

## Context

Notesmith has multiple domain areas: note parsing, indexing, querying, routing, templates, git sync, task management, config, HTTP serving, MCP, and CLI. We needed to decide between a monolithic crate, a few large crates, or fine-grained crates.

## Decision

Use **one crate per domain concept**:

- `notesmith-core` — shared types and traits (Note, Task, VaultEngine, Frontmatter)
- `notesmith-vault` — filesystem I/O, parsing, save pipeline
- `notesmith-index` — SQLite cache + Tantivy search
- `notesmith-query` — SQL query execution
- `notesmith-routing` — rule-based note filing
- `notesmith-templates` — Minijinja template engine
- `notesmith-tasks` — task aggregation and mutation
- `notesmith-hooks` — lifecycle hook execution
- `notesmith-git` — git operations and auto-sync timers
- `notesmith-html` — markdown → HTML rendering
- `notesmith-config` — config types, loading, vault detection
- `notesmith-http` — HTTP daemon, SSE, file watcher, scheduler
- `notesmith-mcp` — MCP server for AI agent access
- `notesmith-cli` — CLI binary

Dependency flows downward: `core ← domain crates ← orchestrators (http, mcp, cli)`.

## Consequences

- Clear ownership boundaries — each crate has a focused responsibility
- Incremental compilation: changing task logic doesn't rebuild the parser
- Some crates are very thin (e.g. `notesmith-query` is 6 lines of re-exports) — may warrant consolidation if they don't deepen over time
- `notesmith-http` is the heaviest orchestrator, depending on 11 workspace crates
- No circular dependencies (enforced by Cargo)
