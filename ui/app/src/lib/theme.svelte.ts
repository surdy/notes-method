import themeCatalog from '../styles/theme-catalog.json';

export type ThemeChoice = 'dark' | 'light' | 'system' | 'manuscript' | 'hc-dark';
export type VisualMode = 'default' | 'high-contrast';

export interface ThemeEntry {
name: string;
display_name: string;
author: string;
tone: 'dark' | 'light';
split_surface: boolean;
palette: Record<string, string>;
tags: string[];
}

interface ThemeState {
theme: string;
followSystem: boolean;
darkTheme: string;
lightTheme: string;
visualMode: VisualMode;
}

type ResolvedTone = 'dark' | 'light';
type ThemeConfigInput =
| string
| {
theme?: string;
mode?: string;
followSystem?: boolean;
darkTheme?: string;
lightTheme?: string;
visualMode?: string;
  }
| null
| undefined;

const DARK_MODE_QUERY = '(prefers-color-scheme: dark)';
const STORAGE_KEY = 'notesmith:theme';
const DEFAULT_THEME = 'notesmith-dark';
const DEFAULT_DARK_THEME = 'notesmith-dark';
const DEFAULT_LIGHT_THEME = 'notesmith-light';
const DEFAULT_VISUAL_MODE: VisualMode = 'default';
const THEME_ENTRIES = themeCatalog as ThemeEntry[];
const THEME_NAMES = new Set(THEME_ENTRIES.map((entry) => entry.name));
const LEGACY_MAPPING: Record<ThemeChoice, Partial<ThemeState>> = {
dark: { theme: DEFAULT_DARK_THEME, followSystem: false, visualMode: DEFAULT_VISUAL_MODE },
light: { theme: DEFAULT_LIGHT_THEME, followSystem: false, visualMode: DEFAULT_VISUAL_MODE },
system: {
theme: DEFAULT_DARK_THEME,
followSystem: true,
darkTheme: DEFAULT_DARK_THEME,
lightTheme: DEFAULT_LIGHT_THEME,
visualMode: DEFAULT_VISUAL_MODE
},
manuscript: { theme: 'manuscript', followSystem: false, visualMode: DEFAULT_VISUAL_MODE },
'hc-dark': {
theme: DEFAULT_DARK_THEME,
followSystem: false,
visualMode: 'high-contrast'
}
};

export function isThemeChoice(value: string | null | undefined): value is ThemeChoice {
return value === 'dark' || value === 'light' || value === 'system' || value === 'manuscript' || value === 'hc-dark';
}

function isVisualMode(value: string | null | undefined): value is VisualMode {
return value === 'default' || value === 'high-contrast';
}

function getSystemTone(): ResolvedTone {
if (typeof window === 'undefined') return 'dark';
return window.matchMedia(DARK_MODE_QUERY).matches ? 'dark' : 'light';
}

function getTone(themeName: string): ResolvedTone {
const entry = THEME_ENTRIES.find((theme) => theme.name === themeName);
return entry?.tone ?? 'dark';
}

function resolveThemeName(theme: string | null | undefined): string {
if (!theme) return DEFAULT_THEME;
if (isThemeChoice(theme)) return LEGACY_MAPPING[theme].theme ?? DEFAULT_THEME;
return THEME_NAMES.has(theme) ? theme : DEFAULT_THEME;
}

function resolveThemeForTone(
theme: string | null | undefined,
tone: ResolvedTone,
fallback: string
): string {
const resolved = resolveThemeName(theme);
return getTone(resolved) === tone ? resolved : fallback;
}

function resolveLegacyObject(value: {
theme?: string;
mode?: string;
visualMode?: string;
}): Partial<ThemeState> {
const theme = resolveThemeName(value.theme);
const visualMode = isVisualMode(value.visualMode) ? value.visualMode : undefined;

if (value.mode === 'system') {
return {
theme,
followSystem: true,
darkTheme: resolveThemeForTone(theme, 'dark', DEFAULT_DARK_THEME),
lightTheme: resolveThemeForTone(theme, 'light', DEFAULT_LIGHT_THEME),
visualMode
};
}

if (value.mode === 'light') {
return {
theme: resolveThemeForTone(theme, 'light', DEFAULT_LIGHT_THEME),
followSystem: false,
visualMode
};
}

return {
theme: resolveThemeForTone(theme, 'dark', DEFAULT_DARK_THEME),
followSystem: false,
visualMode
};
}

function applyToDOM(themeName: string, tone: ResolvedTone, visualMode: VisualMode): void {
if (typeof document === 'undefined') return;

const html = document.documentElement;
html.setAttribute('data-theme-switching', '');
html.setAttribute('data-theme', themeName);
html.setAttribute('data-tone', tone);
html.setAttribute('data-mode', visualMode);
html.style.colorScheme = tone;

if (typeof requestAnimationFrame !== 'function') {
html.removeAttribute('data-theme-switching');
return;
}

requestAnimationFrame(() => {
requestAnimationFrame(() => {
html.removeAttribute('data-theme-switching');
});
});
}

function persistToStorage(state: ThemeState): void {
if (typeof localStorage === 'undefined') return;
try {
localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
} catch (_error) {}
}

