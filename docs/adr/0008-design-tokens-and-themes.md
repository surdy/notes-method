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

Five themes are authored in `ui/app/src/styles/theme-catalog.json` and compiled by the `theme-gen` workspace binary into `ui/app/src/styles/themes/*.css`.

The generated files expose 12-step ramp primitives (`--neutral-*`, `--red-*`, `--blue-*`, etc.) under `[data-theme="..."][data-tone="..."]` selectors, with OKLab interpolation between catalog endpoints. Split-surface themes additionally emit `[data-theme="..."] .editor-surface` so the editor can use a light-paper ramp while the outer chrome stays dark.

The runtime theme picker still controls the active theme through the existing theme store and flash-prevention script; semantic tokens consume the generated ramps while legacy `--ns-*` tokens remain as a compatibility layer during migration.

### Persistence

- Theme choice stored in `localStorage` under `notesmith:theme`
- An inline `<script>` in `app.html` reads localStorage and applies the theme class before any CSS loads, preventing flash of wrong theme
- System mode uses `matchMedia('(prefers-color-scheme: dark)')` with a change listener
- CodeMirror editor theme reads from CSS vars, so it follows the active theme automatically

## Consequences

- Adding a new color to the UI requires defining a token in `app.css` first — no ad-hoc hex values in components
- New themes are added by editing `ui/app/src/styles/theme-catalog.json` and regenerating `ui/app/src/styles/themes/*.css`
- The Manuscript theme works because split-surface generation emits a dedicated `.editor-surface` ramp block separate from the outer theme selector
- Flash prevention still requires the inline script and runtime theme store to stay in sync with the active theme attributes/classes
