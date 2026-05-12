# ADR-0005: Capabilities Endpoint Over Client-Side Detection

**Status**: Accepted  
**Date**: 2026-05 (issue #43)

## Context

The frontend needs to know whether it's running in desktop mode (Tauri, bundled daemon) or hosted mode (browser-only, remote daemon). Different features are available in each mode: desktop can edit global config, open local file paths, and manage the vault registry; hosted cannot.

We considered: `window.__TAURI__` detection and server-driven capabilities.

## Decision

Use a **server-driven capabilities endpoint**:

```
GET /api/capabilities → {
    "deployment_mode": "desktop" | "hosted",
    "can_edit_global_config": true/false,
    "can_edit_vault_config": true/false,
    "can_open_local_paths": true/false,
    "restart_required_fields": ["daemon.bind"]
}
```

Frontend uses this to conditionally show/hide UI elements. No client-side environment sniffing.

## Consequences

- Single source of truth for feature availability
- Server can customize capabilities per deployment without frontend changes
- Future: hosted deployments can configure capabilities via server startup flags
- Slight overhead of one extra API call on app init (cached)
