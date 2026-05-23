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

themeStore.setTheme('github-light');

expect(themeStore.theme).toBe('github-light');
expect(themeStore.activeTheme).toBe('github-light');
expect(harness.attributes.get('data-theme')).toBe('github-light');
expect(harness.attributes.get('data-tone')).toBe('light');
});

it('switches between dark and light theme pairings when following system appearance', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: false });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setDarkTheme('tokyo-night');
themeStore.setLightTheme('github-light');
themeStore.setFollowSystem(true);

expect(themeStore.followSystem).toBe(true);
expect(themeStore.activeTheme).toBe('github-light');
expect(harness.attributes.get('data-theme')).toBe('github-light');
expect(harness.attributes.get('data-tone')).toBe('light');

harness.mediaQuery.dispatch(true);

expect(themeStore.activeTheme).toBe('tokyo-night');
expect(harness.attributes.get('data-theme')).toBe('tokyo-night');
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

themeStore.setLightTheme('github-light');
themeStore.setFollowSystem(true);
const restore = themeStore.preview('tokyo-night');

expect(harness.attributes.get('data-theme')).toBe('tokyo-night');
expect(harness.attributes.get('data-tone')).toBe('dark');

restore();

expect(harness.attributes.get('data-theme')).toBe('github-light');
expect(harness.attributes.get('data-tone')).toBe('light');
});

it('persists the new theme state shape to localStorage', async () => {
const harness: ThemeHarness = createThemeHarness({ prefersDark: true });
const { themeStore } = await import('./theme.svelte.ts');

themeStore.setTheme('tokyo-night');
themeStore.setFollowSystem(true);
themeStore.setDarkTheme('tokyo-night');
themeStore.setLightTheme('github-light');
themeStore.setVisualMode('high-contrast');

expect(JSON.parse(harness.storage.get(STORAGE_KEY) ?? '{}')).toMatchObject({
theme: 'tokyo-night',
followSystem: true,
darkTheme: 'tokyo-night',
lightTheme: 'github-light',
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
expect(themeStore.darkTheme).toBe('notesmith-dark');
expect(themeStore.lightTheme).toBe('notesmith-light');
expect(themeStore.activeTheme).toBe('notesmith-light');
expect(harness.attributes.get('data-theme')).toBe('notesmith-light');
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
expect(themeStore.darkTheme).toBe('tokyo-night');
expect(themeStore.lightTheme).toBe('notesmith-light');
expect(themeStore.activeTheme).toBe('tokyo-night');
expect(harness.attributes.get('data-theme')).toBe('tokyo-night');
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
expect(themeStore.darkTheme).toBe('notesmith-dark');
expect(themeStore.lightTheme).toBe('github-light');
expect(themeStore.activeTheme).toBe('github-light');
expect(harness.attributes.get('data-theme')).toBe('github-light');
expect(harness.attributes.get('data-tone')).toBe('light');
});
});
