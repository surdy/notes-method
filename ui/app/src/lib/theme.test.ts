import { afterEach, describe, expect, it, vi } from 'vitest';

type ThemeHarness = ReturnType<typeof createThemeHarness>;

const STORAGE_KEY = 'notesmith:theme';

function createThemeHarness({
	prefersDark = true,
	storedTheme = null
}: {
	prefersDark?: boolean;
	storedTheme?: unknown;
} = {}) {
	const attributes = new Map<string, string>();
	const classes = new Set<string>();
	const listeners = new Set<() => void>();
	const storage = new Map<string, string>();

	if (storedTheme !== null) {
		storage.set(
			STORAGE_KEY,
			typeof storedTheme === 'string' ? storedTheme : JSON.stringify(storedTheme)
		);
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

	const documentElement = {
		setAttribute: (name: string, value: string) => {
			attributes.set(name, value);
		},
		getAttribute: (name: string) => attributes.get(name) ?? null,
		removeAttribute: (name: string) => {
			attributes.delete(name);
		},
		style: {
			colorScheme: ''
		},
		classList: {
			add: (...tokens: string[]) => {
				for (const token of tokens) classes.add(token);
			},
			remove: (...tokens: string[]) => {
				for (const token of tokens) classes.delete(token);
			},
			contains: (token: string) => classes.has(token)
		}
	};

	vi.stubGlobal('$state', <T>(value: T) => value);
	vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
		callback(0);
		return 1;
	});
	vi.stubGlobal('window', {
		matchMedia: vi.fn().mockReturnValue(mediaQuery)
	});
	vi.stubGlobal('document', { documentElement });
	vi.stubGlobal('localStorage', {
		getItem: vi.fn((key: string) => storage.get(key) ?? null),
		setItem: vi.fn((key: string, value: string) => {
			storage.set(key, value);
		}),
		removeItem: vi.fn((key: string) => {
			storage.delete(key);
		})
	});

	return { attributes, classes, mediaQuery, storage };
}

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('themeStore', () => {
	it('sets data-theme when the active theme changes', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.setTheme('tokyo-night');

		expect(themeStore.theme).toBe('tokyo-night');
		expect(harness.attributes.get('data-theme')).toBe('tokyo-night');
	});

	it('sets data-tone when the mode changes', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.setMode('light');

		expect(themeStore.mode).toBe('light');
		expect(harness.attributes.get('data-tone')).toBe('light');
	});

	it('sets data-mode when the visual mode changes', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.setVisualMode('high-contrast');

		expect(themeStore.visualMode).toBe('high-contrast');
		expect(harness.attributes.get('data-mode')).toBe('high-contrast');
	});

	it('restores the previous attributes after preview ends', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.setTheme('notesmith-dark');
		themeStore.setMode('dark');
		const restore = themeStore.preview('tokyo-night');

		expect(harness.attributes.get('data-theme')).toBe('tokyo-night');

		restore();

		expect(harness.attributes.get('data-theme')).toBe('notesmith-dark');
		expect(harness.attributes.get('data-tone')).toBe('dark');
	});

	it('tracks system preference changes while system mode is active', async () => {
		const harness: ThemeHarness = createThemeHarness({
			prefersDark: false,
			storedTheme: {
				theme: 'notesmith-dark',
				mode: 'system',
				visualMode: 'default',
				resolvedTone: 'dark'
			}
		});
		const { themeStore } = await import('./theme.svelte.ts');

		expect(themeStore.mode).toBe('system');
		expect(harness.attributes.get('data-tone')).toBe('light');

		harness.mediaQuery.dispatch(true);

		expect(themeStore.resolvedTone).toBe('dark');
		expect(harness.attributes.get('data-tone')).toBe('dark');
	});

	it('persists theme state to localStorage', async () => {
		const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
		const { themeStore } = await import('./theme.svelte.ts');

		themeStore.setTheme('tokyo-night');
		themeStore.setMode('light');
		themeStore.setVisualMode('high-contrast');

		expect(JSON.parse(harness.storage.get(STORAGE_KEY) ?? '{}')).toMatchObject({
			theme: 'tokyo-night',
			mode: 'light',
			visualMode: 'high-contrast',
			resolvedTone: 'light'
		});
	});
});
