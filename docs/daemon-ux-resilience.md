# Daemon UX Resilience Plan

> Synthesized from independent analyses by Claude Opus 4.7 and GPT 5.5.
> Source reports: `docs/research/daemon-ux-analysis-claude.md`, `docs/research/daemon-ux-analysis-gpt55.md`
> Decisions refined via grill session (2026-05-14).

## Problem

Notesmith's decoupled daemon architecture (Rust HTTP server ↔ Tauri webview shell) enables CLI, MCP, and multi-client access — but leaks complexity onto users. The daemon is invisible when working, confusing when broken. Users who think "I opened my notes app" are actually running two processes with independent lifecycles, versions, and failure modes. Today there is no connection status UI, no version negotiation, no crash recovery, no one-click restart, and error messages don't explain what to do.

## Design Principles

- **The daemon should be invisible when healthy and obvious when unhealthy.** The goal is Obsidian-level simplicity with the architectural advantages of a daemon. Most users should never learn the word "daemon."
- **Browser and desktop are both first-class.** All frontend resilience (status indicator, SSE reconnect, optimistic save, error messages) must work in both Tauri and plain browser mode.
- **macOS and Linux are the primary targets.** Use `~/Library/` paths on macOS, XDG on Linux. Windows support deferred.
- **No auth for now.** Local-only trust model. Revisit when/if the daemon is exposed beyond localhost.
- **Fixed port (27183), no dynamic fallback.** Clear error showing what process holds the port, offer to kill or configure a different port in settings.

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
  "version": "0.1.0",
  "api_schema": 1,
  "pid": 12345,
  "started_at": "2026-05-14T12:00:00Z",
  "binary_path": "/Applications/Notesmith.app/.../notesmith",
  "vaults": [{ "name": "work", "state": "ready", "notes": 421 }],
  "watchers": [{ "vault": "work", "state": "healthy" }],
  "indexes": [{ "vault": "work", "state": "healthy", "last_reindex": "..." }],
  "resources": {
    "rss_bytes": 52428800,
    "open_fds": 47,
    "sse_connections": 2,
    "cache_size_bytes": 1048576
  }
}
```

- Keep `/ping` as a lightweight alias (backward compat for scripts)
- Include `version` and `api_schema` for version negotiation
- Include `pid` and `binary_path` for multi-instance detection
- Include `resources` for diagnostics (RSS, open FDs, SSE connections, cache size)
- Initial implementation can report `"healthy"` placeholders for watcher/index state until deeper health probes land

### 0.2 Daemon lockfile

Daemon writes a lockfile at `~/Library/Application Support/Notesmith/daemon.lock` (macOS) or `$XDG_RUNTIME_DIR/notesmith/daemon.lock` (Linux) containing PID, port, version, start time. All clients (Tauri, CLI, MCP) read this to discover the daemon instead of hardcoding `127.0.0.1:27183`. Stale lockfiles (PID gone) are cleaned up on next start. Prevents multi-instance confusion and port conflicts.

### 0.3 Structured logging

Daemon logs to file using `tracing-appender`:
- macOS: `~/Library/Logs/Notesmith/daemon.log`
- Linux: `$XDG_STATE_HOME/notesmith/daemon.log` (or `~/.local/state/notesmith/`)
- Daily rotation, 7-day retention
- Tauri also captures sidecar stderr as crash-report fallback
- `GET /admin/logs?tail=200` endpoint for browser-mode log access
- "View Logs" opens log file via system default (`open` on macOS, `xdg-open` on Linux)

### 0.4 Graceful shutdown

Use Axum's built-in `graceful_shutdown` with a 3-second drain window:
1. Daemon receives shutdown signal (SIGTERM, or `POST /admin/shutdown`)
2. Emit SSE `shutting_down` event so frontend can pause writes and queue
3. Stop accepting new connections
4. Wait up to 3s for in-flight requests to complete
5. Exit

`POST /admin/restart` endpoint for browser-mode users (triggers shutdown + relies on supervisor or CLI auto-start to respawn).

---

## Phase 1 — Startup & Connection (parallel)

### 1.1 Tauri startup orchestration

Replace the current "probe `/ping` → open webview" with a deterministic sequence:

1. Show native splash/loading screen immediately ("Starting Notesmith…")
2. Read lockfile; if present, probe `GET /api/status`
3. If no daemon: launch bundled sidecar, poll `/api/status` for up to 10s
4. If daemon responds but version mismatches bundled sidecar: prompt "Restart to finish updating?" or auto-restart if daemon was launched by this app
5. If port conflict (non-Notesmith process): show actionable error with PID/process name, offer to kill it or configure a different port
6. Only load webview after compatibility confirmed
7. If all retries fail: show native fallback screen with [Retry] [Open Diagnostics] [Quit]

**Key outcome**: User never sees a browser "connection refused" page.
**Desktop-only**: Browser users rely on connection status indicator (1.2) and `/admin/restart` (0.4).

### 1.2 Connection status indicator

Add a status pill in the app shell (bottom-left of sidebar). **Works in both Tauri and browser mode.**

| State | Indicator | Trigger |
|---|---|---|
| Connected | Green dot | SSE active, last `/api/status` < 30s |
| Reconnecting | Amber, pulsing | SSE dropped, retrying |
| Disconnected | Red dot | API unreachable |
| Restart required | Blue dot | Version mismatch detected |
| Rebuilding index | Gray dot | Reindex in progress |

Click opens a status popover showing: daemon version, uptime, vault health, resource stats, [Restart Service] [Rebuild Index] [View Logs].

In browser mode, [Restart Service] calls `POST /admin/restart`. In Tauri mode, it uses the Tauri command.

### 1.3 SSE reconnection with full resync

**Works in both Tauri and browser mode.**

- Use exponential backoff on SSE disconnect (1s, 2s, 4s, 8s, max 30s)
- On reconnect, do a full resync: reload notes, config, sidebar, badges — not just sidebar
- Add `Last-Event-ID` header support so daemon can replay missed events from a ring buffer
- Surface "Live updates reconnected" toast on successful reconnect
- Show "Live updates disconnected" in status pill while retrying

### 1.4 Sleep/wake resync

Proactive resync after system sleep/wake, rather than waiting for SSE timeout:

- **Tauri**: Listen for OS wake events (`NSWorkspaceDidWakeNotification` on macOS, equivalent on Linux), emit custom event to webview
- **Browser**: Listen for `document.visibilitychange` as a weaker proxy
- On wake/visibility-restore: immediately probe `/api/status`, trigger full resync (notes, config, sidebar)
- Eliminates the "stale for 5–30 seconds after wake" window

### 1.5 Hot-reload vault registration

Make vault add/remove/rename work without daemon restart:

- Watch the global config file (`~/.config/notesmith/config.toml`) the same way `vault.toml` is watched
- On change, dynamically spin up new `VaultState` + file watcher + cache for added vaults
- Tear down removed vaults
- Emit SSE `vaults.changed` event so frontend refreshes vault list
- No "restart required" messaging for vault changes

---

## Phase 2 — Crash Recovery & Version Safety (parallel)

### 2.1 Auto-restart on daemon crash (supervisor)

**Tauri desktop-only.** Browser users rely on `/admin/restart` and CLI auto-start.

Tauri keeps a handle on the spawned daemon process. On unexpected exit:

1. Capture last 200 lines of stderr (from log file, not discarded pipe)
2. Show non-blocking toast: "Notesmith service stopped unexpectedly. Restarting…"
3. Respawn daemon, verify via `/api/status`
4. On second crash within 60s: stop loop, show modal with [View Error Report] [Restart Anyway] [Quit] and pre-formatted stderr for bug report

### 2.2 Version negotiation

**Works in both Tauri and browser mode.**

- Frontend sends `X-Notesmith-Client-Version` on every API request
- Daemon responds with `X-Notesmith-Server-Version` and `X-Notesmith-Schema-Version`
- On first API call or SSE connect, frontend compares:
  - Same major: proceed
  - Daemon older: blue banner "Background service is out of date. [Update now]" → Tauri triggers restart flow, browser shows [Restart Service]
  - Daemon newer: banner "Please update the Notesmith app"
- For config schema: when daemon can't parse `vault.toml`, emit SSE `config.error` with parse details → frontend shows modal with error context

### 2.3 Optimistic save with offline queue

**Works in both Tauri and browser mode.**

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
- Single "Rebuild Index" button in status popover and settings — rebuilds both SQLite cache and Tantivy index. Internally smart: check each independently, skip if clean
- CLI gets granular flags: `notesmith reindex --cache-only`, `--search-only`

### 3.2 File watcher diagnostics

- On start: log inotify watch limit (Linux), warn if vault size approaches it
- Surface watcher health in `/api/status` as `"healthy" | "degraded" | "limited"`
- Periodic canary check (every 5 min): lightweight directory hash comparison, trigger partial rescan if diverged
- For network mounts (detected via `statfs`): fall back to polling with visible "Polling mode — updates may take up to 30s"
- Add "Refresh Vault" button near empty states

### 3.3 Actionable error messages

**Works in both Tauri and browser mode.**

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

**Desktop-only.**

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

### 4.4 CLI & MCP auto-start

- `notesmith capture`, `notesmith query`, etc. auto-spawn daemon if not running (Ollama pattern). CLI user never manually runs `daemon start`.
- MCP server (`notesmith mcp start`) also auto-starts daemon if not running.
- MCP retries failed HTTP calls once with 3s backoff before returning error.
- MCP returns structured errors hinting agents to retry ("daemon restarting, try again in 5s").

---

## Ordering & Dependencies

```
Phase 0:  /api/status → lockfile → logging → graceful shutdown  (sequential, foundation)
Phase 1:  startup orchestration ‖ status indicator ‖ SSE reconnect ‖ sleep/wake ‖ vault hot-reload
Phase 2:  crash recovery ‖ version negotiation ‖ optimistic save
Phase 3:  index repair ‖ watcher diagnostics ‖ error messages ‖ quit semantics
Phase 4:  onboarding ‖ upgrade flow ‖ config migration ‖ CLI/MCP auto-start
```

Phase 0 unblocks all of Phases 1–3.
Phase 1.1 (startup) depends on 0.1 + 0.2.
Phase 1.5 (vault hot-reload) depends on 0.2 (lockfile/global config watching pattern).
Phase 2.2 (version negotiation) depends on 0.1.
All items within a phase are parallelizable.

## Scope Decisions

- **macOS + Linux primary targets.** Windows deferred. Use platform-appropriate paths (~/Library/ on macOS, XDG on Linux) and APIs.
- **Not changing the architecture**: daemon + webview is correct for CLI/MCP/multi-client.
- **Not embedding backend in Tauri**: defeats the multi-client purpose.
- **No daemon auth for now**: local-only trust model. Revisit when exposing beyond localhost.
- **Fixed port (27183), no dynamic fallback**: clear error + offer to kill occupant or configure different port.
- **Auto-restart is opt-out, not opt-in**: default is "it just works."
- **Single "Rebuild Index" for users**: internally smart (check each index independently). CLI gets `--cache-only`/`--search-only` flags.
- **`window.prompt()` replacement**: separate issue, not part of this plan.

## Validation

Each item must pass:
- `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --all -- --check`
- `npx svelte-check` (zero new errors)
- Manual smoke test of the specific failure scenario it addresses
