## Executive Summary

Notesmith's decoupled daemon architecture is technically elegant — one backend serves CLI, MCP, web, and desktop clients uniformly — but it inherits an entire class of UX problems that single-process apps like Obsidian simply don't have. The user's mental model is "I opened my notes app." The reality is "I launched a thin webview that's trying to HTTP-connect to a sidecar process that may or may not be running, may or may not be the right version, may or may not have crashed, and has no built-in path to recover." Every gap between those two models is a moment where a non-technical user will lose trust in the product.

This report walks through the failure modes a real user will hit, the "huh?" moments that erode confidence even when nothing is broken, concrete remediation patterns, and how peer products (Docker Desktop, Ollama, Obsidian, VS Code, Raycast) navigate the same tradeoffs.

---

## 1. Failure Scenarios — What the User Actually Sees

Below, each scenario is framed from the perspective of a user who has no idea what a "daemon" is. They installed an app called Notesmith. That's their model.

### 1.1 Daemon not running when app launches

**What happens technically:** Tauri webview loads `http://127.0.0.1:27183/app/`. TCP connect fails immediately with `ECONNREFUSED`.

**What the user sees:** A blank window, or the browser's default "This site can't be reached" / "Failed to load" page rendered inside a desktop chrome that says "Notesmith" in the title bar. There is no indication this is an app-internal problem versus an internet outage. There's no button. No menu item that suggests recovery. Some users will quit and relaunch, which doesn't help because Tauri doesn't start the daemon. Others will assume the app is broken and uninstall.

**Emotional response:** "I just installed this and it doesn't even open. Is this a scam?"

### 1.2 Daemon crashes mid-session

**What happens technically:** SSE stream dies silently (no `onerror` UI hook). Next API call (save, search, capture) returns network error. Frontend has no reconnection logic and no global error boundary that distinguishes "backend gone" from "your input was invalid."

