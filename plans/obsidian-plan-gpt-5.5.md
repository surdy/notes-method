# Obsidian Implementation Plan — GPT-5.5

This plan implements the notes method as an Obsidian vault designed for a customer-facing professional: fast capture, inbox zero, customer-centered filing, stream-of-work tracking, and task dashboards.

## 1. Vault structure

I recommend one parent folder for customers instead of putting every customer at vault root. It keeps the vault navigable as the customer list grows while still preserving a dedicated folder per customer.

```text
00 Inbox/
01 Home.md
02 Tasks/
  Active Tasks.md
  Blocked Tasks.md
  Waiting and On Hold.md
03 Customers/
  Acme Corp/
    Acme Corp.md                    # customer index / source of customer state
    Account information/
      Account information.md
      Glossary.md
      Dates and Milestones.md
    Internal meetings/
      2026-05-08 - Acme - Internal account planning.md
    External meetings/
      2026-05-09 - Acme - QBR prep.md
    Streams/
      CRM Migration.md
      Executive Alignment.md
    Assets/
      acme-architecture.pdf
  Globex/
    Globex.md
    Account information/
    Internal meetings/
    External meetings/
    Streams/
    Assets/
04 General/
  People.md
  Reusable notes.md
05 Assets/
  templates/
    Daily Note.md
    Meeting Note.md
    Account Info.md
    Glossary.md
    Dates and Milestones.md
    Stream of Work.md
    Customer Index.md
  data/
    customers.md                    # optional Dataview/base table source
  attachments/
99 Archive/
  Customers/
  Completed Streams/
```

Naming conventions:

- Customer folder: `03 Customers/{Customer Name}/`
- Customer index note: `03 Customers/{Customer Name}/{Customer Name}.md`
- Meeting notes: `YYYY-MM-DD - {Customer} - {Meeting Title}.md`
- Stream notes: human-readable stream name under `Streams/`, e.g. `CRM Migration.md`
- Dashboard notes live outside customer folders so they aggregate globally.

If you strongly prefer the original top-level layout, `03 Customers/Acme Corp/` can be replaced with `Acme Corp/`, but I would not do that once you have more than a handful of customers.

## 2. Plugins

| Plugin | Purpose | Why it belongs in this system |
| --- | --- | --- |
| **Dataview** by Michael Brenan | Query frontmatter, inline fields, and tasks | Main dashboard/query engine for customers, streams, and task rollups. |
| **Tasks** (`obsidian-tasks-plugin`) by Clare Macrae and Ilyas Landikov | Better task syntax, custom statuses, due dates, recurrence | Gives task UX, completion dates, recurring tasks, priorities, and readable task queries. |
| **Templater** by SilentVoid | Dynamic templates and scripts | Creates customer-aware notes, computes file paths, inserts dates, and powers the inbox filing script. |
| **QuickAdd** by Christian B. B. Houmann | Capture commands and macros | One-command creation of meetings, streams, customers, and a `Complete Inbox Note` macro. |
| **Periodic Notes** by Liam Cain | Daily note creation | More flexible than core Daily Notes; stores daily notes in `00 Inbox/`. |
| **Calendar** by Liam Cain | Date navigation | Quick access to daily notes and meeting days. |
| **Auto Periodic Notes** by Jamie Hurst | Automatic background creation of periodic notes | Creates the daily note automatically each morning/open-session when paired with Periodic Notes. |
| **Auto Note Mover** by faru | Rule-based note movement | Useful for simple static routes and safety-net cleanup; dynamic customer routing is better handled by QuickAdd + Templater. |
| **Metadata Menu** by mdelobelle | Controlled metadata editing | Dropdowns for `customer_state`, `stream_status`, `note_type`, etc., reducing typos. |
| **Homepage** by mirnovov | Open dashboard on startup | Opens `01 Home.md` so the vault starts in operating mode. |
| **Advanced URI** by Vinzent | External automation | Optional, but enables macOS Shortcuts/launchd to open/create the daily note at a scheduled time. |
| **Buttons** or **Meta Bind** | In-note command buttons | Optional quality-of-life: buttons like “File this inbox note” or “Create meeting note”. |
| **Note Refactor** by James Lynch | Extract sections into notes | Useful for turning a meeting section into a stream note or follow-up note. |

