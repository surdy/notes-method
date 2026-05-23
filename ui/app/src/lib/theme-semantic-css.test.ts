import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const tokensSemanticCssPath = fileURLToPath(new URL('../styles/tokens-semantic.css', import.meta.url));
const modeDefaultCssPath = fileURLToPath(new URL('../styles/mode-default.css', import.meta.url));
const modeHighContrastCssPath = fileURLToPath(
	new URL('../styles/mode-high-contrast.css', import.meta.url)
);

const tokensSemanticCss = readFileSync(tokensSemanticCssPath, 'utf8');
const modeDefaultCss = readFileSync(modeDefaultCssPath, 'utf8');
const modeHighContrastCss = readFileSync(modeHighContrastCssPath, 'utf8');

function cssVariablesFor(css: string, selector: string): Record<string, string> {
	const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	const match = css.match(new RegExp(`${escapedSelector}\\s*\\{(?<body>[^}]+)\\}`));
	const body = match?.groups?.body;
	if (!body) {
		throw new Error(`Missing CSS rule for ${selector}`);
	}

	return Object.fromEntries(
		[...body.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name, value.trim()])
	);
}

describe('semantic theme CSS', () => {
	it('declares the semantic token contract', () => {
		const root = cssVariablesFor(tokensSemanticCss, ':root');

		expect(root).toEqual({
			'--bg-default': 'initial',
			'--bg-secondary': 'initial',
			'--bg-surface': 'initial',
			'--bg-elevated': 'initial',
			'--bg-hover': 'initial',
			'--bg-active': 'initial',
			'--bg-selected': 'initial',
			'--bg-input': 'initial',
			'--bg-panel': 'initial',
			'--border-default': 'initial',
			'--border-strong': 'initial',
			'--border-subtle': 'initial',
			'--border-input': 'initial',
			'--text-default': 'initial',
			'--text-secondary': 'initial',
			'--text-muted': 'initial',
			'--text-faint': 'initial',
			'--text-inverse': 'initial',
			'--accent': 'initial',
			'--accent-bg': 'initial',
			'--accent-text': 'initial',
			'--accent-hover': 'initial',
			'--color-danger': 'initial',
			'--color-success': 'initial',
			'--color-warning': 'initial',
			'--color-info': 'initial',
			'--syntax-comment': 'initial',
			'--syntax-keyword': 'initial',
			'--syntax-string': 'initial',
			'--syntax-constant': 'initial',
			'--syntax-entity': 'initial',
			'--syntax-variable': 'initial',
			'--syntax-tag': 'initial',
			'--syntax-function': 'initial',
			'--button-bg': 'initial',
			'--button-text': 'initial',
			'--button-hover': 'initial',
			'--button-active': 'initial',
			'--scrollbar-thumb': 'initial',
			'--scrollbar-hover': 'initial'
		});
	});

	it('maps ramp primitives to the default semantic tokens', () => {
		const root = cssVariablesFor(modeDefaultCss, ':root');

		expect(root).toEqual({
			'--bg-default': 'var(--neutral-0)',
			'--bg-secondary': 'var(--neutral-1)',
			'--bg-surface': 'var(--neutral-1)',
			'--bg-elevated': 'var(--neutral-2)',
			'--bg-hover': 'var(--neutral-2)',
			'--bg-active': 'var(--neutral-3)',
			'--bg-selected': 'var(--blue-2)',
			'--bg-input': 'var(--neutral-3)',
			'--bg-panel': 'var(--neutral-1)',
			'--border-default': 'var(--neutral-3)',
			'--border-strong': 'var(--neutral-4)',
			'--border-subtle': 'var(--neutral-2)',
			'--border-input': 'var(--neutral-4)',
			'--text-default': 'var(--neutral-10)',
			'--text-secondary': 'var(--neutral-9)',
			'--text-muted': 'var(--neutral-6)',
			'--text-faint': 'var(--neutral-5)',
			'--accent': 'var(--blue-9)',
			'--accent-bg': 'var(--blue-3)',
			'--accent-text': 'var(--blue-10)',
			'--accent-hover': 'var(--blue-4)',
			'--color-danger': 'var(--red-9)',
			'--color-success': 'var(--green-9)',
			'--color-warning': 'var(--yellow-9)',
			'--color-info': 'var(--cyan-9)',
			'--syntax-comment': 'var(--neutral-6)',
			'--syntax-keyword': 'var(--magenta-9)',
			'--syntax-string': 'var(--green-9)',
			'--syntax-constant': 'var(--blue-9)',
			'--syntax-entity': 'var(--yellow-9)',
			'--syntax-variable': 'var(--red-8)',
			'--syntax-tag': 'var(--red-9)',
			'--syntax-function': 'var(--blue-10)',
			'--button-bg': 'var(--neutral-3)',
			'--button-text': 'var(--neutral-10)',
			'--button-hover': 'var(--neutral-4)',
			'--button-active': 'var(--neutral-5)',
			'--scrollbar-thumb': 'var(--neutral-3)',
			'--scrollbar-hover': 'var(--neutral-4)'
		});
	});

	it('uses tone-specific inverse text overrides', () => {
		expect(cssVariablesFor(modeDefaultCss, '[data-tone="dark"]')).toEqual({
			'--text-inverse': '#ffffff'
		});
		expect(cssVariablesFor(modeDefaultCss, '[data-tone="light"]')).toEqual({
			'--text-inverse': '#1a1a1a'
		});
	});

	it('boosts key contrast pairs in high-contrast mode', () => {
		expect(cssVariablesFor(modeHighContrastCss, '[data-mode="high-contrast"][data-tone="dark"]')).toEqual({
			'--text-default': 'var(--neutral-11)',
			'--text-secondary': 'var(--neutral-10)',
			'--text-muted': 'var(--neutral-8)',
			'--text-faint': 'var(--neutral-7)',
			'--border-default': 'var(--neutral-5)',
			'--border-strong': 'var(--neutral-6)',
			'--bg-hover': 'var(--neutral-3)',
			'--bg-active': 'var(--neutral-4)',
			'--accent': 'var(--blue-10)',
			'--color-danger': 'var(--red-10)',
			'--color-success': 'var(--green-10)'
		});

		expect(cssVariablesFor(modeHighContrastCss, '[data-mode="high-contrast"][data-tone="light"]')).toEqual({
			'--text-default': 'var(--neutral-0)',
			'--text-secondary': 'var(--neutral-1)',
			'--text-muted': 'var(--neutral-3)',
			'--text-faint': 'var(--neutral-4)',
			'--border-default': 'var(--neutral-5)',
			'--border-strong': 'var(--neutral-4)',
			'--bg-hover': 'var(--neutral-8)',
			'--bg-active': 'var(--neutral-7)',
			'--accent': 'var(--blue-2)',
			'--color-danger': 'var(--red-2)',
			'--color-success': 'var(--green-2)'
		});
	});
});
