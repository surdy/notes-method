# UX Analysis: Daemon-Decoupled Architecture in Notesmith

## Executive summary

Notesmith's daemon-decoupled design is powerful for CLI, MCP, browser, and desktop reuse, but it creates a hidden dependency that non-technical users will not understand: the "app" they launch is only a window pointed at a local server. The current Tauri shell does auto-start the daemon if `/ping` fails, but if any daemon responds successfully, it assumes everything is healthy and opens `http://127.0.0.1:27183/app/` rather than an embedded UI. That means stale daemons, failed upgrades, port conflicts, config mismatches, and watcher/index problems can all surface as confusing "the app is blank," "nothing changed," or "search is wrong" experiences.

The highest-impact product fix is to make daemon status explicit and managed: a startup supervisor with version compatibility checks, visible connection state, one-click restart/reindex actions, and plain-language recovery messages.

---

## 1. Current architecture evidence

Key implementation facts:

- The desktop window points to the daemon-served app at `http://127.0.0.1:27183/app/`.
- Tauri starts by calling `ensure_daemon_running_with`, which only probes `/ping`; if it succeeds, it does not start or replace the daemon.
- The daemon serves `/ping`, capabilities, APIs, SSE, and `/app` static files from Axum.
- Frontend API calls use same-origin `API_BASE = ''`, so if the page loads from an old/wrong daemon, all calls go to that daemon.
- Per-vault config and sidebar changes are watched and can emit `config.changed`/`config.error`; `vault.toml` is hot-swapped into `ArcSwap`.
- Global vault registration writes the global config file, but does not add a new `VaultState` or watcher to the running daemon.
- There is no version/status compatibility endpoint in the router; only `/ping` and `/api/capabilities` exist for generic health/capability checks.

---

## 2. Failure scenarios from a non-technical user's perspective

| Scenario | What happens technically | What the user sees / feels | What they should see instead |
|---|---|---|---|
| Daemon not running at launch | Tauri probes `/ping`; if it fails, it launches `notesmith daemon start`, waits up to 10s, then errors if still unavailable. | Best case: app opens after delay. Worst case: no useful window, or app fails before UI is shown because setup fails. | Native startup screen: "Starting Notesmith..." with progress, logs link, Retry, Quit, Open Diagnostics. |
| Daemon crashes mid-session | Existing page remains loaded, but API calls fail. Some actions show generic alerts like "Failed to capture note". | User clicks Capture/Search/Daily and gets generic failures; app may look alive but stale. | Persistent banner: "Notesmith service disconnected. Reconnecting... Restart service." Disable writes until recovered. |
| Old daemon after app upgrade | Tauri only checks `/ping`; if old daemon responds, it does not launch the bundled sidecar. Since the daemon serves `/app`, the user may even see the old frontend. | "I updated Notesmith but nothing changed." New features missing, bugs still present. | Version mismatch dialog: "A previous Notesmith service is still running. Restart it to finish updating." |
| Config schema mismatch | `vault.toml` is parsed into current `VaultConfig`; parse failures produce errors, and old/new fields can be silently ignored or fail depending on schema evolution. | "Settings saved but don't work," or "my config is broken." | Config migration screen with schema version, "Back up and migrate," and exact file/line error where possible. |
| Port conflict on 27183 | Daemon bind fails with contextual error. But Tauri launches with stdout/stderr discarded. | App may never open or times out with little context. | "Port 27183 is in use by another process. Use different port / Quit other service / Show process." |
| Multiple daemon instances | Default bind prevents two on same port, but `--bind` allows other instances. Tauri always targets default URL unless env override is set. | User edits one daemon's config but app talks to another; CLI and app disagree. | Single-instance lock/PID ownership. "Another Notesmith service is already running: version/path/PID." |
| SSE drops or misses events | Client uses `EventSource`; on error it only logs to console, and on reconnect it reloads sidebar config only. | External file changes, captures, or daily notes may not appear until reload. | Visible "Live updates disconnected" indicator; full refresh after reconnect. |
| File watcher stops working | Notify errors are ignored; processing errors are logged as warnings only. | "I can see files in Finder but app doesn't update." | Watcher health in status menu + "Refresh vault / Rebuild index." |
| Cache/index corruption | SQLite and Tantivy open/reindex errors propagate during startup. Manual reindex exists only if daemon is healthy. | "Search is wrong," "notes disappeared," or app fails to start. | "Your markdown files are safe. The search index needs to be rebuilt. Rebuild index?" |

---

## 3. Surprise moments

### "I changed settings but nothing happened"

Per-vault `vault.toml` hot-reloads, but global vault registration does not update daemon state until restart. The app can write a new vault registration, while the daemon's in-memory `vaults` map is built at startup.
**UX fix:** After adding/removing/renaming a vault, show: "Restart required to activate this vault." Offer one-click restart.

### "I updated the app but it behaves the same"

Because Tauri accepts any successful `/ping`, it can keep using an old daemon.
**UX fix:** Compare desktop bundle version, sidecar version, daemon version, API schema version, and frontend build hash.

### "The app worked yesterday but today it's blank"

If the daemon fails to bind/start, Tauri may fail during setup before showing a recovery UI.
**UX fix:** Always create a native recovery window first; then transition to the web app after daemon readiness.

### "I can see my files in Finder but the app shows nothing"

The sidebar uses cached note summaries from `/api/v/{vault}/notes`; if watcher/indexing is stale, the tree is stale.
**UX fix:** Add "Refresh vault" and "Rebuild index" buttons near empty/error states.

### "My capture/daily notes aren't appearing"

Capture and daily commands depend on API calls and then reload notes. If daemon is unavailable or stale, the operation may fail generically.
**UX fix:** Use specific error copy: "Could not contact Notesmith service. Your note was not saved. Restart service."

