<script lang="ts">
import { onDestroy } from 'svelte';

import type { VaultConfigData } from '$lib/api/config';
import { themeStore, type ThemeEntry, type VisualMode } from '$lib/theme.svelte';
import { findThemeByName } from '$lib/theme-picker';
import themeCatalog from '../../../styles/theme-catalog.json';

let {
cfg,
saveImmediate
}: {
cfg: VaultConfigData;
saveImmediate: (section: string) => Promise<void>;
} = $props();

const themeOptions: ThemeEntry[] = themeCatalog as ThemeEntry[];
const darkThemeOptions = themeOptions.filter((theme) => theme.tone === 'dark');
const lightThemeOptions = themeOptions.filter((theme) => theme.tone === 'light');
let previewRestore = $state<(() => void) | null>(null);

type AppearanceState = {
theme: string;
followSystem: boolean;
darkTheme: string;
lightTheme: string;
visualMode: VisualMode;
};

function currentAppearance(): AppearanceState {
return {
theme: themeStore.theme,
followSystem: themeStore.followSystem,
darkTheme: themeStore.darkTheme,
lightTheme: themeStore.lightTheme,
visualMode: themeStore.visualMode
};
}

function updateAppearance(partial: Partial<AppearanceState>) {
cfg.appearance = {
...currentAppearance(),
...partial
};
}

function themeLabel(themeName: string): string {
return findThemeByName(themeOptions, themeName)?.display_name ?? themeName;
}

function activeThemeEntry(): ThemeEntry | undefined {
return findThemeByName(themeOptions, themeStore.activeTheme);
}

function clearPreview() {
previewRestore?.();
previewRestore = null;
}

function previewTheme(themeName: string) {
if (previewRestore && themeStore.theme === themeName) {
clearPreview();
}
clearPreview();
previewRestore = themeStore.preview(themeName);
}

async function selectTheme(themeName: string) {
clearPreview();
updateAppearance({ theme: themeName, followSystem: false });
themeStore.setTheme(themeName);
themeStore.setFollowSystem(false);
await saveImmediate('appearance');
}

async function setFollowSystem(enabled: boolean) {
updateAppearance({ followSystem: enabled });
themeStore.setFollowSystem(enabled);
await saveImmediate('appearance');
}

async function selectDarkTheme(themeName: string) {
updateAppearance({ darkTheme: themeName, followSystem: true });
themeStore.setDarkTheme(themeName);
themeStore.setFollowSystem(true);
await saveImmediate('appearance');
}

async function selectLightTheme(themeName: string) {
updateAppearance({ lightTheme: themeName, followSystem: true });
themeStore.setLightTheme(themeName);
themeStore.setFollowSystem(true);
await saveImmediate('appearance');
}

async function setVisualMode(visualMode: VisualMode) {
updateAppearance({ visualMode });
themeStore.setVisualMode(visualMode);
await saveImmediate('appearance');
}

$effect(() => {
if (cfg?.appearance) {
themeStore.applyFromConfig(cfg.appearance);
}
});

onDestroy(() => {
clearPreview();
});
</script>

<section class="section-content appearance-section">
<h2>Appearance</h2>
<p class="section-description">
Choose Dark, Light, or Split. Follow system appearance can use Dark or Split for the dark
appearance and Light for the light appearance. The high-contrast overlay remains available.
</p>

