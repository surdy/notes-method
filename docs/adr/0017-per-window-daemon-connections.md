# ADR 0017 — Per-Window Daemon Connections

## Status

Accepted (2026-06), fully implemented across Phases A–D. Extends and **amends
Decisions 3 and 4 of [ADR 0014](0014-desktop-connection-management.md)** (the
app-global active selection and the renavigate-every-window switch). Builds on
the desktop transport model of [ADR 0007](0007-sveltekit-tauri.md) and the
remote-daemon mode of [ADR 0012](0012-agent-transport-acp-mcp.md). Implements
GitHub epic **#221** (per-window connections), the follow-on to the now-complete
epic #176.

**Implementation note.** The app-global `DaemonUrlState` is gone; each window
owns its connection via the `WindowRegistry` (`window_registry.rs`). The
status-bar control is now a per-window **badge** (`ConnectionSwitcher.svelte`,
`connection/badge-view.ts`), not a global switcher; the "New Window" menu groups
vaults by server; and `servers.json`'s `active_id` now functions purely as a
**non-destructive default** (drives new-window / menu defaults), never
retargeting open windows. Destructive server edits (URL change, removal) are
blocked while windows are open against the server. Phases A–D: GitHub
#222–#240.

## Context

ADR 0014 made remote connections first-class but kept **one active connection
for the whole app**: `DaemonUrlState(Mutex<String>)` is a single global URL,
`effective_settings`/`frontend_mode` derive from one `ServersFile.active_id`,
and `connection_set_active` re-points **every** open window at the new daemon.

This blocks the workflow we actually want: keeping a **local** working vault and
a **remote** vault (e.g. a self-hosted "memory" server) open **at the same
time**. Switching connection today yanks all windows to one server.

The desktop is already most of the way to per-window connections:

- Each webview receives its daemon via an **`apiBase` query param** baked into
  its window URL; the frontend's `API_BASE`, SSE stream, and embedded agent-chat
  all read that param. **The data/render layer is already per-window.** Only the
  Tauri-side source of truth (which daemon) is global.
- Remote windows use `FrontendMode::Embedded`: the app shell loads from the
  **local** bundled `notesmith-app://localhost/app/`, with the remote supplied
  only as `apiBase`. So a remote daemon being unreachable does **not** blank the
  window — the shell still renders and shows a recoverable offline state. (This
  is why we do **not** treat "offline remote window" as a blocker, and why
  frontend version skew is a non-issue: the remote's frontend assets are never
  used — only its API.)

What is *not* yet per-window and must change:

| Area | Current single-connection assumption |
| --- | --- |
| `DaemonUrlState(Mutex<String>)` | One daemon URL for the whole process |
| `VaultWindows: HashMap<vault_name, label>` | Windows keyed by **vault name only** |
| `vault_window_label(vault)` | Label derived from the **name only** → same-named vaults on two servers collide |
| `effective_settings` / `frontend_mode` | Derived from the single active server |
| `current_daemon_url` / `current_vault_app_url` | Global daemon for all URL building |
| `connection_set_active` → `renavigate_app_windows` | A switch re-points **every** window |
| IPC: ping / shutdown / restart / status | Read the global `current_daemon_url` |
| `registered_vault_names()` | Native + tray menu read **local** config, ignore the connection |
| `windows.json` (`WindowEntry{vault, geometry}`) | No server identity → restore uses the active connection |
| Diagnostics (restart/stop/logs) | Operate on the local sidecar |
| "Open Folder as Vault" | Native local file picker; wrong for a remote host |

## Decision

### 1. The daemon connection is a property of a **window**, not the app

Introduce an explicit window-context model and resolve every daemon-targeted
operation from the **calling window's** context rather than a global:

```text
WindowContext =
    Global                              // onboarding / main / settings chrome
    ServerScoped { server_id }          // a window pinned to a server, no vault yet
    VaultScoped  { server_id, vault }   // a vault window
```

`server_id` is the existing **stable** `servers.rs` id (a slug minted once at
`add()` and never changed by rename/URL edits; `LOCAL_ID = "local"` is the
reserved local sentinel). We do **not** need UUIDs — the slug id is already
stable across renames, which is the only stability property that matters.

### 2. One authoritative window registry, not mirrored maps

