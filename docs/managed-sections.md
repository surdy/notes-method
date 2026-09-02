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

The marker convention stays vault-level, but **replacing a section's interior
is a deterministic core operation** — prompt guidance alone could not hold the
"outside is inviolable" line (see the amendment to
[ADR 0025](adr/0025-work-system-integrations.md): a compliant agent still
stripped trailing spaces from human text, and the save pipeline restamped
`updated:`).

- **Core.** `notesmith_vault::update_managed_section` is pure string surgery:
  it finds the one marker pair, replaces only the byte range between the
  marker lines, and copies every other byte through unchanged. Content is
  written verbatim, with a single `\n` appended when it does not already end
  in one so the end marker keeps its own line. Re-running with identical
  content is a byte-level no-op.
- **HTTP.** `POST /api/v/{vault}/notes-section/{path...}` — see
  [`docs/http-api.md`](http-api.md).
- **MCP.** The `update_managed_section` tool, on the read-write `/mcp/{vault}`
  surface only (rejected on `/mcp-ro/{vault}` like every other write) — see
  [`docs/mcp.md`](mcp.md).
- **Structured refusals.** Duplicate begin or end markers for the same id, an
  inverted pair (end before begin), a begin without an end, an end without a
  begin, a missing pair with `append_if_missing` off, and replacement content
  that itself contains a marker line (which would corrupt the section for
  every later run) all return a coded error and write nothing. The note is
  never partially rewritten.
- **Conflict detection.** The write is atomic (temp file + rename) and guarded
  by the same content hash the other note writes use, so a concurrent human
  edit produces a conflict instead of a silent overwrite.

### No automatic `updated:` on managed-section writes

Managed-section writes are the one write path that does **not** run the save
pipeline. Every other write stamps `updated:`, sorts frontmatter keys, and
trims trailing whitespace; this one does none of that. "Outside the markers is
inviolable" includes the YAML frontmatter — a machine refreshing its own
region is not a human editing the note, so it must not claim the note was
modified, and it must not reformat human bytes on the way past. A caller that
*wants* the note's `updated:` refreshed makes that a separate, explicit
`PATCH /notes/{path...}`.

Templates still ship empty marker pairs (see the work-notes kit's `daily`
template), the `daily-note` prompt tells the agent to call the tool once per
section rather than rewrite the note, and the vault `skill.md` teaches every
agent session the contract above. Note-level git history (the `[git]` timers)
remains the undo mechanism for a bad automated run.

The first user of the convention is the morning daily-briefing agent job
(issue #288): a `[[jobs]]` entry renders the `daily-note` prompt and fills the
daily note's `briefing/*` sections while Focus/Notes/Tasks stay human-owned.
See `docs/cli.md` (`job` and `ai prompt`) for how agent jobs run, and
`docs/example-work-notes-kit.md` for the surrounding kit.
