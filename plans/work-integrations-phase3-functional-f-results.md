---
title: Work integrations phase 3 functional F results
date: 2026-09-02
tags:
  - notesmith
  - verification
  - workiq
  - handoff
status: complete
---

# Work integrations phase 3 functional F results

Related:

- [[work-integrations-phase3-verification-handoff]]
- [[work-integrations-post-fix-rerun-handoff]]

## Environment

- Repository: `surdy/notes-method`, `main` at `e8a04fd`
- Managed-section implementation: `706ef0b`
- Notesmith binary: release build from this checkout
- External agent: GitHub Copilot CLI `1.0.83-3`
- Scratch vault:
  `/Users/surdy/vaults/verify-work-phase3-2026-09-02`
- Verification date: September 2, 2026, matching the daemon's
  `date('now', 'localtime')`
- Copilot's `workiq@copilot-plugins` plugin remained installed and enabled
- `WORKIQ_TOKEN` was unset
- The isolated Notesmith config contained no Work IQ `[[mcp.servers]]` entry

The workspace baseline passed before the run:

```text
cargo test --workspace
cargo build --release
```

## Results

| Phase F check | Result | Evidence |
|---|---|---|
| Interactive write-enabled briefing | Pass | `notesmith ai prompt daily-note ... --agent copilot --allow-writes` exited 0, created `Daily/2026-09-02.md`, and persisted a live Work IQ email summary rather than the disconnected fallback. |
| Summary shape | Pass | The persisted email section contained two terse bullets. Each followed `sender — subject — gist` (one used `sender — subject \| context — gist`); neither used a second sentence or semicolon. Mailbox content is intentionally not reproduced here. |
| Daemon-job briefing | Pass | `notesmith job run daily-briefing` completed with `last: succeeded 2026-09-02T17:45:19.465887+00:00`; the email section again contained a live summary rather than the fallback. |
| Notesmith persistence boundary | Pass | Scanned 36 files across the complete scratch vault, the vault-specific Notesmith application-state directory, and the daemon log segment beginning with this vault's job-runner startup. Found zero raw mail headers, message IDs, quoted-reply markers, bearer values, or lines over 500 characters. |
| Work IQ unavailable fallback | Pass | Re-ran through a Copilot wrapper adding `--disable-mcp-server workiq`. The command exited 0 and `briefing/email` was exactly `Email summary unavailable (Work IQ not connected).` |
| Final live state | Pass | Re-enabled the normal Copilot configuration and re-ran successfully; the scratch note again contains the live two-bullet summary. |

## Commands and boundary scan

Interactive path:

```bash
XDG_CONFIG_HOME="$SCRATCH/.xdg-config" \
  ./target/release/notesmith ai prompt daily-note \
  --date 2026-09-02 \
  --vault verify-work-phase3-2026-09-02 \
  --url http://127.0.0.1:27183 \
  --agent copilot \
  --allow-writes
```

Daemon-job path:

```bash
XDG_CONFIG_HOME="$SCRATCH/.xdg-config" \
  ./target/release/notesmith \
  --vault verify-work-phase3-2026-09-02 \
  job run daily-briefing
```

The leakage scan covered:

```text
/Users/surdy/vaults/verify-work-phase3-2026-09-02/**
/Users/surdy/Library/Application Support/notesmith/verify-work-phase3-2026-09-02/**
~/Library/Logs/Notesmith/daemon.log
  (only the segment from this vault's latest "starting job runner" line)
```

It searched for:

- mail header lines including `From:`, `Received:`, `Message-ID:`,
  `In-Reply-To:`, `References:`, MIME headers, delivery headers, and
  authentication headers;
- angle-bracket message-ID shapes;
- quoted replies (`>`, `On ... wrote:`, and `-----Original Message-----`);
- bearer-token shapes;
- body-like lines longer than 500 characters.

All match counts were zero. The only mailbox-derived content persisted by
Notesmith was the intended short summary in `briefing/email`.

## Cleanup

The scratch vault remains in place at the path above. The isolated daemon was
stopped, and the normal Notesmith.app daemon was restored with the `work` and
`Customer Notes` vaults healthy.
