# Notes Method

A method for organizing my notes. Some details are figured out; others I want to get ideas on.

## Context

I work with customers, and I want to organize each customer's notes in their own folder.

This method is now intended to be implemented by **Notesmith**, a custom markdown notes app. The definitive application blueprint is `plans/notesmith-plan.md`.
The repository root is also the Notesmith Cargo workspace root, with Rust crates living under `crates/` alongside the planning, vault, and spike directories.
- Notesmith should also expose the vault to local AI clients through an MCP server (`notesmith mcp start`) so agents can create, read, search, route, and template notes over stdio without going through the HTTP daemon.

## Per-Customer Folder Structure

For each customer there would be a folder containing:

- **Internal meetings**
- **External meetings**
- **Account information**
  - Account information (note)
  - Glossary
  - Dates or Milestones
- **Projects or streams of work** for that customer

## High-Level Structure

- Inbox
- Tasks (aggregated)
- Dashboards
- Customer 1
- Customer 2
- Customer N
- General
- Assets
  - templates
  - data

## Sidebar Views

- The left sidebar should support both the file tree and YAML-driven smart views from `.notesmith/sidebar-views.yaml`.
- Default smart views should cover **All Notes**, **Tasks**, **Recent**, and **Inbox**.
- Smart views should be backed by read-only SQL over cached note/task views, with optional grouping and badge counts.
- The main note workspace should also include a contextual **right rail** for metadata, backlinks, and outgoing links on the active note.

## Editor Experience

- The primary note surface should be an editable **CodeMirror 6** OFM editor, not a read-only HTML viewer.
- Notes should auto-save shortly after edits and support explicit save with the platform save shortcut.
- If a file changes on disk while the editor is clean, the editor should silently reload; if it is dirty, the editor should warn and let me reload or keep my in-memory edits.
- OFM affordances should stay visible while editing, especially wikilinks, tags, task checkboxes, callout markers, inline fields, and YAML frontmatter.
- Dashboard notes should stay as normal markdown files in the editor; fenced `notesmith` and `notesmith sql` blocks should execute read-only SQL against the cache and render inline result tables.

## Desktop App UX

- The primary desktop experience should be a three-pane app: sidebar on the left, tabbed editor workspace in the center, and a collapsible contextual right rail on the right.
- The note workspace should use tabs that persist across launches and remember each tab's current view mode.
- Each open tab should support three modes: **Source**, **Live Preview**, and **Reading View**, with a breadcrumb toolbar and a simple mode toggle in the header.
- The desktop shell should provide a command palette, quick switcher, and keyboard-first navigation for note creation, search, daily notes, inbox capture, archiving, and view toggling.
- The app should use a dark theme by default and support note deep links through a `notesmith://app/...` URL scheme.

## Inbox Workflow

- All notes start in the **Inbox** folder.
- Once I am done working on a note, I move it to the appropriate folder for long-term storage.
- **Routing engine** (`.notesmith/routing.yaml`) automatically determines each note's destination based on frontmatter fields (`type`, `customer`, `meeting-kind`, `stream`) and moves notes with `notesmith route apply --inbox` or per-note `notesmith route apply <path>`.
- Routed notes are stamped with `archived: true` and `archived-at` in frontmatter before moving.
- The goal is to achieve **Inbox zero**.

## Daily Notes

- Every morning I want a note for that day generated into **Inbox/Daily**. Primary creation should come from an external agent using a saved prompt template, with a daemon scheduler available as a fallback.
- Vault-specific agent instructions should live in `.notesmith/skill.md`, and saved prompt templates should live in `.notesmith/prompts/` so agents can assemble daily-note context consistently.
- Vault hooks can trigger external automation on note creation (`on_note_create`) and daily note creation (`on_daily_create`) without blocking the underlying Notesmith action.

## Tasks

- I want an aggregation of tasks from all notes to appear in a single place.
- The aggregated list should also show the associated project.
- Aggregated tasks should link to the stream note.
- The primary aggregated task list should show only **active** tasks (**To Do** and **In Progress**).
- There should be separate aggregated views for **Blocked**, **Awaiting Customer**, and **On Hold** tasks.
- Each meeting note optionally has tasks associated with it.
- A task can be associated with a stream of work (note).
- Tasks can have a status independent of the stream of work.
- Task statuses: **To Do**, **In Progress**, **Blocked**, **Awaiting Customer**, **On Hold**, **Done**, **Cancelled**.

## Streams of Work

- A stream of work has a status: **In Progress**, **Blocked**, **Done**, **Awaiting Customer**, **On Hold**.
- Tasks can be added to a stream of work regardless of its state.

## Assets / Resources

- Separate folder for assets/resources that are not notes but might be referenced in notes.

## Customer Folders

- Each customer folder would have the same structure, but can optionally add more folders or notes as needed.

## Customer State

- Customers have a state associated with them based on my relationship with them: **Active**, **On Hold**, **Temp**, **Inactive**.
- I will use these states to filter customer folders and smart views.
- Customer state should live in the Customer Index note frontmatter (`state:`), not in the Account Info note.

## Open for Ideas

- What additional customer metadata should live alongside `state:` on the Customer Index note.

## Git Integration

- Opt-in per-vault git sync via `[git]` in `vault.toml` (`enabled`, `auto_commit_every`, `auto_pull_every`, `auto_push_every`, `commit_message`).
- Auto-commit stages only note-relevant file types (`.md`, `.yaml`, `.yml`, `.toml`, `.json`, images, `.pdf`).
- Pull uses fast-forward only — conflicts abort and log a warning instead of attempting resolution.
- Auto-push always pulls first to minimize conflicts.
- CLI: `notesmith git {status, pull, push, sync, log}`.
- HTTP: `GET /api/v/{vault}/git/status`, `POST /api/v/{vault}/git/sync`.
- Non-git vaults are completely unaffected.
