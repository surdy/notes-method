# Using Fact Memory

Fact memory is a curated set of atomic Markdown notes that an agent can consult
across future conversations. It complements the normal wiki; it does not replace
notes, links, search, or ADRs.

The architecture is defined in
[ADR 0021](adr/0021-fact-memory-over-markdown-notes.md). Specialized memory MCP
tools are tracked by [#203](https://github.com/surdy/notes-method/issues/203).
The first shipped slice is read-only `memory_recall`; write/update/supersede/
delete flows still use ordinary note operations, templates, SQL, and hybrid
vault search.

## The central question

Before persisting information, decide:

> Should an agent reliably know this next time, should a person be able to read
> and understand it later, or both?

| Destination | Use when |
|---|---|
| **Fact** | One atomic, durable claim should affect future agent behavior or answers |
| **Wiki** | The information needs explanation, history, evidence, procedures, or normal browsing |
| **Both** | A concise operational claim matters repeatedly and richer detail belongs in a canonical note |
| **Session only** | The information is temporary, speculative, low-value, or secret |

Memory answers **"what should the agent reliably know next time?"** The wiki
answers **"what should be documented and understood?"**

## How to ask the agent

Use explicit language when the destination is clear:

| Prompt | Expected route |
|---|---|
| `Remember that I prefer concise commit messages.` | Fact |
| `Document why Notesmith uses HTTP MCP for Copilot.` | Wiki/ADR |
| `Remember that wireless ADB survives reboot, and document how we proved it.` | Both |
| `Use port 8080 for this test, but do not save it.` | Session only |

When you want the agent to decide, use neutral wording:

```text
Save this for later. Decide whether it belongs as a fact, a wiki note, or both:
...
```

The agent should apply the routing rubric without interrupting unless the
choice is consequential. Existing write-permission previews remain the final
opportunity to reject the proposed route.

## Fact shape

The dogfood vault stores one claim per file under `facts/`:

```markdown
---
type: fact
title: Never auto-capitalize text inputs
description: Never auto-capitalize text inputs.
scope: user
subject: ""
certainty: explicit
source: User statement
status: active
confirmed: 2026-07-10
supersedes: ""
tags: [fact, ui-preference]
---

# Never auto-capitalize text inputs

Never auto-capitalize text inputs; set `autoCapitalize="off"`.
```

Keep the body atomic. If it grows into background, examples, steps, or
reasoning, move that material into a normal note and link it through `source`
or `subject`.

### Field guidance

| Field | Guidance |
|---|---|
| `title` | Short human label, not the full history |
| `description` | The claim in one sentence; used by structured recall |
| `scope` | `user` for cross-workspace preferences; `vault:<name>` for workspace-specific facts |
| `subject` | Optional wikilink to the person, device, project, or topic |
| `certainty` | `explicit`, `observed`, or `inferred` |
| `source` | User statement, wikilink, URL, issue, transcript, or other evidence |
| `status` | `active`, `superseded`, or `retracted` |
| `confirmed` | Date the claim was last verified |
| `supersedes` | Optional wikilink to the replaced fact |

Use `observed` only with a cited source. Use `inferred` only after the user
explicitly confirms the inference. Never silently promote an agent guess into
durable memory.

## Recommended agent workflow

### Recording

1. Read the vault's `.notesmith/memory-index.md`.
2. Classify the input as fact, wiki, both, or session-only.
3. Search `facts/` and the likely wiki segment before writing.
4. Update an exact duplicate instead of creating another file.
5. Use the `fact` template for a new fact.
6. Preserve provenance and use the appropriate certainty.
7. Re-read the result and verify it is one claim.

For **Both**, write the canonical wiki note first. Then create a short fact
whose `source` or `subject` links to that note. Do not duplicate the full wiki
content in the fact.

### Recall

When preferences, identity, environment, or prior choices may matter:

1. Start with `memory_recall(query, scope?, limit?)`.
2. When you know the active workspace scope, pass it so recall includes
   `scope: user` plus matching `scope: vault:<name>` facts.
3. Open the returned fact rather than trusting only the snippet.
4. Follow `source` when explanation or evidence is required.
5. Search the broader wiki when fact recall is insufficient.

Do not inject the entire fact collection into every prompt. Dynamic recall
keeps context focused and prevents stale or unrelated preferences from
dominating the conversation.

## Updating and correcting facts

### Confirming

When an active fact remains true, update `confirmed`. Avoid rewriting the
claim merely to refresh the timestamp.

### Superseding

When a new claim replaces an old one:

1. Create or update the replacement fact.
2. Mark the old fact `status: superseded`.
3. Link the old and new facts through `supersedes` and the note bodies.
4. Keep the old fact available for provenance, but exclude it from normal recall.

Example:

```text
I no longer prefer tea in the morning; remember that I now prefer coffee.
```

The agent should supersede the tea fact, not retain two active preferences.

### Retracting

Use `status: retracted` when a claim was wrong rather than merely outdated.
Hard-delete only accidental entries or sensitive information that should not
remain in file or git history.

## What not to store as facts

- Passwords, API keys, pairing codes, or other secrets.
- Temporary ports, paths, or debugging state.
- Whole meeting transcripts or long procedures.
- Speculation that the user has not confirmed.
- Large copies of information already owned by a canonical note.
- Facts easily derived from the active code or note unless repeated recall has
  demonstrated real value.

Prefer a small, high-signal collection over capturing everything. Memory that
is noisy or contradictory is worse than normal vault search.

## Using the current dogfood implementation

The personal `memory` vault currently provides:

- a `facts/` segment and `fact` template;
- the routing and lifecycle rules in `.notesmith/memory-index.md`;
- ordinary MCP note, template, SQL, lexical, and hybrid-search tools;
- read-only MCP `memory_recall` over active non-example fact notes;
- a schema example tagged `example`;
- real facts stored as normal Markdown notes.

Start a **New Chat** after changing `.notesmith/skill.md` or the memory index,
because the skill is injected when the ACP session starts.

An agent can still list active facts with SQL when it needs custom reporting:

```sql
SELECT n.path, n.title, d.value AS claim, s.value AS scope
FROM v_notes n
JOIN v_fields t ON t.vault_name=n.vault_name AND t.note_path=n.path
  AND t.key='type' AND t.value='fact'
LEFT JOIN v_fields d ON d.vault_name=n.vault_name AND d.note_path=n.path
  AND d.key='description'
LEFT JOIN v_fields s ON s.vault_name=n.vault_name AND s.note_path=n.path
  AND s.key='scope'
LEFT JOIN v_fields st ON st.vault_name=n.vault_name AND st.note_path=n.path
  AND st.key='status'
WHERE COALESCE(st.value, 'active') = 'active'
ORDER BY n.updated_at DESC;
```

Example notes must be excluded when answering real questions, either by path or
the `example` tag.

## Current limitations

The following parts of ADR 0021 are not implemented yet:

- no specialized fact mutation/list tools (`memory_save`, `memory_list`,
  `memory_update`, `memory_supersede`, `memory_delete`);
- no automatic similar-fact/conflict response before writes;
- no companion memory vault automatically attached to other vault sessions;
- no automatic scope filtering by the active vault;
- no stale-fact review UI;
- no bounded core-memory injection.

Consequently, fact recall currently works automatically only when the agent has
the `memory` vault's MCP tools available. When chatting in another vault, attach
the memory MCP endpoint manually or open the memory vault in its own Notesmith
window.

## Suggested dogfood routine

1. Use explicit `Remember`, `Document`, `Both`, and `Do not save` wording for a
   few days.
2. Use neutral `Save this; decide where it belongs` prompts when testing agent
   routing.
3. Review active facts periodically for duplicates and outdated claims.
4. Track cases where generic search/template tools feel awkward.
5. Implement #203 only around repeated friction observed during dogfooding.

The success criterion is not the number of stored facts. It is whether future
tasks become more accurate with less repeated explanation.
