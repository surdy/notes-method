import { afterEach, describe, expect, it, vi } from 'vitest';

type ThemeHarness = ReturnType<typeof createThemeHarness>;

function createThemeHarness({
	prefersDark = true
}: {
	prefersDark?: boolean;
} = {}) {
	const classes = new Set<string>();
	const listeners = new Set<() => void>();

	const mediaQuery = {
		matches: prefersDark,
		addEventListener: vi.fn((_type: string, listener: () => void) => {
			listeners.add(listener);
		}),
		removeEventListener: vi.fn((_type: string, listener: () => void) => {
			listeners.delete(listener);
		}),
		dispatch(matches: boolean) {
			this.matches = matches;
			for (const listener of listeners) {
				listener();
			}
		}
	};

	vi.stubGlobal('$state', <T>(value: T) => value);
	vi.stubGlobal('window', {
		matchMedia: vi.fn().mockReturnValue(mediaQuery)
	});
	vi.stubGlobal('document', {
		documentElement: {
			classList: {
				add: (...tokens: string[]) => {
					for (const token of tokens) classes.add(token);
				},
				remove: (...tokens: string[]) => {
					for (const token of tokens) classes.delete(token);
				},
				contains: (token: string) => classes.has(token)
			}
		}
	});

	return { classes, mediaQuery };
}

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('themeStore', () => {
	it('defaults to system theme on init', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		expect(themeStore.current).toBe('system');
		expect([...harness.classes]).toEqual(['theme-dark']);
	});

	it('applyFromConfig sets theme from vault config', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.applyFromConfig('light');

		expect(themeStore.current).toBe('light');
		expect([...harness.classes]).toEqual(['theme-light']);
	});

	it('applyFromConfig falls back to system for invalid values', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: false });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.applyFromConfig('invalid-theme');

		expect(themeStore.current).toBe('system');
		expect([...harness.classes]).toEqual(['theme-light']);
	});

	it('tracks OS theme changes while system mode is active', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: false });
		const { themeStore } = await import('./theme.svelte.ts');

		expect(themeStore.current).toBe('system');
		expect([...harness.classes]).toEqual(['theme-light']);

		harness.mediaQuery.dispatch(true);

		expect([...harness.classes]).toEqual(['theme-dark']);
	});

	it('set replaces prior theme classes', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.set('manuscript');

		expect(themeStore.current).toBe('manuscript');
		expect([...harness.classes]).toEqual(['theme-manuscript']);
	});
});
