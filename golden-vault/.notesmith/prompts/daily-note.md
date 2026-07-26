---
context_queries:
  - name: open_tasks
    sql: >
      SELECT t.text, t.note_path, due.value AS due
      FROM v_tasks t
      LEFT JOIN v_task_effective_fields due
        ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
      WHERE t.status_group = 'open'
      ORDER BY due IS NULL, due
      LIMIT 20
  - name: recent_meetings
    sql: >
      SELECT n.title, d.value AS date, n.path
      FROM v_notes n
      JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
       AND k.key = 'kind' AND k.value = 'meeting'
      JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
       AND d.key = 'date' AND d.value >= date('now', '-7 days')
      ORDER BY d.value DESC
      LIMIT 10
  - name: blocked_streams
    sql: >
      SELECT n.title, s.value AS status, n.path
      FROM v_notes n
      JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
       AND k.key = 'kind' AND k.value = 'stream'
      JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
       AND s.key = 'status' AND s.value IN ('blocked', 'waiting')
      ORDER BY n.title
  - name: inbox_count
    sql: "SELECT COUNT(*) as count FROM v_notes WHERE path LIKE 'Inbox/%'"
---

# Daily Note Prompt

You are generating today's daily note for a knowledge worker's vault.

## Context

Today's date: {{ today }}

### Open Tasks
{{ open_tasks }}

### Recent Meetings (last 7 days)
{{ recent_meetings }}

### Blocked / Waiting Streams
{{ blocked_streams }}

### Inbox Status
{{ inbox_count }}

## Instructions

Generate a daily note with:
1. A "Plan" section prioritizing the most important tasks for today
2. A "Follow-ups" section for any meeting action items from the past week
3. An "Attention" section for blocked or waiting streams
4. An "Inbox Review" section if there are unprocessed inbox items
5. A "Notes" section (empty, for throughout-the-day capture)

Use the `daily` template format with frontmatter: date: {{ today }}, tags: [daily]
