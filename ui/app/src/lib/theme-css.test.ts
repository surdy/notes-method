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
		[...body.matchAll(/(--ns-[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [
			name,
			value.trim()
		])
	);
}

describe('theme CSS tokens', () => {
	it('uses light-mode callout tokens inside the Manuscript editor pane', () => {
		const light = cssVariablesFor('.theme-light');
		const manuscriptEditor = cssVariablesFor('.theme-manuscript .content-area');
		const calloutTokens = [
			'--ns-accent-surface',
			'--ns-accent-surface-text',
			'--ns-info',
			'--ns-info-bg',
			'--ns-warning-callout-bg',
			'--ns-warning-callout-border',
			'--ns-success',
			'--ns-success-bg'
		];

		expect(Object.fromEntries(calloutTokens.map((token) => [token, manuscriptEditor[token]]))).toEqual(
			Object.fromEntries(calloutTokens.map((token) => [token, light[token]]))
		);
	});
});