Replace the global `DaemonUrlState` and the name-keyed `VaultWindows` with a
**single** registry plus indexes, updated transactionally and cleaned up on
window close:

- `window_label → WindowContext` (authoritative)
- `(server_id, vault) → window_label` (reuse/focus index)

This avoids the drift bug where a label points at one vault but IPC resolves a
different server.

### 3. Server-qualified window identity

`vault_window_label` and the reuse index key on **`(server_id, vault)`**, so the
same vault name on two servers yields two distinct windows. Labels are derived
from the stable `server_id` + a hash of the canonical `(server_id, vault)`
identity — never from the mutable display name or URL.

### 4. Connection-parameterized URL builders and window-aware IPC

`current_vault_app_url`, `frontend_mode`, and `effective_settings` take the
**target connection** (resolved from the window's context), not the global
active selection. Daemon-targeted IPC commands (`ping`, status, SSE wiring)
thread `tauri::Window` and resolve **that window's** daemon. Commands declare
the `WindowContext` they require and fail clearly when invoked from a window
that lacks it (e.g. a vault-scoped command from a Global settings window).

Per-server **auth tokens** (already stored on `ServerEntry`) are resolved by
`server_id` and attached as headers for menu probes, IPC, API, and SSE — never
placed in query params.

### 5. Diagnostics and the local daemon stay **local-scoped**

Restart / Stop / View Logs manage the local sidecar and are relabelled
accordingly ("Restart **Local** Service", "View **Local** Service Logs").
They are de-emphasized (but reachable) in a remote-only setup. Switching a
window to a remote server never spawns a local daemon; the local daemon is
started only when a **local** window needs it.

### 6. Multi-server "New Window" menu

Build the native (and tray) "New Window" menu from **all** configured servers,
grouped by server, each entry showing a local/remote icon and the server name
(remote only; local implicit):

- Maintain a **per-server cached** vault list (`/api/app/vaults`) with a
  last-seen timestamp. Refresh asynchronously with short timeouts and bounded
  concurrency; render **stale data when a server is offline** and grey it out;
  offer a manual refresh. Rebuild the menu on server config / auth / vault
  changes.
- Menu item ids carry the structured `(server_id, vault)` identity (encoded so
  delimiters / unicode / duplicate vault names across servers are unambiguous).

### 7. A **demoted** global default connection

Keep a single `default_server_id`, but make it **non-destructive**: it selects
the server for the onboarding / main / settings chrome, orders the menu groups,
seeds legacy window migration, and is the default target for "Open Folder as
Vault" — **but it never retargets already-open windows**. It is changed
explicitly in Settings, not via a status-bar "switch all windows" affordance.

### 8. Status bar becomes a per-window connection **badge**

The status-bar switcher is converted to a per-window badge showing
live/offline + server name + local/remote icon. It keeps the live-status
indicator users rely on but drops the destructive "switch the whole app"
action. To move work between servers, explicit commands exist instead:
**"Open this vault on another server…"** and (optional) **"Move this window to
another server…"** with an unsaved-state confirmation.

### 9. Persistence carries the server; migration is part of the foundation

`WindowEntry` gains a `server_id`. Legacy `windows.json` entries (no server) are
migrated to the **startup default/active** connection — **not** blindly to
local. If a persisted window references a server that can no longer be resolved,
it restores into an explicit **"unresolved window"** state (Close / Reassign /
Restore Server) rather than silently falling back.

### 10. Server edit / delete while windows are open has explicit semantics

- **Rename:** update display labels only (id is stable, so windows are
  unaffected).
- **URL / token edit:** re-navigate affected windows with confirmation, or
  require reopening — never silently swap endpoints under a live window.
- **Delete:** block while windows are open, or mark affected windows
  "server removed" with Close / Reassign / Restore actions. Never silently
  retarget a deleted-server window to local/default.

### 11. "Open Folder as Vault" is connection-aware

The native folder picker selects a path on the **local** machine, which is only
meaningful for the **local** daemon. Split the UX:

- **Local** target: "Open local folder as vault…" (native picker).
- **Remote** target: "Register a server path as vault…" with an explicit path
  input (or a remote-side browser if the daemon supports it), always showing
  **which server** will receive the vault. Remote registration uses
  `POST /api/app/vaults` (which now eager-loads the vault — see
  [ADR 0014](0014-desktop-connection-management.md) and the HTTP API docs).

## Phased rollout

- **Phase A — foundation (mostly invisible).** Window-context model + single
  authoritative registry + server-qualified labels + connection-parameterized
  URL builders + window-aware IPC + **persistence migration** (server_id on
  `WindowEntry`, legacy → default connection, unresolved-window state). New
  windows default to the demoted `default_server_id`. No multi-server menu yet;
  the global switcher is **neutralized to a no-op-on-other-windows default
  setter** so it cannot coexist destructively with later phases.
- **Phase B+C — menu + status bar (ship together).** Multi-server "New Window"
  enumeration (grouping, icons, cached/offline). Convert the status-bar switcher
  to a per-window badge and add the explicit "Open on another server" command.
  These land together so a multi-server menu never coexists with a destructive
  global switcher.
- **Phase D — connection-aware surfaces & cleanup.** Remote-aware "Add Vault",
  server edit/delete-while-open semantics, local-scoped diagnostics labels,
  Settings panel scoping (global vs server vs vault), and retiring any remaining
  global-active assumptions in favor of `default_server_id`.

## Consequences

- A user can keep local and remote vaults open side by side; each window owns
  its daemon for the rest of its life.
- "New Window" becomes the primary navigation surface across all servers;
  connection is implied by what you open, not a separate mode switch.
- The status bar stops being a control and becomes an indicator; the only
  destructive "retarget" path is an explicit, confirmed command.
- More moving parts: a window registry, per-server token plumbing, cached menu
  enumeration with offline states, and a persistence migration. Mitigated by
  the phasing and the invariant-focused tests below.
- Tauri command registration still needs the three-place wiring
  (`generate_handler!`, `build.rs commands()`, `allow-*` in **both** capability
  files under the `remote` context).

## Testing

Per the repo's TDD gates, drive tests from invariants (run from inside
`crates/notesmith-tauri`, which is workspace-excluded):

