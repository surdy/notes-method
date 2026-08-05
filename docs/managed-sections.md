# Managed sections

A **managed section** is a machine-owned region inside a human-owned note,
delimited by a pair of HTML-comment markers (ADR 0025 Decision 5):

```markdown
<!-- notesmith:section:begin briefing/meetings -->
### Today's meetings
- 09:30–10:00 [[2026-08-04 0930 Acme sync]] (external)
<!-- notesmith:section:end briefing/meetings -->
```

The convention lets automation (agent jobs, connectors, scripts) refresh its
part of a note repeatedly without ever disturbing what a human wrote around
it. It is generic — nothing about it is specific to daily notes; any note can
carry any number of managed sections.

## The contract

- **Markers.** A section is the text between
  `<!-- notesmith:section:begin <id> -->` and
  `<!-- notesmith:section:end <id> -->`, each on its own line. The `<id>` is
  free-form; namespace it by the flow that owns it (`briefing/meetings`,
  `briefing/email`, `sync/status`) so different automations never collide.
- **Replace in place.** A writer refreshing a section replaces only the
  content *between* its marker pair — the markers stay, and a re-run must not
  duplicate the section. Re-running a job any number of times converges to
  the same note shape (idempotent).
- **Outside is inviolable.** Content outside the markers is human-owned and
  must be byte-identical before and after a managed-section update. A writer
  never reorders, reformats, or "cleans up" anything else in the note.
- **Missing pair.** If the note exists but the marker pair for a section does
  not, the writer appends the whole marked block (markers + content) at the
  end of the note rather than guessing an insertion point. It never invents
  markers around existing human text.
- **Ownership of the interior.** Everything between the markers is
  machine-owned: a human edit inside a managed section is legitimate until
  the next run, which overwrites it. Durable human content belongs outside
  the markers.

## How it is implemented

There is no core enforcement engine: the convention lives in vault config and
agent guidance. Templates ship empty marker pairs (see the work-notes kit's
`daily` template), prompts instruct the agent to read the note, splice the new
section content between the markers, and write the result back via
`update_note`, and the vault `skill.md` teaches every agent session the
contract above. Note-level git history (the `[git]` timers) is the undo
mechanism for a bad automated run.

The first user of the convention is the morning daily-briefing agent job
(issue #288): a `[[jobs]]` entry renders the `daily-note` prompt and fills the
daily note's `briefing/*` sections while Focus/Notes/Tasks stay human-owned.
See `docs/cli.md` (`job` and `ai prompt`) for how agent jobs run, and
`docs/example-work-notes-kit.md` for the surrounding kit.