I would not make **Projects** the core system. For this method, a “project” is really a customer stream of work, and Dataview + frontmatter gives more control. If Obsidian **Bases** is available in your Obsidian version, use it as a visual table layer for customers/streams, but keep Dataview as the portable query layer.

Recommended plugin settings:

- Dataview: enable JavaScript queries only if you are comfortable with local scripts. The plan below mostly uses standard Dataview.
- Tasks: configure custom statuses for `To Do`, `Blocked`, `Done`, `Awaiting Customer`, and `On Hold`.
- Templater: template folder `05 Assets/templates/`; enable folder templates if desired.
- Periodic Notes: daily note folder `00 Inbox/`, date format `YYYY-MM-DD`, template `05 Assets/templates/Daily Note.md`.
- Metadata Menu: define allowed values for customer and stream state fields.

## 3. Templates

### 3.1 Daily note template

Path: `05 Assets/templates/Daily Note.md`

````markdown
---
note_type: daily
created: <% tp.date.now("YYYY-MM-DDTHH:mm") %>
date: <% tp.date.now("YYYY-MM-DD") %>
inbox_status: active
route: daily
---

# <% tp.date.now("YYYY-MM-DD dddd") %>

## Top priorities

- [ ] 

## Customer touches

| Customer | Purpose | Follow-up |
| --- | --- | --- |
|  |  |  |

## Meetings today

```dataview
TABLE start_time AS "Time", customer AS "Customer", stream AS "Stream"
FROM "00 Inbox" OR "03 Customers"
WHERE note_type = "meeting" AND date = this.date
SORT start_time ASC
```

## Notes captured today

- 

## Tasks created today

```dataview
TASK
FROM "00 Inbox" OR "03 Customers"
WHERE created = this.date AND !completed
GROUP BY customer
```
````

Daily notes intentionally begin in Inbox. At the end of the day, either leave them in Inbox until processed or file them to `04 General/Daily/` if you want a daily archive. If you rarely revisit daily notes, keep them in `00 Inbox` only while active and archive completed ones monthly.

### 3.2 Meeting note template

Path: `05 Assets/templates/Meeting Note.md`

```markdown
---
note_type: meeting
created: <% tp.date.now("YYYY-MM-DDTHH:mm") %>
date: <% tp.date.now("YYYY-MM-DD") %>
start_time: 
customer: "[[Acme Corp]]"
customer_id: acme-corp
meeting_type: external # external | internal
stream: "[[CRM Migration]]"
stream_id: crm-migration
attendees: []
inbox_status: active # active | ready_to_file | filed
route: customer-meeting
---

# <% tp.date.now("YYYY-MM-DD") %> - Acme Corp - Meeting title

## Purpose


## Attendees

- 

## Notes


## Decisions

- 

## Tasks

- [ ] Follow up on decision owner [customer:: [[Acme Corp]]] [stream:: [[CRM Migration]]] [task_status:: To Do] 📅 2026-05-15
- [-] Waiting on customer data export [customer:: [[Acme Corp]]] [stream:: [[CRM Migration]]] [task_status:: Blocked]

## Links

- Customer: [[Acme Corp]]
- Stream: [[CRM Migration]]

## Filing checklist

- [ ] Tasks have customer and stream fields where relevant
- [ ] Decisions summarized
- [ ] `inbox_status` set to `ready_to_file`
```

### 3.3 Account info template

Path: `05 Assets/templates/Account Info.md`

