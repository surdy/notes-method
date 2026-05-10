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

## User-Facing Documentation

- When completing an issue that adds or changes CLI commands, HTTP endpoints, or SQL views, update the corresponding doc file in `docs/`.
- CLI commands go in `docs/cli.md`, HTTP endpoints in `docs/http-api.md`, SQL views in `docs/sql-views.md`.
- Create the doc file if it doesn't exist yet.
- Keep docs concise: show the command/endpoint signature, parameters, and a usage example.
- Don't document internal crate APIs or architecture — only user-facing surfaces.

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
