# ADR 0008: CSS Design Tokens and Theme System

## Status

Accepted

## Context

Notesmith's UI had colors scattered across 28+ component `<style>` blocks as CSS custom properties with inline fallbacks (e.g., `var(--bg-primary, #1e1e1e)`). The properties were never defined in a `:root` block — the fallback values were the actual colors. This made it impossible to support multiple themes and led to inconsistent color usage across components.

Additionally, `window.prompt()` and `window.alert()` silently fail in Tauri's WKWebView on macOS, so the new UI components (InputPalette, ToastStack) needed consistent design tokens from day one.

## Decision

### Design Tokens

All component-facing UI colors now use bare semantic tokens declared in `ui/app/src/styles/tokens-semantic.css` and mapped in `ui/app/src/styles/mode-default.css`. The semantic layer sits on top of generated ramp primitives so components depend on stable roles instead of theme-specific values. Token categories include:

- **Surfaces**: `--bg-default`, `--bg-secondary`, `--bg-elevated`, `--bg-hover`, `--bg-active`
- **Borders**: `--border-default`, `--border-strong`, `--border-input`, `--border-overlay`
- **Text**: `--text-default`, `--text-secondary`, `--text-muted`, `--text-inverse`
- **Accent**: `--accent`, `--accent-bg`, `--accent-text`, `--accent-hover`
- **Semantic states**: `--color-success`, `--color-warning`, `--color-danger`, plus `--success-*`, `--warning-*`, `--danger-*` surface variants
- **Editor/callouts**: `--editor-*`, `--callout-*`, and `--syntax-*` tokens for split-surface and syntax-aware UI

Components reference semantic tokens without fallbacks (`var(--bg-default)`, not `var(--bg-default, #1e1e1e)`).

### Theme System

A curated catalog of theme palettes is authored in `ui/app/src/styles/theme-catalog.json` and compiled by the `theme-gen` workspace binary into `ui/app/src/styles/themes/*.css`.

The generated files expose 12-step ramp primitives (`--neutral-*`, `--red-*`, `--blue-*`, etc.) under `[data-theme="..."][data-tone="..."]` selectors, with OKLab interpolation between catalog endpoints. Split-surface themes additionally emit `[data-theme="..."] .editor-surface` so the editor can use a light-paper ramp while the outer chrome stays dark.

The runtime theme store and flash-prevention script control the active theme exclusively through `data-theme`, `data-tone`, and `data-mode` attributes on `<html>`. Semantic tokens consume the generated ramps directly; legacy theme classes and `--ns-*` compatibility tokens have been removed from component code.

### Persistence

- Theme state stored in `localStorage` under `notesmith:theme`
- An inline `<script>` in `app.html` reads localStorage and applies `data-theme`, `data-tone`, and `data-mode` on `<html>` before any CSS loads, preventing flash of wrong theme
- System mode uses `matchMedia('(prefers-color-scheme: dark)')` with a change listener to keep `data-tone` current
- CodeMirror editor theme reads from CSS vars, so it follows the active theme automatically

## Consequences

- Adding a new color to the UI requires extending the semantic contract in `tokens-semantic.css` / `mode-default.css` first — no ad-hoc hex values in components
- New themes are added by editing `ui/app/src/styles/theme-catalog.json` and regenerating `ui/app/src/styles/themes/*.css`
- The Manuscript theme works because split-surface generation emits a dedicated `.editor-surface` ramp block separate from the outer theme selector, while semantic overrides can tune the main content area where needed
- Flash prevention still requires the inline script and runtime theme store to stay in sync with the active `data-theme`, `data-tone`, and `data-mode` attributes