```markdown
---
note_type: account_info
customer: "[[Acme Corp]]"
customer_id: acme-corp
account_owner: 
industry: 
segment: 
renewal_date: 
health: unknown
---

# Account information — Acme Corp

## Account team

| Role | Name | Notes |
| --- | --- | --- |
| AE |  |  |
| SE |  |  |
| CSM |  |  |

## Business context


## Technical environment


## Stakeholders

| Name | Role | Influence | Notes |
| --- | --- | --- | --- |
|  |  |  |  |

## Links

- Customer index: [[Acme Corp]]
- Glossary: [[Glossary]]
- Dates: [[Dates and Milestones]]
```

### 3.4 Glossary template

Path: `05 Assets/templates/Glossary.md`

```markdown
---
note_type: glossary
customer: "[[Acme Corp]]"
customer_id: acme-corp
---

# Glossary — Acme Corp

| Term | Meaning | Source / context |
| --- | --- | --- |
|  |  |  |
```

### 3.5 Dates and milestones template

Path: `05 Assets/templates/Dates and Milestones.md`

```markdown
---
note_type: milestones
customer: "[[Acme Corp]]"
customer_id: acme-corp
---

# Dates and Milestones — Acme Corp

| Date | Type | Description | Stream | Status |
| --- | --- | --- | --- | --- |
| 2026-06-30 | Renewal | Contract renewal |  | Upcoming |
|  | Launch |  | [[CRM Migration]] | Planned |
```

### 3.6 Stream of work template

Path: `05 Assets/templates/Stream of Work.md`

````markdown
---
note_type: stream
customer: "[[Acme Corp]]"
customer_id: acme-corp
stream_id: crm-migration
stream_status: In Progress # In Progress | Blocked | Done | Awaiting Customer | On Hold
owner: 
created: <% tp.date.now("YYYY-MM-DD") %>
target_date: 
priority: medium
inbox_status: filed
---

# CRM Migration

## Outcome

What success looks like:

## Current status

- Status: `In Progress`
- Next customer touch:
- Main risk:

## Tasks in this stream

```dataview
TASK
FROM "03 Customers"
WHERE !completed AND stream = this.file.link
WHERE task_status != "Done"
SORT due ASC
```

## Blocked tasks

```dataview
TASK
FROM "03 Customers"
WHERE !completed AND stream = this.file.link
WHERE task_status = "Blocked" OR status = "-"
```

## Meetings

```dataview
TABLE date AS "Date", meeting_type AS "Type", file.link AS "Note"
FROM "03 Customers"
WHERE note_type = "meeting" AND stream = this.file.link
SORT date DESC
```

## Decisions

- 

## Notes


## Archive criteria

- [ ] Outcome delivered or explicitly abandoned
- [ ] Open tasks moved or closed
- [ ] Final summary added
````

### 3.7 Customer index template

Path: `05 Assets/templates/Customer Index.md`

````markdown
---
note_type: customer
customer: Acme Corp
customer_id: acme-corp
customer_state: Active # Active | On Hold | Temp | Inactive
primary_contact: 
account_info: "[[Account information]]"
created: <% tp.date.now("YYYY-MM-DD") %>
---

# Acme Corp

> [!summary]
> State: **Active**  
> Main account note: [[Account information]]

## Active streams

```dataview
TABLE stream_status AS "Status", target_date AS "Target", priority AS "Priority"
FROM "03 Customers/Acme Corp/Streams"
WHERE note_type = "stream" AND stream_status != "Done"
SORT priority DESC, target_date ASC
```

## Open tasks

```dataview
TASK
FROM "03 Customers/Acme Corp"
WHERE !completed
WHERE task_status != "Done" AND task_status != "Blocked"
GROUP BY stream
```

## Blocked / waiting

```dataview
TASK
FROM "03 Customers/Acme Corp"
WHERE !completed
WHERE task_status = "Blocked" OR task_status = "Awaiting Customer" OR task_status = "On Hold"
GROUP BY task_status
```

## Recent meetings

```dataview
TABLE date AS "Date", meeting_type AS "Type", stream AS "Stream"
FROM "03 Customers/Acme Corp"
WHERE note_type = "meeting"
SORT date DESC
LIMIT 10
```

## Core notes

- [[Account information]]
- [[Glossary]]
- [[Dates and Milestones]]
````

