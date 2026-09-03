---
title: Work integrations phase 3 verification handoff
date: 2026-09-02
tags:
  - notesmith
  - verification
  - workiq
  - handoff
status: ready
---

# Work integrations phase 3 verification handoff

**Audience:** the agent session running on Harpreet's work laptop — the same
one that produced the two reports below.

Related:

- [[work-integrations-verification-handoff]] (the original A–G plan)
- [[work-integrations-verification-results-handoff]]
- [[work-integrations-post-fix-rerun-handoff]] (your last report)

## What changed since your last report

Your Finding 1 was accepted in full. Managed-section byte preservation is no
longer asked for in a prompt — it is a deterministic core operation. A pure
string transform in `notesmith-vault` replaces only the byte range between one
marker pair and copies every other byte through unchanged; it is surfaced as
`POST /api/v/{vault}/notes-section/{path...}` and as the
`update_managed_section` MCP tool (read-write `/mcp/{vault}` only, rejected on
`/mcp-ro/{vault}` like every other write). Every recommendation on your list
landed with it: malformed layouts (duplicate begin, duplicate end, inverted
pair, unpaired marker, a marker line inside the replacement content) are
structured refusals that write nothing; a missing pair appends exactly one
separator and one complete block; the write is atomic and hash-guarded so a
concurrent human edit conflicts instead of being silently overwritten; and —
answering your decision question 5 — **managed-section writes skip the save
pipeline entirely**, so there is no `updated:` restamp, no frontmatter key
sorting, and no whitespace trimming. "Outside the markers is inviolable" now
includes the frontmatter.

Your Finding 2 was resolved as your option 2: the `daily-note` prompt now calls
`update_managed_section` once per section instead of reconstructing the note,
and when writes are denied it renders the full four-section briefing to stdout
as a preview rather than reporting a refusal. Your Finding 3 was applied to the
original plan — phase C sub-check 4 now tells the tester to add the recent
meeting reference *before* flipping the stream to active.

Two ADRs were also corrected against your evidence. ADR 0012 carries a dated
amendment scoping the Copilot claim to what you actually observed — Copilot's
ACP mode rejects *client-supplied* stdio MCP servers; Copilot supports stdio
MCP from its own config/SDK paths — and ADR 0025 carries a second amendment
pinning down exactly where the raw-email boundary runs (see "Work IQ briefing"
below, which is now the sanctioned path rather than a workaround).

This handoff assumes you pull a `main` containing that work. Confirm before
starting:

```bash
git -C <repo> log --oneline -5      # must include the managed-section commit
grep -rn "update_managed_section" crates/notesmith-vault/src/managed_section.rs | head -1
```

Read `docs/managed-sections.md` and the `notes-section` section of
`docs/http-api.md` first — together they are the contract you are verifying.

## 0. Setup

Same ground rules as the original plan: a fresh scratch vault, dummy data you
create, no real work vault, and Harpreet does anything only a human can do.

```bash
cd <repo>
cargo test --workspace            # baseline
cargo build --release
mkdir -p ~/vaults/verify-work-phase3 && cd ~/vaults/verify-work-phase3
notesmith kit apply work-notes --path .
notesmith daemon start
```

Rebuild the dummy-data set from section 2 of the original plan, using the
daemon's `date('now','localtime')` day as "today", and re-run the six
`context_queries` through `notesmith query sql` before involving any agent. If
a query returns the wrong rows, record it — do not adjust the SQL to match.

## 1. Rerun phases A–C against the new tool

Repeat phases A, B and C from
[[work-integrations-verification-handoff]] unchanged in shape. The expected
outcomes have moved, so the pass criteria below **supersede** the ones in that
document.

### A. Headless read-only safety — now expects a preview

```bash
notesmith ai prompt daily-note --agent copilot --vault <scratch> --url <daemon>
```

- Exit 0.
- **stdout contains a four-section briefing preview** — `briefing/meetings`,
  `briefing/email`, `briefing/tasks`, `briefing/attention` — preceded by a
  short line saying the vault is read-only so nothing was written. This is the
  behaviour your Finding 2 asked us to make explicit; it is now in the prompt,
  so a bare refusal message is a **fail**, not a pass.
