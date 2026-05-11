
## Approach A (gpt-5.5)

Approach A treats the middle pane as a contextual capability, not a permanent column. Each top-level view declares a default pane mode, and individual folders may override that default when their workflow is more specific than the view. The sidebar remains stable while the workspace resolves to either Browse mode (sidebar + reader) or Triage mode (sidebar + triage queue + reader).

Suggested configuration:

```yaml
panePolicy:
  defaultMode: browse
  views:
    notes:
      mode: browse
    tasks:
      mode: triage
    calendar:
      mode: browse
    review:
      mode: triage
  folders:
    Inbox:
      mode: triage
      reason: Captures need routing before reading.
    Projects:
      mode: browse
      reason: Project notes should open directly in the reader.
    Archive:
      mode: inherit
```

Precedence rules:

1. A folder-level `mode: triage` or `mode: browse` wins over the selected view.
2. A folder-level `mode: inherit` uses the selected view mode.
3. If the selected view has no explicit mode, use `panePolicy.defaultMode`.
4. If no policy is available, fall back to Browse mode to avoid surprising layout expansion.

UX behavior when switching contexts: the mode indicator updates immediately, the middle pane animates in only for Triage mode, and Browse mode preserves the current reader focus by collapsing the middle pane rather than replacing the document. When a user clicks Inbox from any view, the app opens the 3-pane triage layout; when they click Projects, it returns to the 2-pane reading layout even if the previous view was triage-oriented.

## Approach B (opus 4.6)

### Concept: Policy Engine with Section Overrides and Manual Pin

Instead of a binary 2- vs 3-pane choice at the view level, pane layout is resolved per-click through a three-tier precedence engine:

1. **Manual pin override** (highest) — user presses `P` or clicks the 📌 button to force the triage pane open regardless of context. Stays active until explicitly unpinned.
2. **Section-level policy** — each sidebar section declares a default pane mode via a `policy` attribute (`"3"` = show triage, `"2"` = skip triage). Visually indicated by a badge on the section header.
3. **View-level default** (lowest) — the overall view can set a fallback, but sections always override it.

Resolution rule: `pin > section.policy > view.default`.

### Configuration Snippet

```yaml
views:
  main:
    default_panes: 2
    sections:
      inbox:
        policy: 3-pane
        triage_sort: newest-first
      tasks:
        policy: 3-pane
        triage_sort: priority
        triage_group_by: due_date
      customers:
        policy: 3-pane
        triage_sort: last_activity
      projects:
        policy: 2-pane          # no triage — straight to editor
      archive:
        policy: 2-pane

pin_behavior:
  persist: session              # or "permanent" / "per-view"
  empty_triage: show_placeholder  # when pinned on a 2-pane section
```

### Precedence Rules

| Priority | Source | Controls | Persists |
|----------|--------|----------|----------|
| 1 (highest) | Manual pin toggle | Forces 3-pane globally | Until unpinned (session-scoped) |
| 2 | Section `policy` attribute | Determines pane count per section | Permanent (config) |
| 3 (lowest) | View `default_panes` | Fallback for sections without explicit policy | Permanent (config) |

When the user navigates from a 3-pane section to a 2-pane section:
- **Not pinned**: triage pane animates closed, editor expands.
- **Pinned**: triage pane stays open but shows a placeholder ("This section has no triage items. Unpin to collapse.").

### Edge Cases

**Narrow screens (< 900px)**
Triage pane collapses into a slide-over drawer that overlays the editor rather than consuming horizontal space. The pin toggle converts to a "keep drawer open" behavior. Below 600px, the sidebar itself collapses to an icon rail, and triage becomes a full-screen list with back-navigation.

**Deep links**
When a deep link targets a specific note (e.g., `notesmith://vault/inbox/note-42`), the app resolves the note's parent section, applies that section's pane policy, auto-selects the note in the triage list, and loads the editor. If the section is 2-pane, the note opens directly without triage.

**Keyboard navigation continuity**
Arrow keys (↑↓) navigate the triage list when the triage pane is focused. `Esc` closes the triage pane (unless pinned). `P` toggles pin. Focus returns to the last-active triage item when re-entering a 3-pane section, preserving scroll position. Tab order: sidebar → triage list → editor.

**Transition animations**
Pane width changes use a 220ms cubic-bezier ease to avoid layout jank. The CSS grid `grid-template-columns` property transitions smoothly. Content within panes remains stable during transition — no reflow or scroll-position loss.

---

## Approach C (opus 4.7 xhigh)

### Premise

Approaches A and B both decide whether the triage pane is visible. They differ
on *where* the decision lives (per-view vs. per-folder, with precedence). They
share a problem: the moment the layout changes, the user has no idea **why**.
Two clicks into the app, the middle pane appears and disappears like
weather, and the configuration knobs feel disconnected from the result.

Approach C reframes the question. The pane is governed by a small **policy
engine** — an ordered list of rules, each with a condition and an action — and
the UI exposes that engine with a one-click "explain" affordance. View-level and
folder-level decisions stop being separate features and become two kinds of
*condition* in the same rule list.

### Policy debuggability