### "Search finds old content that I deleted"

Search is backed by Tantivy and updated by watcher events; missed watcher events can leave stale results until reindex.
**UX fix:** Add "Search index last updated" and "Rebuild search index" affordance.

---

## 4. Product recommendations

### A. Startup orchestration

Implement a deterministic startup sequence:

1. Show native startup screen immediately.
2. Check `GET /api/status`, not just `/ping`.
3. If no daemon: launch bundled sidecar.
4. If daemon exists but version/build/API schema mismatch: ask to restart or auto-restart if owned by this app.
5. If port conflict: show process/PID and remediation.
6. Load `/app/` only after compatibility is confirmed.

Proposed `/api/status`:

```json
{
  "status": "ok",
  "daemon_version": "0.2.1",
  "api_schema": 3,
  "frontend_build": "2026-05-14.abc123",
  "pid": 12345,
  "binary_path": "/Applications/Notesmith.app/.../notesmith",
  "started_at": "2026-05-14T12:00:00Z",
  "vaults": [{ "name": "work", "state": "ready", "notes": 421 }],
  "watchers": [{ "vault": "work", "state": "healthy" }],
  "indexes": [{ "vault": "work", "state": "healthy", "last_reindex": "..." }]
}
```

### B. One-click daemon restart

Add Tauri commands:

- `daemon_status`
- `daemon_restart`
- `daemon_stop`
- `daemon_open_logs`
- `daemon_reindex_vault`

Only enable for local desktop mode. The existing capabilities endpoint already provides a pattern for server-driven feature flags.

### C. Connection status UI

Add a small status indicator in the app shell:

- Green: "Connected"
- Yellow: "Reconnecting..."
- Red: "Service unavailable"
- Blue: "Restart required"
- Gray: "Index rebuilding"

Do not hide daemon state in console logs. The current SSE error handler logs only to console.

### D. Better error messages

Replace generic messages like "Failed to list notes: 500" with:

- "Notesmith service is not running. Start or restart it."
- "This app was updated, but the background service is still old."
- "Your files are safe. The search index needs to be rebuilt."
- "Vault registration changed. Restart Notesmith to activate it."

### E. Upgrade handling

During app update:

1. New desktop app launches.
2. It probes daemon status.
3. If daemon version differs from bundled sidecar:
   - If daemon is owned by Notesmith desktop: restart automatically.
   - If user-started: ask permission.
4. After restart, verify `/api/status` and reload.

### F. Cache/index recovery

Add automatic corruption handling:

- If SQLite open fails, move bad cache aside and rebuild.
- If Tantivy open fails, delete/recreate index directory.
- Show "Your markdown files are the source of truth; rebuilding only affects cached search/listing."

### G. SSE robustness

Current server emits named SSE events, while the client only uses `onmessage` plus generic `onerror`. Ensure the client listens to named events with `addEventListener(...)` or send default `message` events. On reconnect, reload notes, config, sidebar, and badges, not only sidebar.

---

## 5. Comparison with similar apps

### Docker Desktop

Docker Desktop is a good model for daemon UX. It exposes a Troubleshoot menu with **Restart Docker Desktop**, diagnostics collection, reset options, and support flows. Notesmith should copy the pattern: a visible service status menu, restart action, diagnostics bundle, and safe reset/rebuild actions.

### Ollama

Ollama also runs a local HTTP service. Its docs acknowledge that config changes require restarting the app/service and that updates require "Restart to update." Notesmith should similarly make restart-required states explicit, especially after upgrades or environment/config changes.

### VS Code language servers

VS Code language servers run in separate processes to isolate heavy CPU/memory work from the editor. The UX lesson: process separation is acceptable when the host owns lifecycle, logs, restarts, and user-visible diagnostics. Notesmith should treat its daemon like VS Code treats language servers: managed, restartable, observable.

### Obsidian

Obsidian's simpler single-process mental model avoids this class of confusion. Users open the app and directly see their vault. Notesmith's daemon architecture is more powerful, but the UX must preserve the same illusion: "Notesmith is running" should be the only concept most users need.

### iTerm2 shell integration

iTerm2's shell integration is technically complex, but the product exposes clear install paths: automatic load, menu-based install, or manual install. Notesmith can borrow this layered approach: automatic daemon management for most users, advanced CLI controls for technical users.

### Raycast

Raycast extensions update in the background and only compatible extensions appear in results. Notesmith should similarly hide incompatible states when possible and make compatibility checks automatic.

---

## 6. Priority roadmap

### P0 — must fix before broad non-technical release

1. Add `/api/status` with daemon/app/API/schema versions.
2. Add native startup/recovery window.
3. Add version mismatch detection and restart flow.
4. Add connection banner/status indicator.
5. Fix SSE named-event handling and full refresh on reconnect.
6. Add one-click "Restart Notesmith service."

### P1 — important polish

1. Show "Restart required" after global vault registry changes.
2. Add "Rebuild index" from app.
3. Add watcher/index health to status menu.
4. Preserve logs and expose "Open diagnostics."
5. Improve all API error messages with recommended actions.

### P2 — resilience

1. PID/lockfile ownership model.
2. Automatic cache/index repair.
3. Background service supervisor with crash backoff.
4. Migration framework for config schema changes.
5. Tray/menu service controls.

---

## Bottom line

The architecture is sound, but the daemon must become invisible when healthy and obvious when unhealthy. Today, users can easily experience blank screens, stale behavior after upgrades, silent missed updates, and confusing cache/search drift. The product should adopt a "managed local service" UX: explicit status, compatibility checks, restart/rebuild controls, and recovery-first error messages.