- **Foundation:** same vault name on two servers → distinct labels; same
  `(server_id, vault)` → reuse/focus the existing window; legacy `windows.json`
  migrates to the default connection; an unknown/deleted server does **not**
  silently become local.
- **IPC:** a focused local window resolves the local daemon; a focused remote
  window resolves the remote daemon (with its token); a Global/settings window
  rejects vault-scoped commands.
- **Menu:** two servers with duplicate vault names; one server offline (stale
  cache shown); auth failure; delimiter / unicode / long vault names.
- **Integration:** open `personal` from local and remote simultaneously; restart
  and restore both; delete/rename a server with a window open; a remote SSE
  disconnect/reconnect affects only that window.

## Alternatives considered

- **Idea 1 — menu shows only the active connection's vaults.** Simpler, but you
  can never have a local and a remote vault open at once (switching re-navigates
  every window). Rejected as the long-term target; it conflicts with the core
  workflow.
- **Treat "offline remote window" as a blocker requiring a native fallback or a
  local app-shell sidecar.** Rejected: remote windows already load the shell from
  the **local** embedded bundle (`FrontendMode::Embedded`), so an unreachable
  remote degrades to a recoverable in-app offline state, not a blank window.
- **Mint UUID `server_id`s for stability.** Unnecessary: the existing slug id is
  already stable across rename and URL edits (`update()` never mutates `id`).
- **Fully remove the global active connection.** Rejected: the main/onboarding/
  settings chrome, menu ordering, legacy migration, and "Add Vault" defaults all
  need a default; we keep a **demoted, non-destructive** `default_server_id`.
- **Keep the destructive status-bar switch for "retarget all windows".**
  Rejected: it breaks the per-window invariant; the niche "show this vault on
  another server" need is served by an explicit per-window command instead.

## References

- [ADR 0014 — Desktop Connection Management](0014-desktop-connection-management.md)
- [ADR 0012 — Agent Transport: ACP + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0007 — SvelteKit + Tauri](0007-sveltekit-tauri.md)
- Epic #176 (connect-to-server, complete) and its tasks #177–#182.
- `crates/notesmith-tauri/src/{main.rs,servers.rs,vault_window.rs,app_url.rs}`.
