# ADR 0014 — Desktop Connection Management (local-by-default + server switcher)

## Status

Accepted (2026-06). Refines the desktop transport behaviour described in
[ADR 0007](0007-sveltekit-tauri.md) and the remote-daemon mode referenced in
[ADR 0012](0012-agent-transport-acp-mcp.md). Implements GitHub epic #176
(tasks #177–#182).

## Context

The desktop app could reach a remote daemon only through the
`NOTESMITH_DESKTOP_DAEMON_URL` environment variable. That worked but was a poor
experience:

- Setting it required `launchctl setenv` or a shell-profile export — invisible to
  most users and easy to get wrong.
- It was **all-or-nothing and static**: the app was either local or remote for
  the whole process lifetime, with no way to switch without editing the
  environment and relaunching.
- There was no place to store more than one server, test reachability, or attach
  an access token.

We want the desktop app to feel like a normal client app: **local-only out of
the box**, with an obvious way to add servers and switch between them at runtime.

## Decision

### 1. A persisted server list is the single source of truth

`crates/notesmith-tauri/src/servers.rs` owns a `servers.json` file in the app
config dir (next to `windows.json`), using the same atomic temp-write + rename
and schema-version pattern. It stores a list of `ServerEntry { id, name, url,
token }` plus an `active_id`. A reserved `local` sentinel (`active_id = None`)
means **This Mac** (the local daemon). The file is resilient: a missing,
empty, corrupt, or version-mismatched file degrades to the local-only default
without panicking.

### 2. Two UI surfaces, one list

- **Settings → Connection** (`ConnectionSettings.svelte`) is the system of
  record: add / edit / remove / **test** servers. Testing reports reachability,
  latency, and vault count before the user commits.
- **Status-bar pill** (`ConnectionSwitcher.svelte`, bottom-left) is the quick
  switch between **This Mac** and any saved server.

Both read and mutate the same store through Tauri `connection_*` commands.

### 3. The active selection — not the environment — is authoritative

`effective_settings(app)` derives the daemon URL purely from the store's active
selection via `ServersFile::active_target(DEFAULT_DAEMON_URL)`:
`Local → (local_url, false)`, `Remote → (entry.url, true)`. The
`external_url` flag (remote vs local) therefore follows the in-app choice. This
fixes the central bug of the env-var model: switching to **This Mac** truly goes
local even when `NOTESMITH_DESKTOP_DAEMON_URL` is still exported.

### 4. Runtime switching, no restart

`connection_set_active(id)` persists the new `active_id`, recomputes
`effective_settings`, ensures the local daemon is running when switching **to**
local (and never spawns one when switching to remote), updates the runtime
`DaemonUrlState`, re-navigates each app window to the new `apiBase`, and emits
`notesmith://connection-changed`. `frontend_mode` is app-aware (remote →
embedded assets, local → daemon-served), so the webview loads the right frontend
for the active target.

### 5. `NOTESMITH_DESKTOP_DAEMON_URL` becomes a one-time seed

On first launch with the variable set, `servers::seed_from_env_url` adds the URL
to the list (named after its host) and marks it active, so existing setups keep
working after the upgrade. It is **idempotent**: once a server with that URL
exists, later launches are a no-op, and the user's in-app selection wins. The
variable is no longer required and no longer drives the local/remote decision at
runtime.

## Consequences

- New users get a zero-config local app; remote access is discoverable in the
  UI rather than hidden in the environment.
- Multiple servers can be saved, tested, and switched between live.
- The env-var migration path is seamless and reversible — a seeded server can be
  edited or removed like any other, and switching to local is honoured.
- `servers.json` is desktop-only state; the daemon and CLI are unaffected.
- Tauri command registration still requires the three-place wiring
  (`generate_handler!`, `build.rs commands()`, and `allow-*` grants in **both**
  capability files under the `remote` URL context).

## Alternatives considered

- **Keep env-var only.** Rejected: poor UX, static, single-server.
- **Settings-only (no status-bar switch).** Rejected: switching is a frequent,
  low-friction action that belongs in always-visible chrome.
- **Status-bar-only (no Settings).** Rejected: editing URLs/tokens and testing
  reachability need a real form; the pill stays a quick switch.
