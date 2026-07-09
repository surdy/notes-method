# Notesmith Web Clipper (browser extension)

A minimal Manifest V3 browser extension that clips the current page into a
Notesmith vault. **All extraction happens server-side** in the Notesmith daemon
([ADR 0020](../../docs/adr/0020-web-clipper.md)) — the extension only sends the
current tab's URL to `POST /api/v/{vault}/clip`. There is no client-side
readability/DOM scraping.

## Features

- Toolbar button → popup with a **vault picker** (fetched live from
  `GET /api/app/vaults`) and a comma-separated **tags** field.
- Configurable **daemon base URL** (local `http://127.0.0.1:27183` by default, or
  a remote daemon) via the options page.
- Remembers the last vault used as the default.
- Reports duplicates (`Already clipped: <path>`) returned by the daemon.

## Install (unpacked, developer mode)

1. Start the Notesmith daemon (`notesmith daemon start`).
2. Open your browser's extensions page:
   - Chrome/Edge/Brave: `chrome://extensions`
   - Enable **Developer mode**.
3. Click **Load unpacked** and select this `ui/extension/` folder.
4. Open the extension **Settings** (options page) and confirm the daemon base
   URL. Click **Save** — you'll be prompted to grant permission to reach that
   origin.
5. Navigate to any page and click the toolbar button → **Clip this page**.

## Permissions

- `activeTab` / `tabs` — read the current tab's URL.
- `storage` — persist the base URL and default vault.
- `optional_host_permissions: *://*/*` — requested at runtime (scoped to the
  configured daemon origin) so the popup can call the daemon cross-origin. The
  daemon serves permissive CORS.

## Files

| File | Purpose |
|------|---------|
| `manifest.json` | MV3 manifest |
| `popup.html` / `popup.js` | Toolbar popup: vault picker + clip action |
| `options.html` / `options.js` | Settings: daemon base URL + default vault |
| `config.js` | Shared storage + daemon fetch helpers |

No build step — plain ES modules loaded directly by the browser.

## Notes

- v1 has **no authentication** (an explicit accepted risk, matching the daemon's
  clip endpoint). Prefer a local or trusted-network daemon.
- Icons are intentionally omitted; the browser renders a default action icon.
