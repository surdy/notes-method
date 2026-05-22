# ADR 0009 — Resilience to Malformed User Content

## Status

Accepted (2026-05-20).

## Context

A user vault contained a `.md` file with a malformed YAML frontmatter block
(`slack: slack: slack://...`). On startup the daemon ran `cache.reindex(...)`,
which called `serialize_frontmatter_json` → `serde_yaml::from_str(...)?`. The
`?` propagated the YAML error through the cache transaction (rolled back),
through `create_vault_state`, and out of `main` — the daemon exited 1 before
binding its HTTP port. The desktop app then showed:

> Could not connect to Notesmith daemon
> Notesmith couldn't start its background service.

A single bad note in any vault took down the entire app for both vaults. This
is not acceptable: Notesmith reads arbitrary Markdown that the user — or other
tools like Obsidian, Templater, Dataview — created. Such content **will** be
malformed sometimes, and Notesmith must degrade gracefully, not crash.

## Decision

**Malformed user content must never crash the daemon, the desktop app, or any
indexing pass.** Treat all `.md` content as untrusted input, the same way a
web server treats incoming requests.

The architectural rule is **isolate at the per-note boundary**:

1. Every operation that parses or processes a single note (frontmatter parse,
   markdown render, link extraction, task extraction, route preview, template
   load, search/cache index) must catch errors at the note level, log a
   structured warning with the note path, and continue with the rest of the
   vault.
2. The daemon's startup path is a hot path: it iterates every note in every
   configured vault. **No per-note error in this path may abort startup.**
3. Errors surfaced from individual notes must be observable: log
   `WARN note=<vault-path> stage=<frontmatter|render|...> reason=<error>`,
   bump a counter on the vault's status, and (eventually) expose a per-note
   diagnostic in `/api/status` so the UI can surface "12 notes failed to
   parse" without disrupting the user.

## Consequences

### Required code patterns

- **YAML / JSON / TOML parsing of per-note content uses `.ok()` or matched
  `Err` arms — never `?` that propagates above the per-note boundary.** The
  fallback is "treat as no parsed value" (e.g. `frontmatter: None`); the raw
  body is still indexed.
- **`unwrap` / `expect` are forbidden on values derived from file content.**
  They are permitted only for: regex compilation at module init, mutex locks
  that indicate a programmer bug, and test code.
- **Catch-and-log per note inside indexing loops.** `index_all` iterates
  notes; one bad note must not roll back the whole transaction. Prefer
  per-note savepoints (or skip the row and log) over batched all-or-nothing
  inserts when the failure is content-derived.
- **HTTP handlers that accept arbitrary bodies wrap parse failures into 4xx
  responses with structured error payloads** rather than letting axum return
  a generic 500. Existing pattern in `routes::routing::preview` is the model.
- **Renderers and CodeMirror decoration builders must tolerate malformed
  Markdown.** A failed render must produce a fallback (raw text) rather than
  panic. Decoration construction failures must be caught and reported.

### Required test patterns

Every parser, renderer, or indexer that touches `.md` content must have at
least three test categories:

1. **Happy-path test** — well-formed input produces the expected structure.
2. **Malformed-content test** — broken YAML/Markdown/link syntax produces a
   degraded but valid result (e.g. `Note { frontmatter: None, ... }`) and
   surfaces a logged warning.
3. **Adversarial-content test** — randomized or fuzz-style inputs (or a
   curated `test-fixtures/malformed-vault/` of pathological notes) confirm no
   panic, no error propagation beyond the per-note boundary, and a bounded
   wall-clock cost.

A shared fixture `test-fixtures/malformed-vault/` will contain notes that
have each historically broken the system; new resilience bugs add a fixture
file plus a regression test.

### Required telemetry

The daemon must, by design, be able to answer the question "did anything
fail to index?" without checking logs. `/api/status` already lists per-vault
indexer health; this must be extended with a per-vault `parse_warnings`
count and a sampled list of recent warnings (path + stage + reason).

## Known concrete fix sites (initial sweep, May 2026)

| Site                                                            | Symptom                                  | Fix                                                 |
| --------------------------------------------------------------- | ---------------------------------------- | --------------------------------------------------- |
| `crates/notesmith-index/src/indexer.rs:226`                     | YAML err in one note aborts reindex tx   | Fall back to `"{}"` and log; do not propagate       |
| `crates/notesmith-index/src/indexer.rs:18` (index_all loop)     | Any per-note error rolls back whole tx   | Per-note savepoint; skip+log on failure             |
| `crates/notesmith-html/...` (render path)                       | Pathological Markdown could panic        | Audit + add malformed-content tests                 |
| `ui/app/src/lib/editor/*` (CodeMirror decorations)              | Out-of-order ranges → runtime crash      | Already partially mitigated; add fuzz-style test    |
| `crates/notesmith-vault/src/save_pipeline.rs:50`                | `.ok()?` — already safe, document why   | Comment + test for malformed input round-trip       |

(Each row will be filed as a separate GitHub issue. See triage label
`resilience`.)

## Alternatives considered

- **Validate-on-save only.** Rejected: notes already exist from other tools
  (Obsidian, Templater) and from prior Notesmith versions. We cannot assume
  vault contents are well-formed at any boundary other than "after we wrote
  the file ourselves".
- **Block the offending vault.** Rejected: one note must not disable an
  entire vault; the rest of the notes are still usable.
- **Surface the error to the user as a hard failure.** Rejected: the user
  may not be the author of the offending file (e.g. a synced template), and
  the daemon must remain available so they can navigate to and fix it.

## References

- Original incident: daemon exit 1 with
  `Error: mapping values are not allowed in this context at line 3 column 13`
  on 2026-05-20.
- Tracking issue for the daemon-startup fix: [#90](https://github.com/surdy/notes-method/issues/90).
- Related ADR: [0006 — Crate per domain](0006-crate-per-domain.md) (each
  crate owns its content boundary).