The headline feature is the **rule inspector** in the main pane's header. It
always shows:

- a state badge — `Triage ON` or `Triage OFF` — pulsing softly when ON;
- the `id` of the rule that matched;
- a one-click "explain" toggle.

When opened, the inspector reveals three sections:

1. **Why this layout?** — the matched rule's `reason` field, written for humans
   ("Anything in Inbox is, by definition, unprocessed"), plus the resolved
   context object (`{view, path}`) so the user can see what the engine
   actually saw.
2. **Rules considered (first match wins)** — the full ordered list, with the
   matched rule highlighted, skipped-but-also-matching rules struck through, and
   the action each rule *would* have taken shown as a chip on the right. This is
   the diff between "what could have happened" and "what did happen."
3. **Resolved policy** — the literal `{matched_rule, action}` JSON, copy-able
   for bug reports and for rule authors editing their config.

Two design moves make this work:

- **Layout flips flash the inspector.** When triage turns on or off, the
  inspector card briefly tints accent-blue, training the user's eye on the one
  thing that explains the change.
- **Sidebar entries carry a triage dot.** Every view and folder in the sidebar
  has a tiny green/grey marker showing the rule's verdict for that target — so
  the user can predict the layout *before* clicking, and learn the policy by
  ambient exposure rather than by opening a settings page.

### Config schema

Rules are an ordered list, evaluated top-down, first match wins. Conditions are
expressed as a small DSL over a context object `{view, path, folder_tags,
note_count}`. Actions can turn the triage pane on or off and choose which queue
to populate.

```yaml
panePolicy:
  version: 1
  defaultAction: { triage: false }   # used if no rule matches

  rules:
    - id: inbox-needs-triage
      name: "Inbox needs triage"
      reason: "Anything in Inbox is, by definition, unprocessed."
      when:
        any:
          - view: inbox
          - path: { startsWith: "Inbox/" }
      then:
        triage: true
        queue: unprocessed

    - id: archive-is-read-only
      name: "Archive is read-only"
      reason: "Archived content is preserved as-is; surfacing triage invites churn."
      when:
        path: { matches: "**/Archive/**" }
      then: { triage: false }

    - id: today-surfaces-followups
      name: "Today surfaces follow-ups"
      reason: "The Today view is the daily review surface."
      when: { view: today }
      then:
        triage: true
        queue: due-today

    - id: active-projects-show-followups
      name: "Active project folders show follow-ups"
      reason: "Active projects accumulate open loops worth surfacing."
      when:
        path: { startsWith: "Projects/Active/" }
      then:
        triage: true
        queue: project-followups

    - id: reference-is-read-only
      when: { path: { startsWith: "Reference/" } }
      then: { triage: false }
      reason: "Reference notes are looked up, not worked through."
```

Notes on the schema:

- **Precedence is just list order.** No separate "view vs. folder" tie-breaker
  table — if you want folder rules to win, put them above view rules. This makes
  the precedence model trivially debuggable in the inspector.
- **`reason` is required.** A rule that can fire without explaining itself is a
  rule that will confuse users; the field is part of the contract, not a
  comment.
- **Actions are extensible.** `triage: true` is the v1 action; future rules can
  attach `queue`, `sort`, or `groupBy` without changing the engine.
- **`note_count` is available** but intentionally absent from the defaults —
  count-based rules (e.g., "hide triage if queue is empty") are tempting but
  produce a layout that flickers as the user processes items. Encourage authors
  to keep conditions stable across short time horizons.

### Recommended defaults for first-time users

A new install ships with five rules that cover the 90% case and are intended to
be readable, not minimal:

```yaml
panePolicy:
  version: 1
  defaultAction: { triage: false }
  rules:
    - id: inbox-needs-triage
      when: { any: [{ view: inbox }, { path: { startsWith: "Inbox/" } }] }
      then: { triage: true, queue: unprocessed }
      reason: "Inbox exists to be processed."

    - id: today-surfaces-followups
      when: { view: today }
      then: { triage: true, queue: due-today }
      reason: "Today is the daily review surface."

    - id: archive-is-read-only
      when: { path: { matches: "**/Archive/**" } }
      then: { triage: false }
      reason: "Archived content is settled."

    - id: reference-is-read-only
      when: { path: { startsWith: "Reference/" } }
      then: { triage: false }
      reason: "Reference notes are looked up."

    - id: search-is-passive
      when: { view: search }
      then: { triage: false }
      reason: "Search is a transient lens."
```

Why these and not more:

- **Two ON rules, three OFF rules.** Triage is the loud, layout-expanding state;
  defaults should bias toward calm. Users add ON rules as they discover folders
  worth surfacing.
- **Pair every "show triage" rule with a visible queue name.** The queue name
  becomes the badge text in the middle pane header, so users learn the
  vocabulary of their own setup.
- **No project-folder rule by default.** Projects vary too much across users to
  pre-judge; the inspector's "what would have happened" trace makes adding one
  later a guided experience rather than a configuration treasure hunt.
- **Search and All Notes stay browse-only.** These are the two views users
  inevitably visit while exploring the app; keeping them calm prevents
  first-run users from concluding "the layout is unstable."
