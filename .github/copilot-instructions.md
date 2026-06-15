# Custom Instructions

## Change Tracking

- All changes to this repository must be tracked in git as commits.
- Make a commit for each logical change. Do not leave changes uncommitted at the end of a task.
- Use clear, descriptive commit messages.

## Build, Test & Validation

- **Rust workspace gates** (run after every change): `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`.
- **`notesmith-tauri` is excluded from the workspace** (`Cargo.toml`: `exclude = ["crates/notesmith-tauri", ...]`). The `--workspace` gates **do not** cover it. When you touch `notesmith-tauri` (or `notesmith-agent`, which it depends on), validate it separately: `cd crates/notesmith-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --all`.
- **Run a single test** with `cargo test -p <crate> <test_name>` (e.g. `cargo test -p notesmith-agent select_mcp`). Per-crate suites: `cargo test -p notesmith-vault`, `-p notesmith-index`, `-p notesmith-query`, `-p notesmith-config`.
- **Frontend (`ui/app`)**: `npm run check` (svelte-check), `npx vitest run` (unit), `npm run build` (adapter-static). Theme tokens: `bash scripts/check-theme-tokens.sh`.

## Tauri Desktop App

The desktop shell (`crates/notesmith-tauri`) spawns the daemon as a sidecar; the daemon serves the SvelteKit frontend, and the webview loads it from the **daemon URL** (e.g. `http://127.0.0.1:27183/`), not a local asset.

- **Dev loop / which rebuild applies:**
  - Frontend-only change → `cd ui/app && npm run build`, then ⌘R in the running app.
  - Rust change in `notesmith-tauri` **or** `notesmith-agent` → `cargo tauri build` and **relaunch `Notesmith.app`**. The already-running daemon/app will **not** pick up Rust changes — a stale running daemon is a common source of "my fix didn't apply" confusion.
  - For local dev, launch via `crates/notesmith-tauri/dev-launch.sh` — it builds the frontend bundle and pins `NOTESMITH_APP_DIR`. A missing bundle / unset `NOTESMITH_APP_DIR` produces a **blank window** (the daemon resolves the wrong frontend dir).
- **IPC commands (`#[tauri::command]`):**
  - Use the **concrete `tauri::AppHandle`** — a generic `<R: Runtime>` parameter makes the proc-macro silently skip registration, surfacing as `"<cmd> not allowed. Plugin not found"` at runtime.
  - Every new command must be granted in **both** capability files — `crates/notesmith-tauri/capabilities/default.json` **and** `capabilities/vault-windows.json` — and under the **`remote` URL context** (the webview is on the daemon URL). A missing remote grant surfaces as `"<cmd> not allowed on window ... URL: http://127.0.0.1:...""`.

## Agent MCP Transport (ADR 0012)

- The desktop exposes the active vault to ACP agents over MCP with **capability-aware** transport: prefer an **HTTP** binding (`/mcp/<vault>` rw, `/mcp-ro/<vault>` ro) when the agent advertises `mcpCapabilities.http` at `initialize`; supply a local **stdio bridge** (`notesmith mcp start`) only as a fallback. **Copilot is HTTP/SSE-only** — it silently ignores stdio `mcpServers`.
- **Read-only sessions allow their (read-only) permission requests silently** — they can only ever expose safe reads. Do not reintroduce a read-only "hard-deny". Read-write writes prompt per-call.
- Tauri **strips the target-triple** from bundled `externalBin`, so the sidecar resolves as `notesmith` (not `notesmith-<triple>`) at runtime; resolvers must check both names.
- See `docs/adr/0012-agent-transport-acp-mcp.md` for the full transport/permission policy.

## Keeping `notes-method.md` Up to Date

- `notes-method.md` is the source of truth for the notes organization method/plan.
- As the plan evolves, update `notes-method.md` to reflect the current state.
- Whenever a decision is made, an idea is added, a section is refined, or an open question is resolved, update `notes-method.md` in the same change.
- Preserve the existing structure and intent — wordsmith and reorganize as needed, but do not silently drop or reinterpret items.

