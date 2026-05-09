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