## 4. Task model

Use normal Markdown tasks plus Dataview inline fields. Tasks plugin provides task operations; Dataview provides reliable metadata queries.

### Status encoding

Configure Tasks custom statuses:

| Meaning | Markdown checkbox | Dataview `status` | `task_status` field | Treated as active? |
| --- | --- | --- | --- | --- |
| To Do | `[ ]` | space | `To Do` | Yes |
| Blocked | `[-]` | `-` | `Blocked` | No; appears in blocked view |
| Done | `[x]` | `x` | `Done` | No |
| Awaiting Customer | `[>]` | `>` | `Awaiting Customer` | Yes, but also appears in waiting view |
| On Hold | `[/]` | `/` | `On Hold` | No, unless you choose to review holds weekly |

Task syntax:

```markdown
- [ ] Send Acme migration checklist [customer:: [[Acme Corp]]] [stream:: [[CRM Migration]]] [task_status:: To Do] 📅 2026-05-15
- [>] Wait for Acme to send SSO metadata [customer:: [[Acme Corp]]] [stream:: [[CRM Migration]]] [task_status:: Awaiting Customer] 📅 2026-05-20
- [-] Cannot schedule cutover until security approval arrives [customer:: [[Acme Corp]]] [stream:: [[CRM Migration]]] [task_status:: Blocked]
- [/] Revisit phase 2 after renewal [customer:: [[Acme Corp]]] [stream:: [[Executive Alignment]]] [task_status:: On Hold]
- [x] Send recap email [customer:: [[Acme Corp]]] [stream:: [[CRM Migration]]] [task_status:: Done] ✅ 2026-05-08
```

Rules:

1. Every customer-related task gets `[customer:: [[Customer Name]]]`.
2. Every stream-related task gets `[stream:: [[Stream Name]]]`.
3. Every task gets `[task_status:: ...]` even if the checkbox symbol also implies the status. The redundancy makes queries readable and prevents custom status ambiguity.
4. Due dates use Tasks plugin emoji syntax: `📅 YYYY-MM-DD`.
5. Do not encode customer only as a tag. Links are better because they create backlinks and survive renames.

### Active task aggregation

Path: `02 Tasks/Active Tasks.md`

````markdown
# Active Tasks

```dataview
TASK
FROM "00 Inbox" OR "03 Customers" OR "04 General"
WHERE !completed
WHERE task_status != "Blocked"
WHERE task_status != "Done"
WHERE task_status != "On Hold"
GROUP BY customer
SORT due ASC, file.mtime DESC
```
````

If you want Tasks plugin rendering instead:

```tasks
not done
status.name does not include Blocked
status.name does not include On Hold
path includes 03 Customers
sort by due
sort by path
```

Use Dataview when you want grouping by customer/stream; use Tasks when you want the best task interaction controls.

### Blocked task aggregation

Path: `02 Tasks/Blocked Tasks.md`

````markdown
# Blocked Tasks

```dataview
TASK
FROM "00 Inbox" OR "03 Customers" OR "04 General"
WHERE !completed
WHERE task_status = "Blocked" OR status = "-"
GROUP BY customer
SORT file.mtime DESC
```
````

### Waiting and on-hold review

Path: `02 Tasks/Waiting and On Hold.md`

````markdown
# Waiting and On Hold

## Awaiting customer

```dataview
TASK
FROM "03 Customers"
WHERE !completed
WHERE task_status = "Awaiting Customer" OR status = ">"
GROUP BY customer
SORT due ASC
```

## On hold

```dataview
TASK
FROM "03 Customers"
WHERE !completed
WHERE task_status = "On Hold" OR status = "/"
GROUP BY customer
SORT file.mtime DESC
```
````

## 5. Inbox workflow and auto-move

### Recommended workflow

1. Every new note is created in `00 Inbox/`.
2. The template assigns:
   - `note_type`
   - `customer`
   - `customer_id`
   - `stream` when relevant
   - `route`
   - `inbox_status: active`