<div class="theme-section">
<div class="theme-toolbar">
<div class="theme-summary">
<span class="summary-label">Current theme</span>
<strong>{activeThemeEntry()?.display_name ?? themeStore.activeTheme}</strong>
{#if activeThemeEntry()}
<span class="field-hint">by {activeThemeEntry()?.author}</span>
{/if}
<span class="field-hint">
{themeStore.followSystem
? `Following system appearance (${themeLabel(themeStore.darkTheme)} / ${themeLabel(themeStore.lightTheme)})`
: 'Manual theme selection'}
</span>
</div>

<div class="toggle-stack">
<label class="setting-toggle">
<input
type="checkbox"
checked={themeStore.followSystem}
onchange={(event) =>
void setFollowSystem((event.currentTarget as HTMLInputElement).checked)}
/>
<span>Follow system appearance</span>
</label>

<label class="setting-toggle">
<input
type="checkbox"
checked={themeStore.visualMode === 'high-contrast'}
onchange={(event) =>
void setVisualMode(
(event.currentTarget as HTMLInputElement).checked
? 'high-contrast'
: 'default'
)}
/>
<span>High Contrast</span>
</label>
</div>
</div>

<div class="theme-group">
<div class="group-header">
<h3>Theme</h3>
<span class="field-hint">
{themeStore.followSystem
? 'Used when Follow system appearance is off.'
: 'Hover to preview, click to apply immediately.'}
</span>
</div>

<div class="theme-grid">
{#each themeOptions as theme}
<button
class="theme-card"
class:active={!themeStore.followSystem && themeStore.theme === theme.name}
type="button"
aria-pressed={!themeStore.followSystem && themeStore.theme === theme.name}
onmouseenter={() => previewTheme(theme.name)}
onmouseleave={clearPreview}
onfocus={() => previewTheme(theme.name)}
onblur={clearPreview}
onclick={() => void selectTheme(theme.name)}
>
<div class="theme-swatch" aria-hidden="true">
<span class="swatch swatch-bg" style={`background: ${theme.palette.bg}`}></span>
<span class="swatch swatch-fg" style={`background: ${theme.palette.fg}`}></span>
<span class="swatch swatch-accent" style={`background: ${theme.palette.blue}`}></span>
<span class="swatch swatch-red" style={`background: ${theme.palette.red}`}></span>
<span class="swatch swatch-green" style={`background: ${theme.palette.green}`}></span>
</div>
<span class="theme-name">{theme.display_name}</span>
<span class="theme-meta">{theme.tags.join(' · ')}</span>
</button>
{/each}
</div>
</div>

{#if themeStore.followSystem}
<div class="system-pairings">
<label class="system-select">
<span class="system-label">When dark</span>
<select
value={themeStore.darkTheme}
onchange={(event) =>
void selectDarkTheme((event.currentTarget as HTMLSelectElement).value)}
>
{#each darkThemeOptions as theme}
<option value={theme.name}>{theme.display_name}</option>
{/each}
</select>
</label>

<label class="system-select">
<span class="system-label">When light</span>
<select
value={themeStore.lightTheme}
onchange={(event) =>
void selectLightTheme((event.currentTarget as HTMLSelectElement).value)}
>
{#each lightThemeOptions as theme}
<option value={theme.name}>{theme.display_name}</option>
{/each}
</select>
</label>
</div>
{/if}
</div>
</section>

<style>
.appearance-section {
max-width: 1120px;
}

.theme-section {
display: flex;
flex-direction: column;
gap: 24px;
}

.theme-toolbar,
.system-pairings {
display: flex;
flex-wrap: wrap;
justify-content: space-between;
gap: 20px;
padding: 18px;
border: 1px solid var(--border-default);
border-radius: 16px;
background: var(--bg-surface);
}

.theme-summary,
.toggle-stack,
.theme-group {
display: flex;
flex-direction: column;
gap: 10px;
}

.group-header {
display: flex;
flex-wrap: wrap;
justify-content: space-between;
gap: 8px 16px;
align-items: baseline;
}

.group-header h3 {
margin: 0;
font-size: 14px;
font-weight: 600;
color: var(--text-default);
}

.theme-summary {
align-items: flex-start;
}

.summary-label {
display: block;
font-size: 11px;
font-weight: 600;
letter-spacing: 0.08em;
text-transform: uppercase;
color: var(--text-muted);
}

.field-hint,
.theme-meta,
.system-label {
font-size: 12px;
color: var(--text-muted);
}

.toggle-stack {
align-items: flex-start;
}

.setting-toggle,
.theme-card,
.system-select select {
color: var(--text-default);
}

.setting-toggle {
display: inline-flex;
align-items: center;
gap: 10px;
padding: 10px 12px;
border: 1px solid var(--border-default);
border-radius: 12px;
background: var(--bg-default);
font-size: 13px;
font-weight: 500;
}

.setting-toggle input {
accent-color: var(--accent);
color: var(--text-default);
}

.theme-grid {
display: grid;
grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
gap: 14px;
}

.theme-card {
display: flex;
flex-direction: column;
gap: 10px;
min-height: 124px;
padding: 14px;
border: 1px solid var(--border-default);
border-radius: 14px;
background: var(--bg-surface);
cursor: pointer;
text-align: left;
transition:
transform 120ms ease,
border-color 120ms ease,
background-color 120ms ease,
box-shadow 120ms ease;
}

.theme-card:hover,
.theme-card:focus-visible {
background: var(--bg-hover);
border-color: var(--border-strong);
box-shadow: 0 12px 28px color-mix(in srgb, var(--bg-default) 82%, transparent);
transform: translateY(-1px);
outline: none;
}

.theme-card.active {
border-color: var(--accent);
box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent);
}

.theme-swatch {
display: grid;
grid-template-columns: repeat(5, minmax(0, 1fr));
gap: 4px;
padding: 6px;
border: 1px solid var(--border-subtle);
border-radius: 10px;
background: var(--bg-default);
}

.swatch {
display: block;
height: 30px;
border-radius: 6px;
}

.theme-name {
font-size: 14px;
font-weight: 600;
color: var(--text-default);
}

.system-pairings {
align-items: flex-end;
}

.system-select {
display: flex;
flex-direction: column;
gap: 8px;
min-width: min(260px, 100%);
}

.system-select select {
padding: 10px 12px;
border: 1px solid var(--border-default);
border-radius: 12px;
background: var(--bg-default);
font-size: 13px;
font-weight: 500;
}

.system-select select:focus-visible {
outline: 1px solid var(--accent);
outline-offset: 1px;
border-color: var(--accent);
}

@media (max-width: 720px) {
.theme-toolbar,
.system-pairings {
padding: 16px;
}

.theme-grid {
grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
}

.system-pairings {
align-items: stretch;
}

.system-select {
min-width: 100%;
}
}
</style>
