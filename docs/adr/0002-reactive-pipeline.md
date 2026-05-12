# ADR-0002: File-Watcher → SSE → ArcSwap Reactive Pipeline

**Status**: Accepted  
**Date**: 2026-05 (issues #39–#42)

## Context

Config file changes (vault.toml, sidebar.yaml) and note file changes need to propagate to the frontend without restarting the daemon or manually reloading the browser.

We considered: polling, WebSockets, and file-watcher + SSE.

## Decision

Use a three-stage reactive pipeline:

1. **File watcher** (`notify` crate) detects filesystem changes, classifies them (note vs config), and debounces rapid events.
2. **SSE events** broadcast `VaultEvent` payloads to connected frontends via `GET /api/v/{vault}/events`.
3. **ArcSwap** hot-swaps in-memory config (`VaultConfig`) when vault.toml changes, without restarting any services.

Invalid config files leave the last valid config active and emit an error event.

## Consequences

- Single-direction data flow: filesystem → daemon → frontend
- No WebSocket complexity; SSE auto-reconnects natively
- Config writes (from settings UI) go through the same path: write file → watcher detects → SSE propagates
- ArcSwap gives lock-free reads for hot config access in request handlers
