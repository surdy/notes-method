# Daemon UX Resilience Plan

> Synthesized from independent analyses by Claude Opus 4.7 and GPT 5.5.
> Source reports: `docs/research/daemon-ux-analysis-claude.md`, `docs/research/daemon-ux-analysis-gpt55.md`

## Problem

Notesmith's decoupled daemon architecture (Rust HTTP server ↔ Tauri webview shell) enables CLI, MCP, and multi-client access — but leaks complexity onto users. The daemon is invisible when working, confusing when broken. Users who think "I opened my notes app" are actually running two processes with independent lifecycles, versions, and failure modes. Today there is no connection status UI, no version negotiation, no crash recovery, no one-click restart, and error messages don't explain what to do.

## Design Principle

**The daemon should be invisible when healthy and obvious when unhealthy.** The goal is Obsidian-level simplicity with the architectural advantages of a daemon. Most users should never learn the word "daemon."

## Prior Art

| App | Pattern to adopt |
|---|---|
| Docker Desktop | Menu bar icon + visible status + [Restart] + diagnostics |
| Ollama | Auto-start so seamless users don't know there's a daemon |
| VS Code LSP | [Restart Server] + [View Output] + [Report Issue] trio |
| Obsidian | The UX bar — "it just works" is the ceiling to beat |
| Raycast | Transparent recovery with quiet acknowledgment toast |

---

## Phase 0 — Foundation (sequential, unblocks everything)

### 0.1 `/api/status` endpoint

Replace `/ping` with a rich status endpoint. All downstream UX work depends on this.

```json
{
  "status": "ok",
  "version": "0.2.1",
  "api_schema": 3,
  "pid": 12345,
  "started_at": "2026-05-14T12:00:00Z",
  "binary_path": "/Applications/Notesmith.app/.../notesmith",
  "vaults": [{ "name": "work", "state": "ready", "notes": 421 }],
  "watchers": [{ "vault": "work", "state": "healthy" }],
  "indexes": [{ "vault": "work", "state": "healthy", "last_reindex": "..." }]
}
```

- Keep `/ping` as a lightweight alias (backward compat for scripts)
- Include `version` and `api_schema` for version negotiation
- Include `pid` and `binary_path` for multi-instance detection

### 0.2 Daemon lockfile

Daemon writes a lockfile at `~/Library/Application Support/Notesmith/daemon.lock` (XDG on Linux) containing PID, port, version, start time. All clients (Tauri, CLI, MCP) read this to discover the daemon instead of hardcoding `127.0.0.1:27183`. Stale lockfiles (PID gone) are cleaned up on next start. Prevents multi-instance confusion and port conflicts.

---

## Phase 1 — Startup & Connection (parallel)

### 1.1 Tauri startup orchestration

Replace the current "probe `/ping` → open webview" with a deterministic sequence:

1. Show native splash/loading screen immediately ("Starting Notesmith…")
2. Read lockfile; if present, probe `GET /api/status`
3. If no daemon: launch bundled sidecar, poll `/api/status` for up to 10s
4. If daemon responds but version mismatches bundled sidecar: prompt "Restart to finish updating?" or auto-restart if daemon was launched by this app
5. If port conflict (non-Notesmith process): show actionable error with PID/process name
6. Only load webview after compatibility confirmed
7. If all retries fail: show native fallback screen with [Retry] [Open Diagnostics] [Quit]

**Key outcome**: User never sees a browser "connection refused" page.

### 1.2 Connection status indicator

Add a status pill in the app shell (bottom-left of sidebar):

| State | Indicator | Trigger |
|---|---|---|
| Connected | Green dot | SSE active, last `/api/status` < 30s |
| Reconnecting | Amber, pulsing | SSE dropped, retrying |
| Disconnected | Red dot | API unreachable |
| Restart required | Blue dot | Version mismatch detected |
| Rebuilding index | Gray dot | Reindex in progress |

Click opens a status popover: daemon version, uptime, vault health, [Restart Service] [Rebuild Index] [View Logs].

### 1.3 SSE reconnection with full resync

- Use exponential backoff on SSE disconnect (1s, 2s, 4s, 8s, max 30s)
- On reconnect, do a full resync: reload notes, config, sidebar, badges — not just sidebar
- Add `Last-Event-ID` header support so daemon can replay missed events from a ring buffer
- Surface "Live updates reconnected" toast on successful reconnect
- Show "Live updates disconnected" in status pill while retrying

---

## Phase 2 — Crash Recovery & Version Safety (parallel)

### 2.1 Auto-restart on daemon crash (supervisor)

Tauri keeps a handle on the spawned daemon process. On unexpected exit:

1. Capture last 200 lines of stderr
2. Show non-blocking toast: "Notesmith service stopped unexpectedly. Restarting…"
3. Respawn daemon, verify via `/api/status`
4. On second crash within 60s: stop loop, show modal with [View Error Report] [Restart Anyway] [Quit] and pre-formatted stderr for bug report

### 2.2 Version negotiation

- Frontend sends `X-Notesmith-Client-Version` on every API request
- Daemon responds with `X-Notesmith-Server-Version` and `X-Notesmith-Schema-Version`
- On first API call or SSE connect, frontend compares:
  - Same major: proceed
  - Daemon older: blue banner "Background service is out of date. [Update now]" → triggers Tauri restart flow
  - Daemon newer: banner "Please update the Notesmith app"
- For config schema: when daemon can't parse `vault.toml`, emit SSE `config.error` with parse details → frontend shows modal with error context

### 2.3 Optimistic save with offline queue

- Every save shows "Saving…" → "Saved ✓" indicator near editor
- If save fails: indicator becomes "Save failed — Retrying" and changes queued in IndexedDB
- After retries exhausted: "Save failed — Click to retry" with persistent banner
- On daemon reconnect: auto-flush queued changes

**Key outcome**: User never silently loses work.

---

## Phase 3 — Self-Healing & Diagnostics (parallel)

### 3.1 Index integrity & auto-repair

- On daemon start: fast SQLite + Tantivy consistency check
- On corruption: move bad cache aside, rebuild automatically from markdown files
- Show "Rebuilding search index — search limited for ~30s" banner during rebuild
- Never return empty results without explanation
- Add "Rebuild Index" button in status popover and settings

### 3.2 File watcher diagnostics

- On start: log inotify watch limit (Linux), warn if vault size approaches it
- Surface watcher health in `/api/status` as `"healthy" | "degraded" | "limited"`
- Periodic canary check (every 5 min): lightweight directory hash comparison, trigger partial rescan if diverged
- For network mounts (detected via `statfs`): fall back to polling with visible "Polling mode — updates may take up to 30s"
- Add "Refresh Vault" button near empty states

### 3.3 Actionable error messages

Replace every generic error with a recovery-oriented message:

| Instead of | Show |
|---|---|
| "Connection refused" | "Notesmith service isn't running. [Start it]" |
| "ECONNRESET" | "Lost connection. [Reconnect] — unsaved changes kept locally." |
| "404" on new endpoint | "This feature requires Notesmith v1.4+. You're on v1.3. [Update]" |
| "Failed to fetch" | Silent retry 3×, then status pill turns red |
| Blank page | Native Tauri fallback with logo, explanation, [Restart] [Diagnostics] [Quit] |
| "Search is wrong" | "Search index may be stale. [Rebuild index] — your files are safe." |

### 3.4 Quit semantics & menu bar presence

- `Cmd-Q` closes the window but leaves daemon running (CLI/MCP keep working)
- Second `Cmd-Q` within 5s with no windows: "Also stop the background service?"
- Menu bar icon (like Docker/Ollama/Raycast): shows daemon status, [Open App] [Restart Service] [Stop Service]
- Separate "Close Window" (Cmd-W) from "Quit" (Cmd-Q) from "Stop Background Service" (Diagnostics menu)

---

## Phase 4 — Polish

### 4.1 First-run onboarding

One-time card: "Notesmith runs a small background service so your CLI, agents, and app stay in sync. You'll see a status indicator in the bottom-left." Reframes daemon from "weird hidden thing" to "feature."

### 4.2 Upgrade flow

On Tauri self-update: detect bundled > running daemon version → auto-restart if app-owned, prompt if user-started. Single-question modal: "Update background service? (~2s restart). [Update] [Later]"

### 4.3 Config migration framework

Schema-versioned `vault.toml`. On load, detect old schema, apply migrations, write back. No silent field drops.

### 4.4 CLI auto-start

`notesmith capture`, `notesmith query`, etc. auto-spawn daemon if not running (Ollama pattern). CLI user never manually runs `daemon start`.

---

## Ordering & Dependencies

```
Phase 0:  /api/status  →  lockfile          (sequential, foundation)
Phase 1:  startup orchestration  ‖  status indicator  ‖  SSE reconnect
Phase 2:  crash recovery  ‖  version negotiation  ‖  optimistic save
Phase 3:  index repair  ‖  watcher diagnostics  ‖  error messages  ‖  quit semantics
Phase 4:  onboarding  ‖  upgrade flow  ‖  config migration  ‖  CLI auto-start
```

Phase 0 unblocks all of Phase 1-3.
Phase 1.1 (startup) depends on 0.1 + 0.2.
Phase 2.2 (version negotiation) depends on 0.1.
All items within a phase are parallelizable.