## Test-Driven Development (TDD)

- All Notesmith implementation work must follow TDD: **red → green → refactor**.
- Write failing tests first that define the expected behavior, then write the minimal implementation to make them pass, then refactor.
- Tests go in the standard Rust locations: unit tests in `#[cfg(test)] mod tests` within source files, integration tests in `tests/` directories.
- Use the `golden-vault/` fixture for integration tests against real vault content.
- Use the `insta` crate for snapshot tests where appropriate (rendered output, parsed structures, routing decisions).

## Frontend–Backend Contract Testing

- When frontend code issues SQL queries against backend views (e.g., `v_backlinks`, `v_notes`, `v_tasks`), add an integration test in `tests/` that runs the **exact query** against a real SQLite database populated via the indexer. This catches column-name mismatches before they reach production.
- When adding or changing SQL views in the backend, verify that all frontend query builders (`right-rail.ts`, etc.) still reference valid columns. A grep for the view name across `ui/app/src/` is a quick sanity check.
- When frontend components depend on API response shapes (column names, JSON field names), add at least one integration test that hits the HTTP endpoint and asserts on the response structure — not just status code.
- When frontend code has loading-state guards (e.g., early returns during async fetches), ensure every exit path resets the loading flag. Prefer a `finally` block or equivalent pattern to guarantee cleanup.
- For frontend loading/race bugs, validate with a headless browser flow that asserts the user-visible state and bounds duplicate API calls. Curl-only API checks and cache-header checks are not sufficient for Svelte reactive-loop failures.

## Keeping Architectural Documents in Sync

- `CONTEXT.md`, `notes-method.md`, `README.md`, `plans/notesmith-plan.md`, and `docs/adr/` are the authoritative architecture and product references. They must stay consistent with the codebase.
- When a change renames concepts, adds/removes features, changes API surfaces, or alters configuration, update **all relevant files** in the same commit (or the same PR):
  - `CONTEXT.md` — domain glossary entries and definitions (especially the Frontend section).
  - `notes-method.md` — the notes organization method and product requirements.
  - `README.md` — high-level features list and architecture overview.
  - `plans/notesmith-plan.md` — the definitive architectural blueprint (sections, examples, tables).
  - `docs/adr/` — create a new ADR when making a significant architectural decision; update existing ADRs if a decision is superseded.
- After completing a batch of changes, grep these files for stale terminology before considering the work done.
- Preserve existing structure and intent — update surgically, do not rewrite sections unnecessarily.

## User-Facing Documentation

- When completing an issue that adds or changes CLI commands, HTTP endpoints, or SQL views, update the corresponding doc file in `docs/`.
- CLI commands go in `docs/cli.md`, HTTP endpoints in `docs/http-api.md`, SQL views in `docs/sql-views.md`.
- Create the doc file if it doesn't exist yet.
- Keep docs concise: show the command/endpoint signature, parameters, and a usage example.
- Don't document internal crate APIs or architecture — only user-facing surfaces.

## Frontend Styling — Design Tokens

- All UI colors are defined as `--ns-*` CSS custom properties in `:root {}` in `ui/app/src/app.css`. Components must reference these tokens — never define ad-hoc color values.
- When adding new UI elements, use existing tokens (`var(--ns-bg)`, `var(--ns-text)`, `var(--ns-border)`, etc.). If no token fits, add a new `--ns-*` token to `app.css` and reference it.
- Do not use inline fallback values (e.g., `var(--ns-bg, #1e1e1e)`) — tokens are centrally defined.
- Five themes exist as CSS class overrides (`.theme-dark`, `.theme-light`, `.theme-manuscript`, `.theme-hc-dark`, plus System mode). New components must look correct across all themes.
- Editor-specific colors use separate `--ns-editor-*` tokens so the Manuscript theme (dark chrome + light editor) works.
- The global CSS reset in `app.css` sets `color: inherit; font: inherit;` on form elements. Do not remove this.
- Never leave a `<button>` or `<input>` without an explicit `color` declaration in its scoped CSS — the global reset is a safety net, not a substitute.

