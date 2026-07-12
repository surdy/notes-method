import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const appCssPath = fileURLToPath(new URL('../app.css', import.meta.url));
const appCss = readFileSync(appCssPath, 'utf8');
const modeDefaultPath = fileURLToPath(new URL('../styles/mode-default.css', import.meta.url));
const modeDefaultCss = readFileSync(modeDefaultPath, 'utf8');

describe('theme CSS tokens', () => {
it('imports only the three selected generated themes', () => {
expect([...appCss.matchAll(/@import '.\/styles\/themes\/([^']+)\.css';/g)].map(([, name]) => name))
.toEqual(['dark', 'light', 'split']);
});

it('redeclares semantic tokens inside the Split editor surface', () => {
expect(modeDefaultCss).toContain('[data-theme="split"] .editor-surface {');
expect(modeDefaultCss).not.toContain('[data-theme="manuscript"]');
});
});