- Nothing in the vault changes. Verify by checksum or a `cp -r` snapshot, not
  by inspection, and say in the report which you used.
- Run it once with the daily note **absent** and once with it **present** —
  both must produce the preview. The absent case is the one that regressed
  last time.

### B. Full run with writes

Unchanged from the original plan. The one thing to watch: the agent should
reach the four sections through `update_managed_section` calls, not through
`update_note`. If your agent transcript shows a whole-note `update_note` on the
daily note, record it as a finding even if the resulting bytes look right — the
whole point of this round is that the deterministic path is the one being used.

### C. Managed-section contract — the core of this round

Snapshot the daily note before each sub-check.

1. **Outside is inviolable — including `updated:`.** Add human text under
   `## Focus` and `## Notes` with deliberate trailing spaces, a tab-indented
   line, and a stray `<!--` that is never closed. Note the current `updated:`
   value. Re-run B. Everything outside the four marker pairs must be
   **byte-identical**, and that now explicitly includes the YAML frontmatter:
   `updated:` must be **unchanged**. Last time this was the C1/C3 failure; it
   is the headline check now. Verify by replacing each section interior with a
   fixed placeholder and running `diff`/`cmp` on the remainder, and quote the
   command you used.
2. **Idempotent re-runs.** Two more runs with no data change. Byte-identical is
   now the expectation, not the aspiration — writing the same interior twice is
   a no-op at the byte level.
3. **Missing pair → append, with no metadata change.** Delete the whole
   `briefing/attention` pair. Re-run. The complete marked block is appended at
   the end of the note, separated from the previous final content by exactly
   one blank line; nothing above it moves; **`updated:` does not change** and no
   extra blank line appears anywhere else. (Both of last round's C3 defects are
   in this one sub-check.)
4. **Data change propagates.** Use the corrected fixture: first add a meeting
   note with `date: <today>` and `streams: ["[[Payments Migration]]"]`, *then*
   flip `Streams/Payments Migration.md` to `status: active`. The stream must
   leave Attention entirely. Restore both edits afterwards.
5. **Malformed markers refuse structurally.** New sub-check. By hand, duplicate
   one `<!-- notesmith:section:begin briefing/tasks -->` line so the note has
   two begin markers for the same id. Snapshot the note. Re-run the briefing
   (or hit the endpoint directly, which is the cleaner evidence):

   ```bash
   curl -sS -o /dev/stderr -w '%{http_code}\n' \
     -X POST http://127.0.0.1:27183/api/v/<vault>/notes-section/Daily/<today>.md \
     -H 'Content-Type: application/json' \
     -d '{"section_id":"briefing/tasks","content":"- probe"}'
   ```

   Expect **HTTP 422** with a body carrying a stable `reason` code
   (`duplicate_begin_marker`) and the `section_id`, and the note **byte-identical
   to the snapshot** — no partial rewrite. Through the MCP tool the same
   condition must surface as a tool error the agent can report, not a silent
   success. Repeat once with an inverted pair (end line moved above begin) if
   it is cheap; one malformed case is the minimum.

Then run phases D and E from the original plan as a regression pass. They
passed last time; the managed-section change touches the write path they
exercise, so re-run rather than assume.

## 2. Functional Work IQ briefing (your "phase F functional")

This is now the **sanctioned** judgment-tier path for Copilot, not a fallback.
The ADR 0025 amendment records the reasoning: raw email may transit the
*agent's* context via an agent-attached Work IQ tool, and what Copilot's own
CLI retains locally is governed by Copilot's retention settings, explicitly
outside Notesmith's boundary. Notesmith's boundary is that raw email never
enters Notesmith's processes or storage.

1. Leave Copilot's own Work IQ plugin enabled. Do **not** configure a Work IQ
   `[[mcp.servers]]` entry for this test — the whole point is that Notesmith
   ships no token.
2. Run the write-enabled briefing against the scratch vault. `briefing/email`
   must contain a short bullet summary: sender and subject, at most one clause
   of gist per item.
