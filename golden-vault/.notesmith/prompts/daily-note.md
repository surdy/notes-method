---
context_queries:
  - name: open_tasks
    sql: "SELECT text, due, customer, note_path FROM v_tasks WHERE status IN ('todo', 'in_progress') ORDER BY due NULLS LAST LIMIT 20"
  - name: recent_meetings
    sql: "SELECT title, customer, date FROM v_notes WHERE type = 'meeting' AND date >= date('now', '-7 days') ORDER BY date DESC LIMIT 10"
  - name: inbox_count
    sql: "SELECT COUNT(*) as count FROM v_notes WHERE path LIKE 'Inbox/%' AND archived = 0"
---

# Daily Note Prompt

You are generating today's daily note for a knowledge worker's vault.

## Context

Today's date: {{ today }}

### Open Tasks
{{ open_tasks }}

### Recent Meetings (last 7 days)
{{ recent_meetings }}

### Inbox Status
{{ inbox_count }}

## Instructions

Generate a daily note with:
1. A "Plan" section prioritizing the most important tasks for today
2. A "Follow-ups" section for any meeting action items from the past week
3. An "Inbox Review" section if there are unprocessed inbox items
4. A "Notes" section (empty, for throughout-the-day capture)

Use the daily-note template format with frontmatter: type: daily, date: {{ today }}
