# Custom Instructions

## Change Tracking

- All changes to this repository must be tracked in git as commits.
- Make a commit for each logical change. Do not leave changes uncommitted at the end of a task.
- Use clear, descriptive commit messages.

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

## User-Facing Documentation

- When completing an issue that adds or changes CLI commands, HTTP endpoints, or SQL views, update the corresponding doc file in `docs/`.
- CLI commands go in `docs/cli.md`, HTTP endpoints in `docs/http-api.md`, SQL views in `docs/sql-views.md`.
- Create the doc file if it doesn't exist yet.
- Keep docs concise: show the command/endpoint signature, parameters, and a usage example.
- Don't document internal crate APIs or architecture — only user-facing surfaces.

## Frontend Styling — Dark Theme Safety

- The app uses a dark theme by default. All interactive elements (`<button>`, `<input>`, `<select>`, `<textarea>`) must have `color: inherit` or an explicit color — browsers default these to black text.
- The global CSS reset in `+layout.svelte` sets `color: inherit; font: inherit;` on form elements. Do not remove this.
- When adding new styled components, always verify text visibility on the dark background (`--bg-primary: #1e1e1e`, `--sidebar-bg: #252526`). Use `var(--text-primary, #e0e0e0)` for primary text and `var(--text-muted, #888)` for secondary text.
- Never leave a `<button>` or `<input>` without an explicit `color` declaration in its scoped CSS — the global reset is a safety net, not a substitute.

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