3. When the note is processed, change `inbox_status` to `ready_to_file`.
4. Run QuickAdd command **Complete Inbox Note** from a hotkey/button.
5. The Templater script computes the destination and moves the note.
6. The script changes `inbox_status` to `filed`.

This is better than pure Auto Note Mover because the destination depends on customer and note type. Auto Note Mover is good for static rules; customer-specific dynamic paths need a script.

### Destination rules

| `note_type` / fields | Destination |
| --- | --- |
| `meeting`, `meeting_type: external` | `03 Customers/{customer}/External meetings/` |
| `meeting`, `meeting_type: internal` | `03 Customers/{customer}/Internal meetings/` |
| `account_info` | `03 Customers/{customer}/Account information/Account information.md` |
| `glossary` | `03 Customers/{customer}/Account information/Glossary.md` |
| `milestones` | `03 Customers/{customer}/Account information/Dates and Milestones.md` |
| `stream` | `03 Customers/{customer}/Streams/` |
| `daily` | `04 General/Daily/YYYY/` after processed, or leave in Inbox until weekly review |
| no customer | `04 General/` |

### Templater/QuickAdd filing script sketch

Create a Templater user script such as `05 Assets/scripts/file-inbox-note.js` if you choose to implement this later:

```javascript
module.exports = async (tp) => {
  const app = tp.app;
  const file = app.workspace.getActiveFile();
  if (!file || !file.path.startsWith("00 Inbox/")) return;

  const cache = app.metadataCache.getFileCache(file);
  const fm = cache?.frontmatter ?? {};
  if (fm.inbox_status !== "ready_to_file") return;

  const customerName = String(fm.customer ?? "").replace(/^\[\[/, "").replace(/\]\]$/, "");
  const noteType = fm.note_type;
  const meetingType = fm.meeting_type;

  let folder = "04 General";
  if (customerName && noteType === "meeting") {
    folder = `03 Customers/${customerName}/${meetingType === "internal" ? "Internal meetings" : "External meetings"}`;
  } else if (customerName && noteType === "stream") {
    folder = `03 Customers/${customerName}/Streams`;
  } else if (customerName && ["account_info", "glossary", "milestones"].includes(noteType)) {
    folder = `03 Customers/${customerName}/Account information`;
  } else if (noteType === "daily") {
    const year = (fm.date ?? file.basename).slice(0, 4);
    folder = `04 General/Daily/${year}`;
  }

  await app.vault.createFolder(folder).catch(() => {});
  await app.fileManager.processFrontMatter(file, frontmatter => {
    frontmatter.inbox_status = "filed";
  });
  await app.fileManager.renameFile(file, `${folder}/${file.name}`);
};
```

QuickAdd macro:

1. Run Templater user script `file-inbox-note.js`.
2. Optional: open `01 Home.md` after filing.

Optional Auto Note Mover safety-net rules:

- Notes containing `#route/general` → `04 General/`
- Notes containing `#route/assets` → `05 Assets/attachments/`
- Notes containing `#archive` → `99 Archive/`

## 6. Daily notes automation

Use **Periodic Notes** + **Auto Periodic Notes** + **Templater**.

Settings:

```text
Periodic Notes daily format: YYYY-MM-DD
Daily note folder: 00 Inbox
Daily note template: 05 Assets/templates/Daily Note.md
Auto Periodic Notes: create/open today's daily note on startup
Homepage: open 01 Home.md on startup after daily note creation, or pin Home as the main tab
```

For true “every morning at 7:00” generation even if Obsidian is closed, add **Advanced URI** and a macOS Shortcut/launchd job that opens an Obsidian URI command for Periodic Notes. The Obsidian-native approach creates the daily note when Obsidian starts; the OS-scheduled approach creates it at a clock time.

I would start with startup creation. It is simpler and avoids automation fragility.

## 7. Customer state

Recommendation: store customer state in frontmatter on the **customer index note**, not the account info note.

Reasoning:

- Customer state is operational metadata about your relationship, not account facts.
- The customer index is the natural dashboard/MOC for that customer.
- It lets dashboards query one note per customer instead of parsing account-info files buried one level deeper.
- Account information can remain focused on people, business context, technical facts, and dates.

Customer index frontmatter:

```yaml
---
note_type: customer
customer: Acme Corp
customer_id: acme-corp
customer_state: Active # Active | On Hold | Temp | Inactive
primary_contact: Jane Doe
---
```

Customer state dashboard:

```dataview
TABLE customer_state AS "State", primary_contact AS "Primary Contact", file.link AS "Customer"
FROM "03 Customers"
WHERE note_type = "customer"
SORT customer_state ASC, customer ASC
```

Active customers only:

```dataview
TABLE file.link AS "Customer", primary_contact AS "Primary", account_owner AS "Owner"
FROM "03 Customers"
WHERE note_type = "customer" AND customer_state = "Active"
SORT customer ASC
```

On-hold or inactive customers:

```dataview
TABLE customer_state AS "State", file.mtime AS "Last Updated"
FROM "03 Customers"
WHERE note_type = "customer" AND contains(["On Hold", "Inactive"], customer_state)
SORT customer_state ASC, file.mtime DESC
```

Use Metadata Menu to make `customer_state` a dropdown. This prevents variants like `active`, `Active customer`, or `on-hold` from breaking filters.

## 8. Streams of work

A stream is a note under the customer folder:

```text
03 Customers/Acme Corp/Streams/CRM Migration.md
```

It has frontmatter:

```yaml
---
note_type: stream
customer: "[[Acme Corp]]"
customer_id: acme-corp
stream_id: crm-migration
stream_status: In Progress
owner: Surdy
target_date: 2026-06-30
priority: high
---
```

Status values:

- `In Progress`
- `Blocked`
- `Done`
- `Awaiting Customer`
- `On Hold`

Key principle: stream status and task status are independent. A stream can be `Blocked` while still having active internal tasks, or `Awaiting Customer` while you still have prep work.

Tasks can live:

1. Directly in the stream note.
2. In meeting notes, linked back to the stream with `[stream:: [[CRM Migration]]]`.
3. In daily notes, if captured ad hoc, but they should still include customer and stream metadata.

Stream self-dashboard:

```dataview
TASK
FROM "03 Customers/Acme Corp" OR "00 Inbox"
WHERE stream = this.file.link
WHERE !completed
GROUP BY task_status
SORT due ASC
```

Streams by status:

```dataview
TABLE customer AS "Customer", stream_status AS "Status", target_date AS "Target", priority AS "Priority"
FROM "03 Customers"
WHERE note_type = "stream" AND stream_status != "Done"
SORT customer ASC, priority DESC, target_date ASC
```

Completed streams should move to `99 Archive/Completed Streams/{Customer}/` only after the stream note contains a final summary and open tasks are closed or moved.

## 9. Dashboards

### 9.1 Home dashboard

Path: `01 Home.md`

````markdown
# Home

## Inbox

```dataview
TABLE note_type AS "Type", customer AS "Customer", stream AS "Stream", file.mtime AS "Updated"
FROM "00 Inbox"
SORT file.mtime DESC
```

## Due / active tasks

```dataview
TASK
FROM "00 Inbox" OR "03 Customers"
WHERE !completed
WHERE task_status != "Blocked" AND task_status != "On Hold" AND task_status != "Done"
SORT due ASC
LIMIT 25
```

## Blocked tasks

```dataview
TASK
FROM "03 Customers"
WHERE !completed
WHERE task_status = "Blocked" OR status = "-"
GROUP BY customer
```

## Active customers

```dataview
TABLE file.link AS "Customer", primary_contact AS "Primary", file.mtime AS "Updated"
FROM "03 Customers"
WHERE note_type = "customer" AND customer_state = "Active"
SORT file.mtime DESC
```

## Active streams

```dataview
TABLE customer AS "Customer", stream_status AS "Status", target_date AS "Target"
FROM "03 Customers"
WHERE note_type = "stream" AND stream_status != "Done"
SORT target_date ASC
LIMIT 20
```
````

