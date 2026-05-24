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

- `v_notes`: `vault_name`, `path`, `title`, `created_at`, `updated_at`, `word_count`
- `v_fields`: `vault_name`, `note_path`, `key`, `value`, `value_type`
- `v_tasks`: `vault_name`, `id`, `note_path`, `line_number`, `text`, `status_char`, `status_group`, `note_title`
- `v_task_fields`: `vault_name`, `task_id`, `key`, `value`, `note_path`
- `v_backlinks`: `vault_name`, `source_path`, `target_path`, `link_text`, `source_title`
- `v_periodic`: `vault_name`, `note_path`, `period_kind`, `period_key`, `period_start`, `period_end`, `title`

## Common workflow recipes

1. Process inbox: `notesmith inbox list` → review → `notesmith route apply --inbox`
2. Create meeting note: `notesmith template instantiate external-meeting --prompt customer=Acme --prompt date=2026-05-10`
3. Weekly review:
   - `notesmith query sql "SELECT t.text, due.value AS due, customer.value AS customer, t.note_path FROM v_tasks t LEFT JOIN v_task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due' LEFT JOIN v_task_fields customer ON customer.vault_name = t.vault_name AND customer.task_id = t.id AND customer.key = 'customer' WHERE t.status_group = 'open' ORDER BY due.value IS NULL, due.value ASC"`
   - `notesmith query sql "SELECT t.text, due.value AS due, t.note_path FROM v_tasks t LEFT JOIN v_task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due' WHERE due.value < date('now') AND t.status_group = 'open' ORDER BY due.value"`
   - `notesmith query sql "SELECT COUNT(*) as count FROM notes WHERE path LIKE 'Inbox/%'"`
4. Daily workflow:
   - `notesmith daily agent-create` for agent-driven prompt assembly or write mode
   - `notesmith daily ensure` for scheduler fallback when no agent is involved

## Routing rules summary

Routing uses `.notesmith/routing.yaml`, an expressive YAML DSL with `all` / `any` / `not`, field predicates, tag predicates, and path globs. The first matching rule wins. Use `notesmith route preview <path>` before moving, and use `notesmith route apply` to apply configured mutations, stamp `archived: true` plus `archived-at`, and move the note.
