# ADR 0008: CSS Design Tokens and Theme System

## Status

Accepted

## Context

Notesmith's UI had colors scattered across 28+ component `<style>` blocks as CSS custom properties with inline fallbacks (e.g., `var(--bg-primary, #1e1e1e)`). The properties were never defined in a `:root` block — the fallback values were the actual colors. This made it impossible to support multiple themes and led to inconsistent color usage across components.

Additionally, `window.prompt()` and `window.alert()` silently fail in Tauri's WKWebView on macOS, so the new UI components (InputPalette, ToastStack) needed consistent design tokens from day one.

## Decision

### Design Tokens

All UI colors are defined as `--ns-*` CSS custom properties in a single `:root {}` block in `ui/app/src/app.css`. The `--ns-` prefix avoids collisions with library CSS. Token categories:

- **Surfaces**: `--ns-bg`, `--ns-sidebar-bg`, `--ns-surface`, `--ns-surface-hover`, `--ns-surface-active`
- **Borders**: `--ns-border`, `--ns-border-strong`, `--ns-border-input`
- **Text**: `--ns-text`, `--ns-text-secondary`, `--ns-text-muted`
- **Accent**: `--ns-accent`, `--ns-accent-bg`
- **Semantic**: `--ns-success`, `--ns-warning`, `--ns-danger` (with `-bg`, `-text`, `-border` variants)
- **Editor**: `--ns-editor-bg`, `--ns-editor-text` (separate from chrome tokens for Manuscript theme)

Components reference tokens without fallbacks (`var(--ns-bg)`, not `var(--ns-bg, #1e1e1e)`).

### Theme System

Five themes implemented as CSS class overrides on `<html>`:

| Theme | Class | Description |
|---|---|---|
| Dark | `.theme-dark` | Default — matches `:root` token values |
| Light | `.theme-light` | White backgrounds, dark text, blue accent |
| System | JS-driven | Applies dark or light based on `prefers-color-scheme` |
| Manuscript | `.theme-manuscript` | Dark chrome + light editor (`--ns-editor-bg`/`--ns-editor-text` overridden) |
| High Contrast | `.theme-hc-dark` | Pure black, cyan borders, vivid accent colors |

Each theme class only overrides tokens that differ from the dark defaults.

### Persistence

- Theme choice stored in `localStorage` under `notesmith:theme`
- An inline `<script>` in `app.html` reads localStorage and applies the theme class before any CSS loads, preventing flash of wrong theme
- System mode uses `matchMedia('(prefers-color-scheme: dark)')` with a change listener
- CodeMirror editor theme reads from CSS vars, so it follows the active theme automatically

## Consequences

- Adding a new color to the UI requires defining a token in `app.css` first — no ad-hoc hex values in components
- New themes only need to override the subset of tokens that change
- The Manuscript theme works because editor tokens (`--ns-editor-*`) are separated from chrome tokens (`--ns-bg`, `--ns-sidebar-bg`)
- Flash prevention requires the inline script to stay in sync with the theme class names in `app.css`
