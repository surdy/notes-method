import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const appHtmlPath = fileURLToPath(new URL('../app.html', import.meta.url));
const appHtml = readFileSync(appHtmlPath, 'utf8');
const bootstrapScript = appHtml.match(/<script>([\s\S]*?)<\/script>/)?.[1];

function runBootstrap(storedTheme: unknown, prefersDark = true) {
	const attributes = new Map<string, string>();
	const rawStored =
		storedTheme === null
			? null
			: typeof storedTheme === 'string'
				? storedTheme
				: JSON.stringify(storedTheme);

	vm.runInNewContext(bootstrapScript ?? '', {
		localStorage: {
			getItem: () => rawStored
		},
		window: {
			matchMedia: () => ({ matches: prefersDark })
		},
		document: {
			documentElement: {
				setAttribute: (name: string, value: string) => attributes.set(name, value),
				style: { colorScheme: '' }
			}
		}
	});

	return attributes;
}

describe('theme bootstrap', () => {
	it('uses Dark by default before Svelte starts', () => {
		const attributes = runBootstrap(null);

		expect(attributes.get('data-theme')).toBe('dark');
		expect(attributes.get('data-tone')).toBe('dark');
	});

	it('maps removed themes by former tone before first paint', () => {
		expect(runBootstrap('tokyo-night').get('data-theme')).toBe('dark');
		expect(runBootstrap('github-light').get('data-theme')).toBe('light');
		expect(runBootstrap('manuscript').get('data-theme')).toBe('split');
	});

	it('normalizes retired follow-system pairings before choosing the active theme', () => {
		const stored = {
			theme: 'tokyo-night',
			followSystem: true,
			darkTheme: 'manuscript',
			lightTheme: 'github-light',
			visualMode: 'high-contrast'
		};

		expect(runBootstrap(stored, true).get('data-theme')).toBe('split');
		const lightAttributes = runBootstrap(stored, false);
		expect(lightAttributes.get('data-theme')).toBe('light');
		expect(lightAttributes.get('data-tone')).toBe('light');
		expect(lightAttributes.get('data-mode')).toBe('high-contrast');
	});

	it('constrains legacy manual modes to the requested tone', () => {
		expect(
			runBootstrap({ theme: 'github-dark', mode: 'light', visualMode: 'default' }).get(
				'data-theme'
			)
		).toBe('light');
		expect(
			runBootstrap({ theme: 'github-light', mode: 'dark', visualMode: 'default' }).get(
				'data-theme'
			)
		).toBe('dark');
	});
});
