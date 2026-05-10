# Notesmith Vault Skill

Use this file as the vault-specific operating manual. Prefer Notesmith commands over ad hoc file edits so the daemon, cache, routing, and events stay consistent.

## Command cheat sheet

```bash
notesmith daemon start            # Start the daemon
notesmith daemon status           # Check daemon status
notesmith note list               # List all notes
notesmith note get <path>         # Read a note
notesmith note create --title "X" --content "Y" --folder "Inbox"
notesmith note update <path> --content "Y"
notesmith note append <path> --content "Y"
notesmith note delete <path>
notesmith note move <from> <to>
notesmith search "query"          # Full-text search
notesmith query sql "SELECT ..."  # SQL query
notesmith task list               # List tasks
notesmith task add <path> --text "..." --status todo
notesmith task toggle <path> --hash <hash> --status done
notesmith inbox add --content "..."  # Quick capture
notesmith inbox list
notesmith template list
notesmith template render <name> --prompt key=value
notesmith template instantiate <name> --prompt key=value
notesmith route preview <path>    # Preview routing destination
notesmith route apply --inbox     # Route all inbox notes
notesmith daily ensure            # Create today's daily note
notesmith daily open              # Open today's daily note
notesmith daily agent-create      # Agent workflow (see below)
notesmith skill print             # Output this skill file
```

## Vault folder structure

```text
Inbox/              — New/unprocessed notes land here
Inbox/Daily/        — Daily notes
Customers/          — Per-customer folders
  <Customer>/
    Account Info/   — Account info, glossary, milestones
    Meetings/       — Meeting notes (internal + external)
    Streams/        — Work streams
General/            — Non-customer notes
Assets/
  templates/        — Note templates (.md.j2)
  scripts/          — Hook scripts
  data/             — Data files
.notesmith/
  vault.toml        — Vault configuration
  routing.yaml      — Routing rules
  skill.md          — This file
  prompts/          — Saved prompt templates
```

## Note type schemas

- `daily`: `type`, `date`, `weather`, `energy`
- `meeting` (external): `type`, `customer`, `meeting-kind: external`, `date`, `attendees`, `stream`
- `meeting` (internal): `type`, `customer`, `meeting-kind: internal`, `date`, `attendees`
- `customer`: `type`, `customer`, `state` (`Active` / `Churned` / `Prospect`)
- `stream`: `type`, `customer`, `stream`, `status` (`active` / `paused` / `completed`)
- `account-info`: `type`, `customer`
- `glossary`: `type`, `customer`
- `milestones`: `type`, `customer`
- `note`: `type`, optional `customer`, optional `stream`

## SQL view contract

- `v_notes`: `vault_name`, `path`, `title`, `type`, `customer`, `stream`, `state`, `status`, `date`, `created_at`, `updated_at`, `archived`, `mtime_unix`, `frontmatter_json`
- `v_tasks`: `vault_name`, `task_hash`, `note_path`, `heading_path`, `ordinal`, `status`, `text`, `customer`, `stream`, `owner`, `due`, `scheduled`, `start_date`, `done_at`, `priority`
- `v_backlinks`: `note_path`, `backlink_path`, `kind`, `heading_ref`, `block_ref`
- `v_customers`: same columns as `v_notes`, filtered to `type = 'customer'`
- `v_streams`: same columns as `v_notes`, filtered to `type = 'stream'`

## Common workflow recipes

1. Process inbox: `notesmith inbox list` → review → `notesmith route apply --inbox`
2. Create meeting note: `notesmith template instantiate external-meeting --prompt customer=Acme --prompt date=2026-05-10`
3. Weekly review:
   - `notesmith query sql "SELECT text, due, customer, note_path FROM v_tasks WHERE status IN ('todo', 'in_progress') ORDER BY due NULLS LAST"`
   - `notesmith query sql "SELECT text, due, note_path FROM v_tasks WHERE due < date('now') AND status IN ('todo', 'in_progress') ORDER BY due"`
   - `notesmith query sql "SELECT COUNT(*) as count FROM v_notes WHERE path LIKE 'Inbox/%' AND archived = 0"`
4. Daily workflow:
   - `notesmith daily agent-create` for agent-driven prompt assembly or write mode
   - `notesmith daily ensure` for scheduler fallback when no agent is involved

## Routing rules summary

Routing is frontmatter-driven. `.notesmith/routing.yaml` matches fields like `type`, `customer`, `meeting-kind`, and `stream`, then maps the note to a destination folder; the first matching rule wins. Use `notesmith route preview <path>` before moving, and use `notesmith route apply` to move notes and stamp them with `archived: true` plus `archived-at`.
