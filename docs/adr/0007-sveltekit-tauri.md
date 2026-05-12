# ADR-0007: SvelteKit + Tauri for Desktop App

**Status**: Accepted  
**Date**: 2025 (initial app design)

## Context

Notesmith needs a desktop app with a web-technology UI (three-pane layout, CodeMirror editor, command palette). The UI must also work in a browser for the hosted/web deployment mode.

## Decision

Use **SvelteKit** for the frontend framework and **Tauri** for the desktop shell:

- SvelteKit compiles to static files (adapter-static) served by the Rust daemon
- Tauri wraps the UI in a native webview window for the desktop app
- The same UI works in both desktop (Tauri) and web (browser) modes
- CodeMirror 6 for the markdown editor
- Svelte 5 runes (`$state`, `$derived`, `$effect`) for reactive state management

## Consequences

- Single codebase for desktop and web UI
- Tauri is lighter than Electron (no bundled Chromium)
- The Rust daemon is the canonical backend in both modes — Tauri just provides the window chrome
- Desktop-specific features (file path opening, global config editing) gated by capabilities endpoint, not framework detection
- Frontend has no server-side rendering — all pages are client-side SPA