3. **Boundary check, same as before.** Search the entire scratch vault *and*
   `NOTESMITH_STATE_DIR` (job history, run records, captured stdout, daemon
   logs, connector state) for raw email leakage — quoted bodies, `From:` /
   `Received:` headers, message-IDs, long verbatim passages. Only the summary
   may exist on Notesmith's side. Say in the report what you grepped for and
   where; a bare "looked clean" is not evidence.
4. Confirm the same holds when the briefing runs as a **daemon job** rather
   than an interactive CLI invocation — that is the path where prompt rendering
   and job recording could capture something.
5. Disable the Work IQ plugin, re-run, and confirm the exact fallback line
   returns and the run still succeeds.

## 3. Auth-fixture transport verification

Your report recommended replacing the production Work IQ dependency in the
transport test with a controllable fixture. Do that here. This validates
Notesmith's HTTP `[[mcp.servers]]` propagation and `$VAR` header expansion
without any corp credential.

Write a minimal Streamable HTTP MCP server that requires an `Authorization`
header, in the scratch area (not the repo). Something along these lines is
enough — it needs exactly one tool and it needs to log the header it received:

```python
#!/usr/bin/env python3
"""Minimal Streamable-HTTP MCP fixture that requires an Authorization header."""
import json, os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

EXPECTED = "Bearer " + os.environ["FIXTURE_EXPECTED_TOKEN"]
TOOL = {
    "name": "fixture_ping",
    "description": "Returns OK. Fixture only.",
    "inputSchema": {"type": "object", "properties": {}},
}

class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):        # keep stderr readable
        sys.stderr.write("fixture: " + fmt % args + "\n")

    def do_POST(self):
        auth = self.headers.get("Authorization")
        # Log only the shape, never the value: this file must not become the
        # thing that leaks the token you are checking for.
        sys.stderr.write(
            f"fixture: auth header {'present' if auth else 'ABSENT'}, "
            f"matches={auth == EXPECTED}\n"
        )
        body = self.rfile.read(int(self.headers.get("Content-Length", 0) or 0))
        msg = json.loads(body or b"{}")
        if auth != EXPECTED:
            return self._send(401, {"error": "unauthorized"})
        method, mid = msg.get("method"), msg.get("id")
        if mid is None:                       # a notification
            return self._send(202, None)
        if method == "initialize":
            result = {
                "protocolVersion": msg["params"]["protocolVersion"],
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "auth-fixture", "version": "0"},
            }
        elif method == "tools/list":
            result = {"tools": [TOOL]}
        elif method == "tools/call":
            result = {"content": [{"type": "text", "text": "OK"}]}
        else:
            return self._send(200, {"jsonrpc": "2.0", "id": mid,
                                    "error": {"code": -32601,
                                              "message": "method not found"}})
        self._send(200, {"jsonrpc": "2.0", "id": mid, "result": result},
                   session=True)

    def _send(self, code, payload, session=False):
        data = b"" if payload is None else json.dumps(payload).encode()
        self.send_response(code)
        if data:
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
        if session:
            self.send_header("Mcp-Session-Id", "fixture-session")
        self.end_headers()
        if data:
            self.wfile.write(data)

HTTPServer(("127.0.0.1", 8765), Handler).serve_forever()
```

Adjust it until a real agent session can actually list `fixture_ping` — if the
agent's MCP client is stricter than this sketch (session-id handling, SSE
`Accept` negotiation), fix the fixture, and note what it needed. That detail is
itself a useful finding.

Then:

```bash
export FIXTURE_EXPECTED_TOKEN=s3cr3t-fixture-value    # server side
export FIXTURE_TOKEN=s3cr3t-fixture-value             # Notesmith side
```

```toml
# ~/.config/notesmith/config.toml (the isolated one for this run)
[[mcp.servers]]
id = "auth-fixture"
url = "http://127.0.0.1:8765/mcp"
display_name = "Auth Fixture"
enabled = true

[mcp.servers.headers]
Authorization = "Bearer $FIXTURE_TOKEN"
```

Verify, through a **real agent session** (`notesmith ai prompt …`, and once
through the desktop chat if convenient) — not by unit test, that part is
already covered:

