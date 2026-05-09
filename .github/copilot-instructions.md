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

- Use sub-agents (via the `task` tool) to parallelize work whenever tasks are independent of each other.
- Launch multiple agents in parallel when the work can be decomposed: e.g., exploring different crates simultaneously, running tests and linting concurrently, or writing tests for independent modules at the same time.
- Choose the right agent type and model for the job:
  - **explore** (Haiku) — codebase research, file lookups, reading multiple modules in parallel.
  - **task** (Haiku) — running commands (tests, builds, lints) where only pass/fail matters.
  - **general-purpose** (Opus 4.6 or GPT-5.5) — complex multi-step implementation work that needs full tooling and high-quality reasoning.
  - **code-review** (Sonnet) — reviewing diffs for bugs and regressions before committing.
- Do not do work yourself that a sub-agent can own end-to-end; delegate and collect results.
- Provide complete context in each sub-agent prompt — sub-agents are stateless.
