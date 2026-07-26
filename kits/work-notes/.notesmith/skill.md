# Work Notes vault

Entity model: meetings (dated event records, `Meetings/YYYY/MM/`), streams
(ongoing initiatives, `Streams/`), customers (`Customers/<Name>/`), people
(`People/`). `kind` is the canonical type field — `meeting`, `stream`,
`customer`, `account`, `person`. Tags are topical only, never kinds.

Relationships are frontmatter lists of quoted wikilinks: `customers`, `streams`,
`attendees`. Folders are for humans — never infer relationships from paths. A
meeting can relate to zero, one, or many customers; external meetings have
exactly one.

## Folder structure

```text
Inbox/              — capture lands here until enriched + routed
Meetings/YYYY/MM/   — dated meeting records
Streams/            — ongoing initiatives (status is metadata; done streams stay)
Customers/<Name>/   — <Name>.md folder note + optional kind:account notes
People/             — created lazily, when someone recurs
Daily/ Weekly/ Quarterly/
Dashboards/
General/            — notes with no kind; routing leaves them alone
```

## Retrieval

- Membership queries: `v_field_values` (one row per list member; exact value
  match, e.g. `key='customers' AND value='[[Acme Corp]]'`).
- Task queries: `v_task_effective_fields` — tasks inherit their note's
  frontmatter; task-level inline fields override per key.
- `list_notes` / `list_tasks` take a `fields` map with the same semantics.
- Free-text digging: `vault_search` (hybrid), with `filters` for
  `fields` / `tags` / `path_prefix`. Time-based: `time_query`.
- Cite notes by path; quote the exact line when reporting decisions.

## Writing

- Meeting/stream/person notes: use `create_from_template`, then enrich
  frontmatter. New notes land in `Inbox/`; routing files them by `kind`.
- Quote wikilinks in YAML: `- "[[Acme Corp]]"`.
- Tasks: plain checkboxes; only add `[due:: ]` / `[owner:: ]` for real
  deadlines or delegation. Don't copy note metadata onto tasks.
- Do not create People notes for one-off attendees; link them and move on.

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
notesmith skill print             # Output this skill file
```