## CodeMirror Decorations

- Do not provide block decorations (`Decoration.widget({ block: true })`) from `ViewPlugin.decorations`; CodeMirror rejects them. Use a `StateField` with `EditorView.decorations.from(field)` and update it via state effects.
- When building decoration sets from mixed line, mark, replace, or widget decorations, prefer `Decoration.set(ranges, true)` unless the ordering is proven valid. This avoids `Ranges must be added sorted by from position and startSide` crashes.
- Validate CodeMirror decoration changes with a headless browser flow against notes containing the relevant syntax (SQL fences, frontmatter, tasks, callouts), not just TypeScript checks.

## Resilience to Malformed User Content

All `.md` content is **untrusted input**. A single malformed note must never crash the daemon, the desktop app, or any indexing pass. See [`docs/adr/0009-resilience-to-malformed-content.md`](../docs/adr/0009-resilience-to-malformed-content.md) for the full policy.

When writing or reviewing code that touches note content:

- **Isolate at the per-note boundary.** Any parse/render/index operation on a single note must catch errors locally, log `WARN note=<path> stage=<...> reason=<...>`, and continue with the next note. Never let a per-note error propagate up to startup or to a transaction that touches multiple notes.
- **Forbidden in hot paths:** `?` propagation of `serde_yaml::from_str(...)` (or any `serde_json` / `toml` parse of note-derived bytes) above the per-note boundary. Use `.ok()` and fall back to `None` / `"{}"` / empty.
- **`unwrap` / `expect` are forbidden** on values derived from file content. Allowed only for: regex compile at module init, mutex locks (programmer bugs), and test code.
- **Indexer loops use per-note savepoints**, not one big transaction. One bad note must not roll back the whole vault.
- **HTTP handlers** that accept arbitrary bodies return structured 4xx, never let parse failures become 500s (model: `routes::routing::preview`).

### Tests required for any new parser, renderer, or indexer touching note content

1. Happy-path test with well-formed input.
2. **Malformed-content test** — broken YAML/Markdown/link syntax produces a degraded-but-valid result and logs a warning. Add a fixture under `test-fixtures/malformed-vault/` if no existing fixture covers the case.
3. **No-panic test** — pathological input (e.g. unclosed code fences, nested `{{` placeholders, non-UTF-8 sequences via fuzz, deeply nested YAML) does not panic and completes in bounded time.

When fixing a resilience bug, add the offending content to `test-fixtures/malformed-vault/` as a regression fixture in the same commit. Label related issues with `resilience`.

## Sub-Agent Delegation

- Use sub-agents (via the `task` tool) to parallelize independent work and to delegate complex implementation that benefits from a focused context window.
- For small, targeted changes (single-file edits, doc tweaks, config changes), work directly — the overhead of prompt crafting + wait time isn't worth it.

### When to delegate
- **Exploring unfamiliar code**: Launch parallel `explore` agents to research different crates/modules simultaneously.
- **Complex implementation**: Delegate multi-file feature work to a `general-purpose` agent with the highest-capability reasoning model available.
- **Running validation**: Use `task` agents for builds, tests, and lints where only pass/fail matters.
- **Researching external APIs**: Use `research` agents for crate documentation, API references, or design pattern lookups.

### Agent type → model selection
| Agent type | Default model | Use for |
|---|---|---|
| `explore` | Haiku (fast) | Codebase research, file lookups, reading multiple modules |
| `task` | Haiku (fast) | Commands: tests, builds, lints — pass/fail only |
| `general-purpose` | Best reasoning model | Multi-step implementation requiring full tooling |
| `code-review` | Sonnet | Reviewing diffs for bugs/regressions before committing |
| `research` | Default | External docs, API references, design patterns |

### Prompt quality is critical
- Sub-agents are stateless — provide complete context in every prompt.
- Include: exact file paths and line ranges, existing code patterns to follow, struct/type definitions, expected test patterns, and acceptance criteria.
- The more precise the prompt, the more likely the agent succeeds on the first attempt.