1. **The header arrives.** The fixture's stderr shows `matches=True` and the
   agent can call `fixture_ping` and get `OK`. That proves the entry reached
   `session/new`, the agent opened the connection, and `$FIXTURE_TOKEN` was
   expanded on the way.
2. **The resolved secret appears nowhere on Notesmith's side.** Grep for the
   literal token value across: the daemon log, `NOTESMITH_STATE_DIR` (job
   history and run records included), the scratch vault, the rendered prompt,
   and any diagnostics output. Also check the **config API**: the desktop
   Settings → MCP Servers screen must show the `Authorization` row with an
   empty value field and a "value stored" hint, and saving from that screen
   must not wipe the stored token. Only `config.toml` may hold the literal
   `Bearer $FIXTURE_TOKEN` string — the *unexpanded* form. If you find the
   resolved value anywhere else, that is a security finding: stop and report it
   before continuing.
3. **Auth failure degrades gracefully.** Change `FIXTURE_TOKEN` to a wrong
   value (leave the fixture's expected value alone) and re-run the daily
   briefing job. The fixture returns 401, the agent loses that server's tools —
   and the **vault job must still succeed**: the four managed sections still get
   written, `briefing/email` falls back to
   `Email summary unavailable (Work IQ not connected).`, and `job list` shows
   `succeeded`. A failed job here is a finding. Do the same with the fixture
   stopped entirely (connection refused), which is the more common real-world
   failure.

Then remove the fixture entry from config and stop the server.

## 4. Connector groundwork — capture, do not build

Your recommended direction was a Work IQ CLI-backed connector. Before that can
be designed, we need the real CLI interface rather than guesses. **Do not build
the connector.** Do not add a `Source`, a `[[jobs]]` entry, or any Rust. Just
capture and report:

```bash
npx -y @microsoft/workiq@latest --help
npx -y @microsoft/workiq@latest fetch --help
```

Report back, verbatim where possible:

- the full `fetch` subcommand list and every flag with its description;
- the output format — JSON? NDJSON? human text? is there a `--json` /
  `--format` flag, and is the schema stable-looking or presentation-shaped?
- what an inbox/message listing actually looks like: run it against your own
  mailbox with the **narrowest possible** filter (a small `--top`/`--limit`, a
  single recent day) and paste a **redacted** sample — field names and shapes,
  with subjects and addresses replaced by placeholders. We need the schema, not
  the data;
- **the key question: can `fetch` return sender + subject metadata without
  message bodies?** Is there a field selection / projection flag (`--select`,
  `--fields`, `--no-body`, a `$select`-style passthrough)? If bodies always come
  back, say so plainly — that changes the design, because the ADR 0025
  amendment permits a deterministic connector to persist only sender/subject
  metadata, and a connector that must *receive* bodies to get there needs an
  explicit discard step and a much narrower blast radius;
- how it authenticates on a fresh shell (does it reuse the MSAL cache
  non-interactively? does it prompt?), and what it does when the cache is
  expired — exit code and message text, since that becomes the "reconnect"
  error;
- whether it supports a time window / `--since` filter, per-run limits, and
  what it does on rate limiting;
- rough latency and output size for a one-day inbox listing.

That report is the input to the connector design. A guessed interface would
mean designing twice.

## 5. Upstream Copilot issue: do NOT file

Harpreet has decided nothing gets filed against `github/copilot-cli` — do not
open an issue, comment on one, or otherwise report this upstream from any
account. The evidence (the `Rejecting non-http/sse MCP server` log line and
the changelog contradiction) stays recorded in
[[work-integrations-post-fix-rerun-handoff]] and the ADR 0012 amendment;
periodically re-testing against new Copilot CLI releases is how the claim
stays checkable. Nothing in this round depends on the outcome — the HTTP
binding is the right answer regardless.

## 6. Report back

One results table: phase / sub-check, pass / fail / not-exercised, and for
every failure the exact command, the exact output, and the note content
involved. Byte-identity claims must say how they were checked. Keep the
`workiq fetch` capture and the fixture findings as their own sections — they
are deliverables in their own right, not appendices.

Leave the scratch vault in place and list its path. Restore Notesmith.app, its
normal daemon, and the real global config when you are done, and say so.
