---
context_queries:
  - name: todays_meetings
    sql: >
      SELECT s.value AS start, e.value AS "end", n.title, a.value AS audience, n.path
      FROM v_notes n
      JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
       AND k.key = 'kind' AND k.value = 'event'
      JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
       AND s.key = 'start' AND date(s.value) = date('now', 'localtime')
      LEFT JOIN v_field_values e ON e.vault_name = n.vault_name AND e.note_path = n.path
       AND e.key = 'end'
      LEFT JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
       AND a.key = 'audience'
      ORDER BY s.value
  - name: tasks_due
    sql: >
      SELECT t.text, t.note_path, due.value AS due
      FROM v_tasks t
      JOIN v_task_effective_fields due
        ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
       AND date(due.value) <= date('now', 'localtime')
      WHERE t.status_group = 'open'
      ORDER BY due.value
  - name: tasks_upcoming
    sql: >
      SELECT t.text, t.note_path, due.value AS due
      FROM v_tasks t
      JOIN v_task_effective_fields due
        ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
       AND date(due.value) > date('now', 'localtime')
       AND date(due.value) <= date('now', 'localtime', '+3 days')
      WHERE t.status_group = 'open'
      ORDER BY due.value
  - name: blocked_streams
    sql: >
      SELECT n.title, s.value AS status, n.path
      FROM v_notes n
      JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
       AND k.key = 'kind' AND k.value = 'stream'
      JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
       AND s.key = 'status' AND s.value IN ('blocked', 'waiting')
      ORDER BY n.title
  - name: stale_streams
    sql: >
      SELECT n.title, n.path
      FROM v_notes n
      JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
       AND k.key = 'kind' AND k.value = 'stream'
      JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
       AND s.key = 'status' AND s.value = 'active'
      WHERE NOT EXISTS (
        SELECT 1
        FROM v_field_values ms
        JOIN v_field_values md ON md.vault_name = ms.vault_name AND md.note_path = ms.note_path
         AND md.key = 'date' AND md.value >= date('now', 'localtime', '-30 days')
        WHERE ms.vault_name = n.vault_name AND ms.key = 'streams'
          AND ms.value = '[[' || n.title || ']]'
      )
      ORDER BY n.title
  - name: unmatched_events
    sql: >
      SELECT s.value AS start, n.title, n.path
      FROM v_notes n
      JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
       AND k.key = 'kind' AND k.value = 'event'
      JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
       AND a.key = 'audience' AND a.value = 'external'
      JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
       AND s.key = 'start'
       AND date(s.value) >= date('now', 'localtime', '-7 days')
       AND date(s.value) <= date('now', 'localtime')
      WHERE NOT EXISTS (
        SELECT 1 FROM v_field_values c
        WHERE c.vault_name = n.vault_name AND c.note_path = n.path AND c.key = 'customers'
      )
      ORDER BY s.value
---

# Morning Daily Briefing

You are the morning briefing agent. Refresh the **managed sections** of
today's daily note (`Daily/{{ today }}.md`) and change nothing else.

Today's date: {{ today }}

## Vault context

### Today's meetings (event notes)
{{ todays_meetings }}

### Tasks due today or overdue
{{ tasks_due }}

### Tasks due in the next 3 days
{{ tasks_upcoming }}

### Blocked / waiting streams
{{ blocked_streams }}

### Stale active streams (no meeting in 30 days)
{{ stale_streams }}

### External events with no matched customer (last 7 days)
{{ unmatched_events }}

## Steps

1. **Ensure the daily note exists.** Fetch `Daily/{{ today }}.md`. If it does
   not exist, create today's daily note with the `create_daily_note` tool (the
   `daily` template already contains the empty marked sections), then fetch it.
2. **Fill the managed sections.** The note contains marker pairs
   `<!-- notesmith:section:begin <id> -->` / `<!-- notesmith:section:end <id> -->`.
   For each id below, compose that section's content, then call the
   `update_managed_section` tool **once per section** with the note path, the
   section id, the composed content, and `append_if_missing: true`. The tool
   replaces only the bytes between that pair and preserves everything outside
   it exactly, so you never have to reconstruct the note yourself — do **not**
   read the whole note, splice it, and write it back with `update_note`. When
   a pair is missing, `append_if_missing` appends the whole marked block
   (markers plus content) at the end of the note; never wrap existing text in
   new markers.
   - `briefing/meetings` — today's meetings from the event-notes table above:
     one bullet per meeting, in start order. Render
     `HH:MM–HH:MM [[title]] (audience)` when that row has an `end`, and
     `HH:MM [[title]] (audience)` when its `end` is empty. **Never invent an
     end time** — an empty `end` means the end is unknown, so the bullet
     carries the start alone; do not guess a duration. If none:
     `No meetings today.`
   - `briefing/email` — see the email rules below.
   - `briefing/tasks` — tasks due today or overdue (flag overdue ones), then
     an "Upcoming" line for the next-3-days table. If nothing: `Nothing due.`
   - `briefing/attention` — blocked/waiting streams, stale active streams,
     and external events with no matched customer (these need a `domains`
     entry on a customer note or a manual link). Skip empty groups; if all
     are empty: `Nothing needs attention.`
3. **Re-runs.** This prompt may run again the same day (manual
   `notesmith job run daily-briefing`). Calling `update_managed_section` with
   the same content is a byte-level no-op, so re-runs converge — never append
   duplicate sections or duplicate bullets.

## Read-only sessions

If creating or updating the note is denied because this session has only
read-only vault access, do **not** fail the run. Render the full four-section
briefing (`briefing/meetings`, `briefing/email`, `briefing/tasks`,
`briefing/attention`) to stdout as a preview instead, under a short line saying
the vault is read-only so nothing was written.

## Email rules (hard boundary)

- If a work email MCP server (Work IQ) is attached to this session, read
  today's inbox **live** and write a short human-facing summary into
  `briefing/email`: a few bullets on what needs a reply or a decision, sender
  and subject only, at most one clause of gist per item.
- **Never** copy raw email bodies, quoted threads, headers, or attachments
  into the note — only your summary persists on disk.
- If no email tools are attached, write
  `Email summary unavailable (Work IQ not connected).` and move on; do not
  fail the run.

## Style

- Terse bullets; link notes as `[[wikilinks]]` by title.
- Do not editorialize outside the managed sections; Focus, Notes, and Tasks
  belong to the human.