### 9.2 Active tasks dashboard

Path: `02 Tasks/Active Tasks.md`

```dataview
TASK
FROM "00 Inbox" OR "03 Customers" OR "04 General"
WHERE !completed
WHERE task_status != "Blocked"
WHERE task_status != "Done"
WHERE task_status != "On Hold"
GROUP BY customer
SORT due ASC
```

### 9.3 Blocked tasks dashboard

Path: `02 Tasks/Blocked Tasks.md`

```dataview
TASK
FROM "00 Inbox" OR "03 Customers" OR "04 General"
WHERE !completed
WHERE task_status = "Blocked" OR status = "-"
GROUP BY customer
SORT file.mtime DESC
```

### 9.4 Customers by state

Path: `03 Customers/Customers by State.md` or `01 Home.md` section

```dataview
TABLE rows.file.link AS "Customers"
FROM "03 Customers"
WHERE note_type = "customer"
GROUP BY customer_state
SORT key ASC
```

### 9.5 Per-customer overview

This is the customer index note. Its core queries are:

```dataview
TABLE stream_status AS "Status", target_date AS "Target", priority AS "Priority"
FROM "03 Customers/Acme Corp/Streams"
WHERE note_type = "stream" AND stream_status != "Done"
SORT priority DESC, target_date ASC
```

```dataview
TASK
FROM "03 Customers/Acme Corp"
WHERE !completed
GROUP BY stream
SORT due ASC
```

```dataview
TABLE date AS "Date", meeting_type AS "Type", stream AS "Stream"
FROM "03 Customers/Acme Corp"
WHERE note_type = "meeting"
SORT date DESC
LIMIT 10
```

### 9.6 Inbox zero dashboard

Path: `00 Inbox/Inbox.md` or Home section

```dataview
TABLE note_type AS "Type", inbox_status AS "Status", customer AS "Customer", route AS "Route", file.mtime AS "Updated"
FROM "00 Inbox"
WHERE file.name != "Inbox"
SORT inbox_status ASC, file.mtime DESC
```

## 10. Refinements and open questions

### Opinionated refinements

1. **Use a customer index as the source of truth for customer state.** This directly addresses the open question. Account info is still important, but it should not be the operational control plane.
2. **Separate `Blocked`, `Awaiting Customer`, and `On Hold`.** They sound similar but require different behavior:
   - Blocked: something prevents progress and needs escalation or dependency removal.
   - Awaiting Customer: waiting on external response; follow up by due date.
   - On Hold: intentionally paused; review weekly/monthly.
3. **Make Inbox filing metadata-driven.** Avoid drag-and-drop filing. A note is fileable only when frontmatter is complete and `inbox_status: ready_to_file`.
4. **Treat streams as the main unit of customer work.** Meeting notes are evidence/history; stream notes are the operating surface.
5. **Use links, not tags, for customer and stream assignment.** Tags are fine for broad categories, but links are better for customer/stream relationships.
6. **Keep task status independent from stream status.** Do not auto-close tasks just because a stream is done; require a final stream review.

### Gaps / edge cases

| Area | Recommendation |
| --- | --- |
| Recurring tasks | Use Tasks plugin recurrence syntax for regular follow-ups, e.g. `🔁 every week 📅 2026-05-15`. |
| Snoozed tasks | Use `On Hold` plus a due date, or Tasks scheduled date `⏳ YYYY-MM-DD`. |
| Mobile capture | Create a QuickAdd “Inbox capture” command that only asks for title/body and always writes to `00 Inbox/`. Add metadata later. |
| Attachments | Store customer-specific files in `03 Customers/{Customer}/Assets/`; generic assets in `05 Assets/attachments/`. |
| Archiving customers | Change `customer_state: Inactive`, then optionally move folder to `99 Archive/Customers/`. Dashboards should filter by state, not path, so archived data remains queryable if included. |
| Renaming customers | Because tasks use links, Obsidian can update links. Also update `customer_id` manually only if needed; prefer stable slugs. |
| Confidential notes | Add `sensitivity: internal` or `sensitivity: customer-shareable` frontmatter if you often reuse notes for external comms. |
| Customer aliases | Add `aliases: [Acme, Acme Inc]` to the customer index note for search/linking. |
| Search | Use consistent names and frontmatter instead of relying on folder search. |
| Reviews | Add weekly review note/query for `On Hold`, `Awaiting Customer`, stale streams, and Inbox items older than 3 days. |

