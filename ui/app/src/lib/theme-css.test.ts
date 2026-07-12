import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const appCssPath = fileURLToPath(new URL('../app.css', import.meta.url));
const appCss = readFileSync(appCssPath, 'utf8');
const modeDefaultPath = fileURLToPath(new URL('../styles/mode-default.css', import.meta.url));
const modeDefaultCss = readFileSync(modeDefaultPath, 'utf8');
const darkCss = readFileSync(fileURLToPath(new URL('../styles/themes/dark.css', import.meta.url)), 'utf8');
const lightCss = readFileSync(fileURLToPath(new URL('../styles/themes/light.css', import.meta.url)), 'utf8');
const splitCss = readFileSync(fileURLToPath(new URL('../styles/themes/split.css', import.meta.url)), 'utf8');

describe('theme CSS tokens', () => {
it('imports only the three selected generated themes', () => {
expect([...appCss.matchAll(/@import '.\/styles\/themes\/([^']+)\.css';/g)].map(([, name]) => name))
.toEqual(['dark', 'light', 'split']);
expect(appCss.indexOf("@import './styles/mode-high-contrast.css';"))
.toBeGreaterThan(appCss.indexOf("@import './styles/themes/split.css';"));
});

it('redeclares semantic tokens inside the Split editor surface', () => {
expect(modeDefaultCss).toContain('[data-theme="split"] .editor-surface {');
expect(modeDefaultCss).not.toContain('[data-theme="manuscript"]');
});

it('uses the selected mockups exact surface colors', () => {
expect(darkCss).toContain('--bg-secondary: #15171a;');
expect(darkCss).toContain('--bg-elevated: #1b1e23;');
expect(lightCss).toContain('--bg-secondary: #f7f8fa;');
expect(lightCss).toContain('--bg-panel: #f4f5f7;');
expect(splitCss).toContain('--bg-secondary: #1d2024;');
expect(splitCss).toContain('--bg-panel: #22252a;');
expect(splitCss).toContain('[data-theme="split"] .editor-surface');
expect(splitCss).toContain('--bg-secondary: #f3f4f5;');
});
});
