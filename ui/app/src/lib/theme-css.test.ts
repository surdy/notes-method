import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const appCssPath = fileURLToPath(new URL('../app.css', import.meta.url));
const appCss = readFileSync(appCssPath, 'utf8');

function cssVariablesFor(selector: string): Record<string, string> {
const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const match = appCss.match(new RegExp(`${escapedSelector}\\s*\\{(?<body>[^}]+)\\}`));
const body = match?.groups?.body;
if (!body) {
throw new Error(`Missing CSS rule for ${selector}`);
}

return Object.fromEntries(
[...body.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name, value.trim()])
);
}

describe('theme CSS tokens', () => {
it('uses light-mode semantic callout tokens inside the Manuscript content area', () => {
expect(cssVariablesFor('[data-theme="manuscript"] .content-area')).toEqual({
'--accent-bg': '#e8f4fd',
'--accent-text': '#005a9e',
'--color-info': '#1976d2',
'--warning-bg': '#fff6e6',
'--warning-border': '#ed6c02',
'--color-success': '#2e7d32',
'--success-bg': '#e8f5e9'
});
});
});