### Weekly review dashboard snippet

```dataview
TABLE customer AS "Customer", stream_status AS "Status", file.mtime AS "Last Updated"
FROM "03 Customers"
WHERE note_type = "stream" AND stream_status != "Done"
WHERE file.mtime < date(today) - dur(14 days)
SORT file.mtime ASC
```

### Open questions to decide later

1. Do you want daily notes archived to `04 General/Daily/` or kept permanently in Inbox until manually emptied?
2. Should `Awaiting Customer` tasks appear in the main active task dashboard? I recommend yes, sorted by due date, plus a separate waiting view.
3. Do you want customer folders hidden when inactive, or just filtered out of dashboards? I recommend filtering first; move only when truly archived.
4. Should meeting notes be created from calendar events? If yes, consider adding a calendar integration later, but do not make the first version depend on it.

## 11. Implementation order

1. **Create folders**
   - `00 Inbox/`
   - `02 Tasks/`
   - `03 Customers/`
   - `04 General/`
   - `05 Assets/templates/`
   - `05 Assets/data/`
   - `05 Assets/attachments/`
   - `99 Archive/`

2. **Install core plugins first**
   - Dataview
   - Tasks
   - Templater
   - QuickAdd
   - Periodic Notes
   - Calendar
   - Metadata Menu

3. **Configure Tasks statuses**
   - `[ ]` To Do
   - `[-]` Blocked
   - `[x]` Done
   - `[>]` Awaiting Customer
   - `[/]` On Hold

4. **Configure Periodic Notes**
   - Daily folder: `00 Inbox/`
   - Format: `YYYY-MM-DD`
   - Template: `05 Assets/templates/Daily Note.md`

5. **Create templates**
   - Daily note
   - Meeting note
   - Account info
   - Glossary
   - Dates and milestones
   - Stream of work
   - Customer index

6. **Create first customer manually**
   - `03 Customers/Acme Corp/Acme Corp.md`
   - `Account information/`
   - `Internal meetings/`
   - `External meetings/`
   - `Streams/`
   - `Assets/`

7. **Create dashboards**
   - `01 Home.md`
   - `02 Tasks/Active Tasks.md`
   - `02 Tasks/Blocked Tasks.md`
   - `02 Tasks/Waiting and On Hold.md`
   - optional `03 Customers/Customers by State.md`

8. **Add Metadata Menu field definitions**
   - `note_type`
   - `customer_state`
   - `stream_status`
   - `task_status`
   - `meeting_type`
   - `inbox_status`

9. **Set up QuickAdd captures**
   - New customer
   - New external meeting
   - New internal meeting
   - New stream
   - Quick inbox capture
   - Complete Inbox Note

10. **Implement inbox filing automation**
    - Start with the QuickAdd + Templater filing script.
    - Add Auto Note Mover only for static routing/archival rules.
    - Test with one meeting note and one stream note before using broadly.

11. **Enable startup flow**
    - Homepage opens `01 Home.md`.
    - Periodic Notes/Auto Periodic Notes creates today’s daily note in `00 Inbox/`.
    - Optional Advanced URI + macOS automation if you require clock-time creation.

12. **Run a one-week pilot**
    - Use the system for one active customer and one temporary customer.
    - Watch for metadata friction.
    - Tighten templates before scaling to all customers.

13. **Scale to all customers**
    - Create customer index notes for every customer.
    - Set `customer_state` for each.
    - Move existing notes into the customer folder structure.
    - Add streams only for active bodies of work; do not create empty stream notes for everything.
