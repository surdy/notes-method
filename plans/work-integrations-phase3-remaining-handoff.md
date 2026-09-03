---
title: Work integrations phase 3 remaining work
date: 2026-09-03
tags:
  - notesmith
  - verification
  - handoff
status: ready
---

# Work integrations phase 3 — remaining work

**Audience:** the agent session on Harpreet's work laptop.

This is a pointer document: the full instructions live in
[[work-integrations-phase3-verification-handoff]], which has been **updated
since you last pulled** (you tested at `0bc7c3b`; pull current `main` first).
Two changes matter: section 5 now says the upstream Copilot issue must NOT be
filed (nothing gets filed or commented on `github/copilot-cli` from any
account — the tracker to watch, read-only, is
[copilot-cli#3889](https://github.com/github/copilot-cli/issues/3889)), and
there is a **new section 6** with an experiment you haven't seen yet.

## Already done — do not repeat

| Handoff section | Status | Results doc |
|---|---|---|
| §2 Functional Work IQ briefing (phase F) | ✅ Pass | [[work-integrations-phase3-functional-f-results]] |
| §3 Auth-fixture transport verification | ✅ Pass (one item open, below) | [[work-integrations-phase3-auth-fixture-results]] |
| §5 Upstream Copilot issue | Superseded: do NOT file | — |

## Remaining, in priority order

1. **§1 — Rerun phases A–C against `update_managed_section`.** The highest
   value item: field validation of the deterministic managed-section op your
   Finding 1 asked for. Pass criteria are in the handoff — read-only runs
   print the four-section preview and write nothing; C1 byte-identity now
   includes the `updated:` frontmatter; C3 appends with no metadata change;
   the new C5 malformed-marker check must return the structured
   422/`duplicate_begin_marker` error and leave the note untouched.
2. **§6 (new) — `--additional-mcp-config` spawn-time experiment.** Research
   on #3889 found Copilot accepts stdio MCP servers via
   `--additional-mcp-config=@<file>` on its own command line, and headless
   runs are one-session-per-process, so the flag's process-scoping doesn't
   bite. Use the same wrapper-script trick as your phase-F
   `--disable-mcp-server` run. Prove the transport with a trivial local
   stdio server first; only then try `workiq mcp` (pre-installed binary, not
   `npx` — the 60s init budget from #4421). Full steps in the handoff. If it
   works, report exact flag syntax and logs: that becomes the spec for a
   small spawn-time config-injection change in `notesmith-agent`.
3. **§4 — Capture the `workiq fetch` interface.** `workiq fetch --help`,
   subcommands, output shapes, whether sender+subject metadata is available
   without bodies, auth-expiry behavior, `--since`/limit support, latency.
   Do NOT build the connector — the capture is the design input.
4. **Settings redaction acceptance check (small, manual).** The one open
   item from your auth-fixture report: with an isolated config containing
   the fixture entry, open Settings → MCP Servers in the real desktop app
   and verify (a) `Authorization` renders empty with the "value stored"
   hint, (b) saving without typing a value preserves
   `Bearer $FIXTURE_TOKEN` in `config.toml`. Harpreet can click through it
   with you if UI automation isn't available.

## Ground rules (unchanged)

Scratch vault only; dummy data; the existing scratch vault at
`/Users/surdy/vaults/verify-work-phase3-2026-09-02` is fine to reuse. Ask
Harpreet for anything only a human can do. Nothing gets filed upstream.
Report back in the usual form: one results table per section, exact commands
and output for failures, byte-identity claims stating how they were checked.
