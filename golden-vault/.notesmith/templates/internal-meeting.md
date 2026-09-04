---
name: internal-meeting
description: "New internal meeting — prefilled from the calendar event in progress; leave the title blank to take the calendar's"
output_path: "Inbox/{{ meeting_date or date }} - {{ meeting_slug or title or 'Untitled' }}.md"
prompts:
  - { name: title, type: text, required: false }
context_queries:
  calendar_events: >-
    SELECT n.path AS path,
    MAX(CASE WHEN f.key = 'event_id' THEN f.value END) AS event_id,
    MAX(CASE WHEN f.key = 'start' THEN f.value END) AS start,
    MAX(CASE WHEN f.key = 'end' THEN f.value END) AS end,
    MAX(CASE WHEN f.key = 'audience' THEN f.value END) AS audience,
    MAX(CASE WHEN f.key = 'organizer' THEN f.value END) AS organizer
    FROM v_notes n
    JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path
    WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event')
    GROUP BY n.path
    HAVING substr(MAX(CASE WHEN f.key = 'start' THEN f.value END), 1, 10)
    BETWEEN date('now', 'localtime', '-1 day')
    AND date('now', 'localtime', '+1 day')
  calendar_event_members: >-
    SELECT note_path AS path, key AS key, ordinal AS ordinal, value AS value
    FROM v_field_values
    WHERE key IN ('attendees', 'customers')
    AND note_path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event')
    AND note_path IN (SELECT note_path FROM v_fields WHERE key = 'start'
    AND substr(value, 1, 10) BETWEEN date('now', 'localtime', '-1 day')
    AND date('now', 'localtime', '+1 day'))
    ORDER BY note_path, key, ordinal
pre_render_hook: ".notesmith/scripts/meeting-prefill.sh"
---
---
kind: meeting
audience: internal
date: {{ meeting_date or date }}
customers: []
streams: []
attendees: []
{% if event_id %}event_id: "{{ event_id }}"
event: "[[{{ event_link }}]]"
{% endif %}---

# {{ meeting_date or date }} — {{ meeting_title or title or 'Untitled' }}
{% if event_matched %}
> {{ meeting_time }}{% if event_organizer %} · organized by {{ event_organizer }}{% endif %} · from [[{{ event_link }}]]
{% endif %}
{% if event_attendees %}## Attendees

<!-- From the calendar. Replace with "[[Person]]" wikilinks in the attendees field during enrichment. -->
{% for address in event_attendees %}- {{ address }}
{% endfor %}
{% endif %}## Notes

## Decisions

## Tasks

- [ ]