function coerceStoredState(value: unknown): Partial<ThemeState> | null {
if (typeof value === 'string') {
if (isThemeChoice(value)) {
return LEGACY_MAPPING[value];
}

return { theme: resolveThemeName(value) };
}

if (!value || typeof value !== 'object') return null;

const candidate = value as {
theme?: string;
mode?: string;
followSystem?: boolean;
darkTheme?: string;
lightTheme?: string;
visualMode?: string;
};

if (candidate.mode !== undefined && candidate.followSystem === undefined) {
return resolveLegacyObject(candidate);
}

return {
theme: typeof candidate.theme === 'string' ? resolveThemeName(candidate.theme) : undefined,
followSystem:
typeof candidate.followSystem === 'boolean' ? candidate.followSystem : undefined,
darkTheme:
typeof candidate.darkTheme === 'string'
? resolveThemeForTone(candidate.darkTheme, 'dark', DEFAULT_DARK_THEME)
: undefined,
lightTheme:
typeof candidate.lightTheme === 'string'
? resolveThemeForTone(candidate.lightTheme, 'light', DEFAULT_LIGHT_THEME)
: undefined,
visualMode: isVisualMode(candidate.visualMode) ? candidate.visualMode : undefined
};
}

function readFromStorage(): Partial<ThemeState> | null {
if (typeof localStorage === 'undefined') return null;

try {
const raw = localStorage.getItem(STORAGE_KEY);
if (!raw) return null;
return coerceStoredState(JSON.parse(raw));
} catch (_error) {
try {
const raw = localStorage.getItem(STORAGE_KEY);
return raw ? coerceStoredState(raw) : null;
} catch (_nestedError) {
return null;
}
}
}

class ThemeStore {
theme = $state<string>(DEFAULT_THEME);
followSystem = $state<boolean>(false);
darkTheme = $state<string>(DEFAULT_DARK_THEME);
lightTheme = $state<string>(DEFAULT_LIGHT_THEME);
visualMode = $state<VisualMode>(DEFAULT_VISUAL_MODE);

constructor() {
if (typeof window === 'undefined') return;

const stored = readFromStorage();
if (stored) {
this.theme = stored.theme ?? this.theme;
this.followSystem = stored.followSystem ?? this.followSystem;
this.darkTheme = stored.darkTheme ?? this.darkTheme;
this.lightTheme = stored.lightTheme ?? this.lightTheme;
this.visualMode = stored.visualMode ?? this.visualMode;
}

this.commit();

const mediaQuery = window.matchMedia(DARK_MODE_QUERY);
mediaQuery.addEventListener('change', () => {
if (!this.followSystem) return;
this.commit();
});
}

get activeTheme(): string {
if (!this.followSystem) return this.theme;
return getSystemTone() === 'dark' ? this.darkTheme : this.lightTheme;
}

get resolvedTone(): ResolvedTone {
return getTone(this.activeTheme);
}

get current(): string {
if (this.visualMode === 'high-contrast') return 'hc-dark';
if (this.followSystem) return 'system';
if (this.activeTheme === 'manuscript') return 'manuscript';
return this.activeTheme;
}

private getState(): ThemeState {
return {
theme: this.theme,
followSystem: this.followSystem,
darkTheme: this.darkTheme,
lightTheme: this.lightTheme,
visualMode: this.visualMode
};
}

private commit(): void {
applyToDOM(this.activeTheme, this.resolvedTone, this.visualMode);
persistToStorage(this.getState());
}

applyFromConfig(config: ThemeConfigInput): void {
if (!config) return;

if (typeof config === 'string') {
this.set(config);
return;
}

if (
typeof config.theme === 'string' &&
isThemeChoice(config.theme) &&
config.mode === undefined &&
config.followSystem === undefined &&
config.darkTheme === undefined &&
config.lightTheme === undefined &&
config.visualMode === undefined
) {
this.set(config.theme);
return;
}

const coerced = coerceStoredState(config);
if (!coerced) return;

this.theme = coerced.theme ?? this.theme;
this.followSystem = coerced.followSystem ?? this.followSystem;
this.darkTheme = coerced.darkTheme ?? this.darkTheme;
this.lightTheme = coerced.lightTheme ?? this.lightTheme;
this.visualMode = coerced.visualMode ?? this.visualMode;
this.commit();
}

setTheme(themeName: string): void {
this.theme = resolveThemeName(themeName);
this.commit();
}

setFollowSystem(enabled: boolean): void {
this.followSystem = enabled;
this.commit();
}

setDarkTheme(themeName: string): void {
this.darkTheme = resolveThemeForTone(themeName, 'dark', DEFAULT_DARK_THEME);
this.commit();
}

setLightTheme(themeName: string): void {
this.lightTheme = resolveThemeForTone(themeName, 'light', DEFAULT_LIGHT_THEME);
this.commit();
}

setVisualMode(visualMode: VisualMode): void {
this.visualMode = visualMode;
this.commit();
}

preview(themeName: string): () => void {
const previousTheme = this.activeTheme;
const previousTone = this.resolvedTone;
const previewTheme = resolveThemeName(themeName);
applyToDOM(previewTheme, getTone(previewTheme), this.visualMode);

return () => {
applyToDOM(previousTheme, previousTone, this.visualMode);
};
}

getCatalog(): ThemeEntry[] {
return THEME_ENTRIES;
}

set(choice: string): void {
if (isThemeChoice(choice)) {
const mapped = LEGACY_MAPPING[choice];
this.theme = mapped.theme ?? this.theme;
this.followSystem = mapped.followSystem ?? this.followSystem;
this.darkTheme = mapped.darkTheme ?? this.darkTheme;
this.lightTheme = mapped.lightTheme ?? this.lightTheme;
this.visualMode = mapped.visualMode ?? this.visualMode;
this.commit();
return;
}

this.theme = resolveThemeName(choice);
this.commit();
}
}

export const themeStore = new ThemeStore();
