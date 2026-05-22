import { afterEach, describe, expect, it, vi } from 'vitest';

type ThemeHarness = ReturnType<typeof createThemeHarness>;

function createThemeHarness({
	savedTheme = 'dark',
	prefersDark = true
}: {
	savedTheme?: string | null;
	prefersDark?: boolean;
} = {}) {
	const classes = new Set<string>();
	const storage = new Map<string, string>();
	const listeners = new Set<() => void>();

	if (savedTheme) {
		storage.set('notesmith:theme', savedTheme);
	}

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
	vi.stubGlobal('localStorage', {
		getItem: vi.fn((key: string) => storage.get(key) ?? null),
		setItem: vi.fn((key: string, value: string) => {
			storage.set(key, value);
		})
	});

	return { classes, mediaQuery, storage };
}

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('themeStore', () => {
	it('restores the saved theme and applies its class on init', async () => {
		const harness: ThemeHarness = createThemeHarness({ savedTheme: 'light', prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		expect(themeStore.current).toBe('light');
		expect([...harness.classes]).toEqual(['theme-light']);
	});

	it('tracks OS theme changes while system mode is active', async () => {
		const harness: ThemeHarness = createThemeHarness({ savedTheme: 'system', prefersDark: false });
		const { themeStore } = await import('./theme.svelte.ts');

		expect(themeStore.current).toBe('system');
		expect([...harness.classes]).toEqual(['theme-light']);

		harness.mediaQuery.dispatch(true);

		expect([...harness.classes]).toEqual(['theme-dark']);
	});

	it('persists explicit selections and replaces prior theme classes', async () => {
		const harness: ThemeHarness = createThemeHarness({ savedTheme: 'dark', prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.set('manuscript');

		expect(themeStore.current).toBe('manuscript');
		expect(harness.storage.get('notesmith:theme')).toBe('manuscript');
		expect([...harness.classes]).toEqual(['theme-manuscript']);
	});
});
