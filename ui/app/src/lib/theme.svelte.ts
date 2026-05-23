import themeCatalog from '../styles/theme-catalog.json';

export type ThemeChoice = 'dark' | 'light' | 'system' | 'manuscript' | 'hc-dark';
export type ThemeMode = 'dark' | 'light' | 'system';
export type VisualMode = 'default' | 'high-contrast';

type ResolvedTone = 'dark' | 'light';

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
mode: ThemeMode;
visualMode: VisualMode;
resolvedTone: ResolvedTone;
}

const DARK_MODE_QUERY = '(prefers-color-scheme: dark)';
const STORAGE_KEY = 'notesmith:theme';
const DEFAULT_THEME = 'notesmith-dark';
const THEME_ENTRIES = themeCatalog as ThemeEntry[];
const THEME_NAMES = new Set(THEME_ENTRIES.map((entry) => entry.name));
const LEGACY_MAPPING: Record<ThemeChoice, Omit<ThemeState, 'resolvedTone'>> = {
dark: { theme: 'notesmith-dark', mode: 'dark', visualMode: 'default' },
light: { theme: 'notesmith-light', mode: 'light', visualMode: 'default' },
system: { theme: 'notesmith-dark', mode: 'system', visualMode: 'default' },
manuscript: { theme: 'manuscript', mode: 'dark', visualMode: 'default' },
'hc-dark': { theme: 'notesmith-dark', mode: 'dark', visualMode: 'high-contrast' }
};

export function isThemeChoice(value: string | null | undefined): value is ThemeChoice {
return value === 'dark' || value === 'light' || value === 'system' || value === 'manuscript' || value === 'hc-dark';
}

function isThemeMode(value: string | null | undefined): value is ThemeMode {
return value === 'dark' || value === 'light' || value === 'system';
}

function isVisualMode(value: string | null | undefined): value is VisualMode {
return value === 'default' || value === 'high-contrast';
}

function isResolvedTone(value: string | null | undefined): value is ResolvedTone {
return value === 'dark' || value === 'light';
}

function getSystemTone(): ResolvedTone {
if (typeof window === 'undefined') return 'dark';
return window.matchMedia(DARK_MODE_QUERY).matches ? 'dark' : 'light';
}

function resolveThemeName(theme: string | null | undefined): string {
if (!theme) return DEFAULT_THEME;
if (isThemeChoice(theme)) return LEGACY_MAPPING[theme].theme;
return THEME_NAMES.has(theme) ? theme : DEFAULT_THEME;
}

function resolveThemeTone(theme: string, mode: ThemeMode): ResolvedTone {
const entry = THEME_ENTRIES.find((t) => t.name === theme);
const nativeTone = entry?.tone ?? 'dark';

if (mode === 'system') {
	const systemTone = getSystemTone();
	// If theme only supports one tone, stay with its native tone
	const hasVariant = THEME_ENTRIES.some((t) => t.name === theme && t.tone === systemTone);
	return hasVariant ? systemTone : nativeTone;
}

// If user explicitly picks a mode that doesn't match the theme, stay native
const hasVariant = THEME_ENTRIES.some((t) => t.name === theme && t.tone === mode);
return hasVariant ? mode : nativeTone;
}

function applyToDOM(state: ThemeState): void {
if (typeof document === 'undefined') return;

const html = document.documentElement;
html.setAttribute('data-theme-switching', '');
html.setAttribute('data-theme', state.theme);
html.setAttribute('data-tone', state.resolvedTone);
html.setAttribute('data-mode', state.visualMode);
html.style.colorScheme = state.resolvedTone;

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

const candidate = value as Partial<ThemeState>;
return {
theme: typeof candidate.theme === 'string' ? resolveThemeName(candidate.theme) : undefined,
mode: isThemeMode(candidate.mode) ? candidate.mode : undefined,
visualMode: isVisualMode(candidate.visualMode) ? candidate.visualMode : undefined,
resolvedTone: isResolvedTone(candidate.resolvedTone) ? candidate.resolvedTone : undefined
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
mode = $state<ThemeMode>('system');
visualMode = $state<VisualMode>('default');
resolvedTone = $state<ResolvedTone>('dark');

constructor() {
if (typeof window === 'undefined') return;

const stored = readFromStorage();
if (stored) {
this.theme = stored.theme ?? this.theme;
this.mode = stored.mode ?? this.mode;
this.visualMode = stored.visualMode ?? this.visualMode;
}

this.resolvedTone = resolveThemeTone(this.theme, this.mode);
applyToDOM(this.getState());
persistToStorage(this.getState());

const mediaQuery = window.matchMedia(DARK_MODE_QUERY);
mediaQuery.addEventListener('change', () => {
if (this.mode !== 'system') return;
this.resolvedTone = resolveThemeTone(this.theme, this.mode);
applyToDOM(this.getState());
persistToStorage(this.getState());
});
}

get current(): string {
if (this.visualMode === 'high-contrast') return 'hc-dark';
if (this.theme === 'manuscript') return 'manuscript';
if (this.mode === 'system') return 'system';
return this.resolvedTone;
}

private getState(): ThemeState {
return {
theme: this.theme,
mode: this.mode,
visualMode: this.visualMode,
resolvedTone: this.resolvedTone
};
}

private commit(): void {
this.resolvedTone = resolveThemeTone(this.theme, this.mode);
applyToDOM(this.getState());
persistToStorage(this.getState());
}

applyFromConfig(config: string | { theme?: string; mode?: string; visualMode?: string } | null | undefined): void {
if (!config) return;

if (typeof config === 'string') {
this.set(config);
return;
}

if (typeof config.theme === 'string' && isThemeChoice(config.theme) && !config.mode && !config.visualMode) {
this.set(config.theme);
return;
}

this.theme = resolveThemeName(config.theme ?? this.theme);
this.mode = isThemeMode(config.mode) ? config.mode : this.mode;
this.visualMode = isVisualMode(config.visualMode) ? config.visualMode : this.visualMode;
this.commit();
}

setTheme(themeName: string): void {
this.theme = resolveThemeName(themeName);
this.commit();
}

setMode(mode: ThemeMode): void {
this.mode = mode;
this.commit();
}

setVisualMode(visualMode: VisualMode): void {
this.visualMode = visualMode;
this.commit();
}

preview(themeName: string): () => void {
const previous = this.getState();
const previewTheme = resolveThemeName(themeName);
applyToDOM({
theme: previewTheme,
mode: this.mode,
visualMode: this.visualMode,
resolvedTone: resolveThemeTone(previewTheme, this.mode)
});

return () => {
applyToDOM(previous);
};
}

getCatalog(): ThemeEntry[] {
return THEME_ENTRIES;
}

set(choice: string): void {
if (isThemeChoice(choice)) {
const mapped = LEGACY_MAPPING[choice];
this.theme = mapped.theme;
this.mode = mapped.mode;
this.visualMode = mapped.visualMode;
this.commit();
return;
}

this.theme = resolveThemeName(choice);
this.commit();
}
}

export const themeStore = new ThemeStore();
