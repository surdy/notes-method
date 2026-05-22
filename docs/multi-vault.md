# Multi-Vault Window Management

Notesmith supports multiple vaults, each opening in its own native window.

---

## Window-Per-Vault

Each vault gets a dedicated native window. Opening a vault that already has an active window focuses the existing window instead of creating a duplicate.

---

## Registering Vaults

Vaults are registered via:
- **File menu → Open Folder as Vault** — select a folder containing markdown files
- **Name-this-vault modal** — appears when a new folder is opened, prompting for a display name

Registered vaults appear in:
- The **File** menu under a vault list
- The **system tray** menu for quick access

---

## Vault Lifecycle

| Action | Behavior |
|--------|----------|
| Open vault | Creates a new window (or focuses existing) |
| Close window | Window is destroyed; vault is deregistered from active windows |
| App restart | Previously open vaults are restored from `windows.json` |

---

## Data Safety

Notesmith uses 1-second debounced auto-save. Because all edits are saved continuously, closing a vault window requires no confirmation dialog. The native close button (red ✕) works immediately without risk of data loss.

---

## Per-Vault Isolation

Each vault window maintains isolated state:
- Separate `localStorage` namespace (tabs, view modes, sidebar state)
- Independent note tree and tab bar
- Own editor instances

---

## Persistence

Open vault windows are persisted in `windows.json` (managed by the Tauri shell). On next app launch, all previously-open vaults reopen automatically in their own windows.
