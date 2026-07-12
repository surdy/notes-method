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

return { attributes, mediaQuery, storage };
}

afterEach(() => {
vi.unstubAllGlobals();
vi.resetModules();
});

describe('themeStore', () => {
it('sets data-theme and derives data-tone from the catalog', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setTheme('light');

expect(themeStore.theme).toBe('light');
expect(themeStore.activeTheme).toBe('light');
expect(harness.attributes.get('data-theme')).toBe('light');
expect(harness.attributes.get('data-tone')).toBe('light');
});

it('switches between dark and light theme pairings when following system appearance', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: false });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setDarkTheme('dark');
themeStore.setLightTheme('light');
themeStore.setFollowSystem(true);

expect(themeStore.followSystem).toBe(true);
expect(themeStore.activeTheme).toBe('light');
expect(harness.attributes.get('data-theme')).toBe('light');
expect(harness.attributes.get('data-tone')).toBe('light');

harness.mediaQuery.dispatch(true);

expect(themeStore.activeTheme).toBe('dark');
expect(harness.attributes.get('data-theme')).toBe('dark');
expect(harness.attributes.get('data-tone')).toBe('dark');
});

it('sets data-mode when the visual mode changes', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setVisualMode('high-contrast');

expect(themeStore.visualMode).toBe('high-contrast');
expect(harness.attributes.get('data-mode')).toBe('high-contrast');
});

it('restores the previous attributes after preview ends', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: false });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setLightTheme('light');
themeStore.setFollowSystem(true);
const restore = themeStore.preview('split');

expect(harness.attributes.get('data-theme')).toBe('split');
expect(harness.attributes.get('data-tone')).toBe('dark');

restore();

expect(harness.attributes.get('data-theme')).toBe('light');
expect(harness.attributes.get('data-tone')).toBe('light');
});

it('persists the new theme state shape to localStorage', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setTheme('split');
themeStore.setFollowSystem(true);
themeStore.setDarkTheme('dark');
themeStore.setLightTheme('light');
themeStore.setVisualMode('high-contrast');

expect(JSON.parse(harness.storage.get(STORAGE_KEY) ?? '{}')).toMatchObject({
theme: 'split',
followSystem: true,
darkTheme: 'dark',
lightTheme: 'light',
visualMode: 'high-contrast'
});
});

it('migrates legacy string and object values from storage', async () => {
let harness: ThemeHarness = createThemeHarness({
prefersDark: false,
storedTheme: 'system'
});
let { themeStore } = await import('./theme.svelte.ts');

expect(themeStore.followSystem).toBe(true);
expect(themeStore.darkTheme).toBe('dark');
expect(themeStore.lightTheme).toBe('light');
expect(themeStore.activeTheme).toBe('light');
expect(harness.attributes.get('data-theme')).toBe('light');
expect(harness.attributes.get('data-tone')).toBe('light');

vi.unstubAllGlobals();
vi.resetModules();

harness = createThemeHarness({
prefersDark: true,
storedTheme: {
theme: 'tokyo-night',
mode: 'system',
visualMode: 'default'
}
});
({ themeStore } = await import('./theme.svelte.ts'));

expect(themeStore.followSystem).toBe(true);
expect(themeStore.darkTheme).toBe('dark');
expect(themeStore.lightTheme).toBe('light');
expect(themeStore.activeTheme).toBe('dark');
expect(harness.attributes.get('data-theme')).toBe('dark');
expect(harness.attributes.get('data-tone')).toBe('dark');

vi.unstubAllGlobals();
vi.resetModules();

harness = createThemeHarness({
prefersDark: false,
storedTheme: {
theme: 'github-light',
mode: 'system',
visualMode: 'default'
}
});
({ themeStore } = await import('./theme.svelte.ts'));

expect(themeStore.followSystem).toBe(true);
expect(themeStore.darkTheme).toBe('dark');
expect(themeStore.lightTheme).toBe('light');
expect(themeStore.activeTheme).toBe('light');
expect(harness.attributes.get('data-theme')).toBe('light');
expect(harness.attributes.get('data-tone')).toBe('light');
});

it('maps removed themes to Dark, Light, or Split by their former tone', async () => {
let harness: ThemeHarness = createThemeHarness({
storedTheme: {
theme: 'tokyo-night',
followSystem: false,
darkTheme: 'github-dark',
lightTheme: 'github-light',
visualMode: 'default'
}
});
let { themeStore } = await import('./theme.svelte.ts');

expect(themeStore.theme).toBe('dark');
expect(themeStore.darkTheme).toBe('dark');
expect(themeStore.lightTheme).toBe('light');
expect(harness.attributes.get('data-theme')).toBe('dark');

vi.unstubAllGlobals();
vi.resetModules();

harness = createThemeHarness({ storedTheme: 'manuscript' });
({ themeStore } = await import('./theme.svelte.ts'));

expect(themeStore.theme).toBe('split');
expect(themeStore.activeTheme).toBe('split');
expect(harness.attributes.get('data-theme')).toBe('split');
expect(harness.attributes.get('data-tone')).toBe('dark');
});

it('treats a one-field theme config as a manual selection', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: false });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setDarkTheme('split');
themeStore.setVisualMode('high-contrast');
themeStore.setFollowSystem(true);
themeStore.applyFromConfig({ theme: 'split' });

expect(themeStore.theme).toBe('split');
expect(themeStore.followSystem).toBe(false);
expect(themeStore.darkTheme).toBe('dark');
expect(themeStore.lightTheme).toBe('light');
expect(themeStore.visualMode).toBe('default');
expect(themeStore.activeTheme).toBe('split');
expect(harness.attributes.get('data-theme')).toBe('split');

themeStore.applyFromConfig({ theme: 'github-light' });

expect(themeStore.theme).toBe('light');
expect(themeStore.followSystem).toBe(false);
expect(harness.attributes.get('data-theme')).toBe('light');
});
});
