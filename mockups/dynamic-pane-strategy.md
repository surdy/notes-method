
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