**What the user sees:** The note they just typed appears to save (optimistic UI) but never actually persists. Or the save spinner hangs forever. Or clicking "New note" does nothing. The sidebar still shows files (they're cached in the SvelteKit store), so the app *looks* fine — until the user reloads, at which point it's a blank page (1.1).

**Emotional response:** "Did it save? Let me try again. Why isn't anything happening? Did I lose my work?" — This is the worst category of failure because the user doesn't know they're in a failure state until they've potentially lost data.

### 1.3 Daemon running old version after upgrade

**What happens technically:** User updates the Notesmith app bundle. Tauri webview launches the new frontend. But the previously-launched `notesmithd` is still running the old binary (it's a separate process supervised by launchd/systemd or just a stray background process). The new frontend may call new API endpoints that the old daemon returns 404 for, or send new request shapes the old daemon rejects with 400.

**What the user sees:** New features advertised in release notes silently don't work. Buttons appear (frontend shipped them) but clicking them produces silent failures or cryptic toasts. Old bugs they read were fixed are still present.

**Emotional response:** "The changelog said this was fixed in 1.4. I'm on 1.4. Why is it still broken? This team ships sloppy releases." The trust damage here is disproportionate because the user blames the *version they just installed*, not an invisible background process.

### 1.4 Config schema mismatch (old daemon, new config format)

**What happens technically:** User edits `vault.toml` in their editor (or a future settings UI writes it). The new schema has a renamed section. The running daemon's TOML deserializer rejects it. ArcSwap doesn't update. SSE `config.changed` never fires — or worse, fires with an error event the frontend doesn't handle.

**What the user sees:** Settings appear to save (the file on disk is correct) but the app behaves as if nothing changed. No error. If they restart the daemon, *then* the new daemon parses it fine and they see their changes — but they have no reason to think "restart the daemon" is a meaningful action.

**Emotional response:** "I set this preference three times and it keeps reverting." This is the classic source of bug reports that are impossible to reproduce because the developer's daemon is fresh.

### 1.5 Port conflict on 27183

**What happens technically:** Another process (a previous orphan `notesmithd`, a misconfigured dev server, a colleague's app that grabbed an arbitrary port) holds `127.0.0.1:27183`. New daemon fails to bind. Or, if bind succeeds (the daemon is running) but the *other* process is what's actually answering on that port, the frontend gets HTTP responses from a totally unrelated server — best case 404, worst case some other app's HTML rendering inside Notesmith's window.

**What the user sees:** Either "can't connect" (1.1) or, in the bizarre cross-talk case, garbage UI. They have no concept of "port" and no tools to diagnose. `lsof -i :27183` is not in their vocabulary.

**Emotional response:** "Why does this app collide with my other apps? Real apps don't do this."

### 1.6 Multiple daemon instances

**What happens technically:** User installs Notesmith via Homebrew for CLI use, then installs the desktop app, which bundles its own daemon. Two `notesmithd` processes try to start. One wins the port; the other fails silently. Or, if they use different ports (some dev override), the CLI talks to one daemon and the desktop app to another — they appear to operate on the same vault but the in-memory caches diverge. Save through one, search through the other, get stale results.

**What the user sees:** "I just captured a note from the CLI but it's not in the app." "Search results disagree depending on which window I'm in." File-watcher-driven reconciliation will *eventually* heal this, but in the meantime the app feels haunted.

**Emotional response:** Confusion that hardens into distrust of the search feature specifically.

### 1.7 SSE connection drops and doesn't reconnect

**What happens technically:** Network blip, laptop sleep/wake, OS firewall transient, daemon GC pause exceeding keepalive. `EventSource` fires `onerror`, transitions to `CLOSED`. Native browser `EventSource` auto-reconnects, but if the frontend uses a custom fetch-based SSE reader (common in SvelteKit setups), it doesn't.

**What the user sees:** Live updates stop. File changes made via CLI or another machine (Syncthing, iCloud) no longer appear in the sidebar. Edits in another window don't reflect. The app feels "frozen in time" but doesn't admit it. A manual refresh fixes it temporarily.

**Emotional response:** "This app isn't real-time like they promised." Users who do collaborative or cross-device workflows (the exact target audience of a power-user notes tool) will notice first and complain loudest.

### 1.8 File watcher stops working

**What happens technically:** `notify` (the Rust file watcher crate) on macOS uses FSEvents; on Linux, inotify with a per-user watch limit (`fs.inotify.max_user_watches`). On large vaults, the limit is silently exceeded and new file events stop firing. Or the user's vault is on a network mount (SMB, NFS, iCloud Drive) where inotify/FSEvents simply don't emit events for remote-originated changes.

**What the user sees:** Files added externally (via git pull, Obsidian sync, mobile app, terminal) don't appear in the sidebar. Edits to existing files don't trigger re-render. Search returns stale content. Daemon restart fixes it, then it degrades again.

**Emotional response:** "It works sometimes." This is the single hardest UX problem to surface because the failure is *the absence of an event*.

### 1.9 Cache/index corruption

**What happens technically:** SQLite vault cache or Tantivy index gets into an inconsistent state — power loss mid-write, disk full, OS update mid-session, a panic in the indexer leaves a half-written segment. On next start, queries return empty results, errors, or partial results.

**What the user sees:** "All my notes are gone." (They aren't — the markdown files on disk are fine — but the *app* shows nothing because the cache is empty/broken.) Search returns nothing for queries that obviously should match. Sidebar is empty despite a populated vault directory.

**Emotional response:** Total panic. This is the single highest-stakes moment in the entire product. A user who believes they've lost a year of notes will not stay calm long enough to discover that `~/Notes/` is intact.

---

## 2. Surprise Moments — Confusion Even When Nothing Is "Broken"

These are subtler than failures but, in aggregate, define whether the product feels trustworthy.

### 2.1 "I changed settings but nothing happened"

User edits `vault.toml` to rename a section or adds a new vault. Hot-reload doesn't cover structural changes. The change is saved. The app shows no indicator that a restart is required. The user reads their setting, sees it's there, and concludes the app is buggy.

**Why it stings:** The mental model "I saved the setting, therefore the setting is applied" is universal and correct in literally every other app. Notesmith violates it silently.

### 2.2 "I updated the app but it behaves the same"

Same as 1.3, but reframed as a *surprise* rather than a failure: even when nothing technically breaks, new features are missing because the old daemon is still serving. The user has no concept that "the app" is two pieces with independent lifecycles.

### 2.3 "The app worked yesterday but today it's blank"

Most likely the daemon was killed by macOS App Nap, a system reboot, an OS update, or a process cleanup. The Tauri app doesn't auto-start the daemon. User opens the app, sees blank. Yesterday everything was fine. There's no narrative bridge between "yesterday" and "today" — the user fills the gap with "this app is unreliable."

### 2.4 "I can see my files in Finder but the app shows nothing"

This is the cache-corruption (1.9) or watcher-lost (1.8) scenario as a surprise moment. The disconnect between file system reality and app reality is jarring because every other notes app (Obsidian, Bear, Apple Notes) treats the file system as ground truth in real time.

### 2.5 "My capture/daily notes aren't appearing"

CLI `notesmith capture` succeeds (writes via daemon). Desktop app doesn't show the new note. Either:
- Different daemon instance (1.6),
- SSE dropped (1.7),
- File watcher silent (1.8),
- Or just — and this is the worst — the desktop app loads its sidebar from a snapshot at startup and never refreshes the listing UI.

User: "I literally just ran the command. Where's my note?"

### 2.6 "Search finds old content that I deleted"

Tantivy index hasn't been updated because:
- File watcher missed the delete (1.8),
- SSE didn't notify the frontend to invalidate (1.7),
- Indexer crashed silently and isn't reprocessing,
- Or the delete happened while the daemon was down and re-scan on start was skipped.

User: "I deleted that note three weeks ago. Why is it still in search? Are my deletes even working? What else hasn't been deleted?" — privacy and security implications follow if the note was sensitive.

### 2.7 "The app started fast yesterday and is hanging today"

First-launch-of-day daemon cold start: re-scanning vault, rebuilding caches, hydrating Tantivy. Subsequent launches are instant because the daemon stays warm. The user experiences inconsistent performance with no visible explanation — they don't know "the daemon went away overnight."

### 2.8 "Quitting the app didn't actually quit it"

Inverse problem. User cmd-Q's Notesmith. The Tauri window closes. The daemon keeps running (which is correct for CLI/MCP scenarios). User thinks they've quit, but `notesmithd` is still in Activity Monitor consuming RAM. When they try to "fully restart" by quitting and reopening, they're not actually restarting the part that matters.

---

## 3. Recommendations — Concrete UX & Implementation Proposals

Organized roughly in order of leverage (highest impact for least cost first).

### 3.1 Health check + connection status (foundational)

**The single most important addition.** Every other recommendation depends on the frontend knowing whether the daemon is reachable.

- Add `GET /healthz` returning `{status, version, build_sha, schema_version, uptime_s, vault_count}`.
- Frontend polls `/healthz` every 5s when SSE is silent, every 30s when SSE is active (as a liveness check independent of SSE).
- Add a persistent **status pill** in the bottom-left of the sidebar: green dot ("Connected"), amber ("Reconnecting…"), red ("Disconnected — Click to restart"). Click to open a status panel with: daemon version, uptime, port, vault path, "Restart daemon" button, "Show logs" button.
- This single UI element resolves the worst part of 1.2, 1.7, 2.3, and 2.7 — the silent-failure-with-no-feedback problem. The user always knows whether they're in a healthy state.

### 3.2 Tauri orchestrates daemon startup

The Tauri shell should be responsible for ensuring a daemon is available before loading the webview. Pseudo-flow:

1. On app launch, Tauri sends `GET /healthz` to `127.0.0.1:27183` with a 500ms timeout.
2. If healthy and `version` matches the bundled daemon: load webview. Done.
3. If healthy but version mismatches: prompt user "An older Notesmith background service is running. Restart it to use the latest features? [Restart] [Continue with old version]". On restart, Tauri sends shutdown signal, waits for port to free, spawns bundled daemon, polls `/healthz`, loads webview.
4. If unreachable: show a native loading screen ("Starting Notesmith…"), spawn bundled daemon as a child process, poll `/healthz` for up to 10s, then load webview. If 10s elapses, show actionable error (see 3.5).
5. If port is occupied by a non-Notesmith process: try ports 27184, 27185, … pass chosen port to webview as a query param.

This eliminates 1.1, 1.3, 1.5, 1.6, 2.2, and 2.3 entirely for the desktop user. CLI users still need their own startup logic (or auto-spawn-on-CLI-call).

### 3.3 Auto-restart on crash (supervisor)

Tauri keeps a handle on the spawned daemon. On unexpected exit (non-zero code, SIGKILL, panic), Tauri:

1. Captures the last 200 lines of daemon stderr.
2. Shows a non-blocking toast: "Notesmith background service stopped unexpectedly. Restarting…"
3. Respawns. On second crash within 60s, stops the loop and shows: "Notesmith keeps crashing. [View error report] [Restart anyway] [Quit]" — with the captured stderr pre-formatted for a bug report.

This addresses 1.2.

### 3.4 Version negotiation

- Frontend sends `X-Notesmith-Client-Version` header on every request.
- Daemon sends `X-Notesmith-Server-Version` and `X-Notesmith-Schema-Version` on every response.
- On handshake (first SSE event or first API call), frontend compares versions:
  - Same major: proceed.
  - Daemon older: show banner "The Notesmith background service is out of date. [Update now]" — clicking triggers the Tauri restart flow (3.2).
  - Daemon newer: show banner "Please update the Notesmith app." (Rare but possible if user updated CLI via brew but not the cask.)
- For schema versions on `vault.toml` / `sidebar.yaml`: when daemon detects a config it can't parse, it should *not* silently keep the old config — it should emit an SSE `config.error` event with the parse error and the offending file. Frontend shows a modal: "Your settings file uses a newer format than this background service. Restart to load? [Restart] [Edit file]". Resolves 1.4 and 2.1.

### 3.5 Error messages with verbs, not nouns

Every daemon-related error message should answer: *what should I do right now?*

| ❌ Instead of | ✅ Show |
|---|---|
| "Connection refused" | "Notesmith's background service isn't running. [Start it]" |
| "ECONNRESET" | "Lost connection to Notesmith. [Reconnect] — Your unsaved changes are kept locally." |
| "404 Not Found" on a new endpoint | "This feature requires Notesmith service v1.4 or later. You're on v1.3. [Update]" |
| "Failed to fetch" | (silent retry up to 3x, then status pill turns red) |
| Blank page on daemon-down | A native Tauri-rendered fallback HTML with: app logo, "Background service unavailable", [Restart Service] [Open Diagnostics] [Quit] buttons |

The fallback HTML in particular addresses 1.1 — the user is never staring at a browser error page.

### 3.6 Optimistic UI with explicit pending state

For 1.2's silent-data-loss failure mode:

- Every save shows a brief "Saving…" → "Saved" indicator near the editor (like Google Docs).
- If save fails, the indicator becomes "Save failed — Retrying" and the changes are queued in IndexedDB.
- If retries exhaust, "Save failed — Click to retry" with a persistent banner. Never let a user believe they've saved when they haven't.
- This is the difference between "the daemon went away and I lost work" and "the daemon went away, I knew immediately, my work was preserved locally, and it auto-flushed when reconnected."

### 3.7 SSE reconnection with exponential backoff

If using a custom SSE reader, replicate the browser's `EventSource` semantics: reconnect with exponential backoff (1s, 2s, 4s, 8s, max 30s), include a `Last-Event-ID` header so the daemon can replay missed events from a small ring buffer. On reconnect, fire a `resync` event that the frontend uses to refetch sidebar/index state. Resolves 1.7 and 2.5.

### 3.8 File watcher self-diagnostics

- On daemon start, log inotify watch limit (Linux) and warn if vault size approaches it. Surface via `/healthz` as `watcher.status: "ok" | "degraded" | "limited"`.
- Periodic (e.g., every 5 min) lightweight directory hash check as a watcher canary: if filesystem state diverges from in-memory state, log a warning and trigger a partial rescan.
- For known-bad mounts (network filesystems detected via `statfs`), automatically fall back to polling mode and surface "Polling mode (network drive detected — updates may take up to 30s)" in the status panel.
- Resolves 1.8 and 2.4.

### 3.9 Index integrity checks

- On daemon start, run a fast Tantivy/SQLite consistency check (segment count, last-known-good marker). On corruption, automatically rebuild from disk (the markdown files are ground truth).
- Surface during rebuild: "Rebuilding search index — search will be limited for ~30s" — never just return empty results.
- Resolves 1.9 and the worst case of 2.6.

### 3.10 Single source of truth for daemon location

- Daemon writes a lockfile at `~/Library/Application Support/Notesmith/daemon.lock` (or XDG equivalent) containing PID, port, version, start time, socket auth token.
- All clients (Tauri, CLI, MCP) read this lockfile to discover the daemon, rather than hardcoding `127.0.0.1:27183`. Stale lockfiles (PID gone) are cleaned up on next start.
- Resolves 1.5 and 1.6 — only one daemon can hold the lock.

### 3.11 Quit semantics

- App menu: separate "Close Window" (cmd-W) from "Quit Notesmith" (cmd-Q) from "Quit Notesmith Background Service" (in a Diagnostics submenu).
- Default cmd-Q closes the window but leaves the daemon running (so CLI/MCP keep working). On second cmd-Q within 5s with no windows open, ask: "Also stop the background service? Background sync, capture, and agent integrations will pause."
- A menu bar icon (like Docker, Ollama, Raycast) showing daemon status and offering "Restart Service" / "Stop Service" / "Open App" gives users control without needing terminal commands. Resolves 2.8.

### 3.12 First-run onboarding sets expectations

A one-time onboarding card: "Notesmith runs a small background service so your CLI, MCP agents, and app all stay in sync. You'll see a status indicator in the bottom-left." This reframes the daemon from "weird hidden thing" to "feature that explains the integrations the user came here for." Lowers the surprise budget for every later issue.

### 3.13 Update flow that handles the daemon

When the Tauri app self-updates:

1. Download new bundle.
2. On relaunch, Tauri detects bundled-daemon-version > running-daemon-version, automatically gracefully shuts down old daemon (`POST /admin/shutdown` with auth token), waits for port, spawns new daemon.
3. Single-question prompt only if active edits would be interrupted: "Update Notesmith? Your background service will restart (~2 sec). [Update] [Later]"

Resolves 1.3 and 2.2 entirely.

---

## 4. How Other Daemon-Based Apps Solve This

### Docker Desktop
- **Pattern:** Heavy native shell that owns the daemon lifecycle. Menu bar icon shows three states (running/starting/stopped) with explicit color coding.
- **Strengths:** Status is always visible. "Restart" and "Quit Docker Desktop" are first-class menu items. When the daemon (`dockerd` inside the VM) is down, every CLI command emits a clear `Cannot connect to the Docker daemon. Is the docker daemon running?` and the GUI shows a banner with a [Start] button.
- **Weaknesses:** Slow to start, opaque about *why* it's slow. Update flow is heavy-handed (full quit + relaunch).
- **Lesson for Notesmith:** Adopt the menu-bar status icon and the "shell owns the daemon" model (recommendation 3.2 + 3.11).

### Ollama
- **Pattern:** Mac app is a thin wrapper that ensures the local server is running on `127.0.0.1:11434`. CLI commands transparently start the server if not running.
- **Strengths:** Auto-start is so seamless most users don't know there's a daemon. Menu bar icon is minimal but present.
- **Weaknesses:** No version negotiation visible to users; updates require manual restart of the menu bar app, but this isn't well surfaced. Logs are hidden.
- **Lesson:** "Auto-start on first request" (from CLI as well as GUI) is a great default — apply to `notesmith capture` and similar CLI commands so they spawn the daemon if absent.

### VS Code with language servers
- **Pattern:** Editor owns LSP server lifecycle entirely. If a language server crashes, VS Code shows a notification with [Restart Server] and offers to file an issue with logs attached. Output panel always shows server logs on demand.
- **Strengths:** Crash recovery is one click. Logs are accessible without leaving the app. Multiple servers can coexist with explicit per-language status.
- **Weaknesses:** When LSP misbehaves silently (slow, partial responses), users don't know whether to blame VS Code, the extension, or their code.
- **Lesson:** The "[Restart Server] + [View Output] + [Report Issue]" trio in a single notification is the gold standard for crash recovery. Adopt for 3.3.

### Obsidian (single process)
- **Pattern:** Everything runs in one Electron process. No daemon. File watcher, search index, plugins all in-process.
- **Strengths:** Zero daemon-class problems. Mental model is simple: app open = features work, app closed = nothing happens.
- **Weaknesses:** No CLI integration, no agent integration, no headless server mode, no multi-client. Plugins can crash the whole app. Sync requires a paid service or third-party plugins.
- **Lesson:** This is the ceiling Notesmith must beat on UX while preserving the architectural advantages. Every daemon-related friction point is something Obsidian users won't tolerate. The bar is "feels as seamless as Obsidian, *plus* CLI and agents work."

### iTerm2 shell integration
- **Pattern:** Optional, opt-in installer (`curl ... | bash`) that adds shell hooks. No daemon, but a parallel "thing the user installed that has its own version."
- **Strengths:** Banner inside iTerm tells you when shell integration is out of date and offers to update.
- **Weaknesses:** Manual install, easy to forget on new machines.
- **Lesson:** Version mismatch banners are normal and accepted by users when the action is one click. Adopt for 3.4.

### Raycast
- **Pattern:** Single menu bar app with extensions. No separate daemon process visible to the user, but extensions run in isolated Node workers. If a worker crashes, Raycast restarts it transparently with a small toast.
- **Strengths:** Recovery is invisible-but-acknowledged. The app *tells you* something happened without making you fix it.
- **Weaknesses:** When the main Raycast process is down, the global hotkey just doesn't fire — nothing tells you why.
- **Lesson:** Transparent recovery with a quiet acknowledgment ("Reconnected") is better than either silent recovery (user wonders if it broke) or loud recovery (user's flow is interrupted).

### Synthesis

| App | Lifecycle owner | Status surface | Crash recovery | Version handling |
|---|---|---|---|---|
| Docker Desktop | GUI shell | Menu bar icon + dashboard | Manual restart, prompted | Update prompts, full restart |
| Ollama | GUI + CLI auto-spawn | Menu bar icon | Implicit (next request restarts) | Manual |
| VS Code LSP | Editor | Status bar + output panel | Notification with [Restart] | Per-extension, surfaced |
| Obsidian | n/a (single process) | n/a | n/a | App-level only |
| iTerm2 integration | Manual | In-terminal banner | n/a | Banner with [Update] |
| Raycast | Single app | Menu bar | Transparent + toast | Transparent |

**Notesmith should target Ollama-style auto-start + Docker-style menu bar visibility + VS Code-style crash recovery + iTerm2-style version banners.** That combination directly maps to recommendations 3.2, 3.11, 3.3, and 3.4.

---

## 5. Prioritized Roadmap

If the team can only do a few things, in order:

1. **`/healthz` endpoint + status pill in sidebar** (3.1) — unblocks all other UX work; ~2 days of effort.
2. **Tauri auto-spawns and supervises the daemon** (3.2 + 3.3) — eliminates the "blank window on launch" failure that defines first impressions; ~1 week.
3. **Optimistic UI with explicit save state + offline queue** (3.6) — protects against data loss, the highest-stakes failure class; ~1 week.
4. **Lockfile-based daemon discovery** (3.10) — eliminates port conflicts and multi-instance confusion before they become support tickets; ~2 days.
5. **Version headers + mismatch banner** (3.4) — closes the "I updated and nothing changed" gap; ~3 days.
6. **Native Tauri fallback page when daemon unreachable** (3.5) — replaces the browser error page with something on-brand and actionable; ~2 days.
7. **SSE reconnection with `Last-Event-ID`** (3.7) — restores the "real-time" promise; ~3 days.
8. **Menu bar icon + quit semantics** (3.11) — establishes ongoing trust; ~1 week.
9. **Watcher diagnostics + index integrity checks** (3.8 + 3.9) — addresses the long-tail "haunted app" issues; ~2 weeks.

Items 1–4 alone would resolve the catastrophic failure modes (1.1, 1.2, 1.5, 1.6, 1.9 partial) and the most common surprise moments (2.1, 2.2, 2.3). Everything beyond is polish that compounds trust over time.

---

## Closing Thought

The decoupled daemon is the right architecture — it's what enables CLI, MCP agents, multiple clients, and headless deployments to share one consistent backend. But every architecture leaks its complexity somewhere, and right now Notesmith leaks all of it onto the user. The good news: this is a solved problem, repeatedly, by the apps above. The fix isn't to hide the daemon (Obsidian's path, which forfeits the reason Notesmith exists) but to **make the daemon a legible, controllable, self-healing part of the product** — visible enough that users trust it, transparent enough that they don't think about it.