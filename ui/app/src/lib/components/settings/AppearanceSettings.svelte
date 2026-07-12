<script lang="ts">
import { onDestroy } from 'svelte';

import type { VaultConfigData } from '$lib/api/config';
import { themeStore, type ThemeEntry, type VisualMode } from '$lib/theme.svelte';
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
let previewRestore = $state<(() => void) | null>(null);

type AppearanceState = {
theme: string;
followSystem: boolean;
darkTheme: string;
lightTheme: string;
visualMode: VisualMode;
};

function updateAppearance(partial: Partial<AppearanceState>) {
cfg.appearance = {
theme: themeStore.theme,
followSystem: themeStore.followSystem,
darkTheme: themeStore.darkTheme,
lightTheme: themeStore.lightTheme,
visualMode: themeStore.visualMode,
...partial
};
}

function activeThemeEntry(): ThemeEntry | undefined {
return themeOptions.find((theme) => theme.name === themeStore.activeTheme);
}

function clearPreview() {
previewRestore?.();
previewRestore = null;
}

function previewTheme(themeName: string) {
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
<p class="section-description">
Choose one of three carefully tuned themes, then optionally adapt it to the system appearance.
</p>

<div class="theme-section">
<div class="theme-group">
<div class="group-header">
<h3>Theme</h3>
<span class="field-hint">
{themeStore.followSystem
? `Following the system · currently ${activeThemeEntry()?.display_name ?? themeStore.activeTheme}`
: 'Hover to preview. Changes apply immediately.'}
</span>
</div>

<div class="theme-grid">
{#each themeOptions as theme}
<button
class="theme-card"
class:active={themeStore.activeTheme === theme.name}
type="button"
aria-pressed={themeStore.activeTheme === theme.name}
onmouseenter={() => previewTheme(theme.name)}
onmouseleave={clearPreview}
onfocus={() => previewTheme(theme.name)}
onblur={clearPreview}
onclick={() => void selectTheme(theme.name)}
>
<div
class="workspace-preview"
aria-hidden="true"
style={`--preview-chrome: ${theme.semantic?.bg_panel ?? theme.palette.bg}; --preview-sidebar: ${theme.semantic?.bg_secondary ?? theme.palette.bg}; --preview-editor: ${theme.editor_semantic?.bg_default ?? theme.semantic?.bg_default ?? theme.palette.bg}; --preview-text: ${theme.editor_palette?.fg ?? theme.palette.fg}; --preview-border: ${theme.semantic?.border_default ?? theme.palette.black}; --preview-accent: ${theme.palette.blue}`}
>
<span class="preview-sidebar">
<span></span>
<span></span>
<span></span>
</span>
<span class="preview-editor">
<span></span>
<span></span>
<span></span>
</span>
<span class="preview-rail">
<span></span>
<span></span>
</span>
</div>
<span class="theme-copy">
<span class="theme-name">{theme.display_name}</span>
<span class="theme-description">{theme.description}</span>
</span>
</button>
{/each}
</div>
</div>

<div class="preference-list">
<label class="preference-row">
<span class="preference-copy">
<strong>Follow system appearance</strong>
<span>Use Light during the day and your preferred dark appearance at night.</span>
</span>
<input
class="toggle-input"
type="checkbox"
role="switch"
checked={themeStore.followSystem}
onchange={(event) =>
void setFollowSystem((event.currentTarget as HTMLInputElement).checked)}
/>
<span class="toggle-track" aria-hidden="true"><span></span></span>
</label>

{#if themeStore.followSystem}
<div class="system-pairings">
<div class="pairing-row">
<span class="pairing-copy">
<strong>Dark appearance</strong>
<span>Choose the theme used when the system is dark.</span>
</span>
<div class="segmented-control" role="group" aria-label="Dark appearance">
{#each darkThemeOptions as theme}
<button
type="button"
class:active={themeStore.darkTheme === theme.name}
aria-pressed={themeStore.darkTheme === theme.name}
onclick={() => void selectDarkTheme(theme.name)}
>{theme.display_name}</button
>
{/each}
</div>
</div>

<div class="pairing-row">
<span class="pairing-copy">
<strong>Light appearance</strong>
<span>Light is the single carefully tuned light theme.</span>
</span>
<span class="fixed-choice">Light</span>
</div>
</div>
{/if}

<label class="preference-row">
<span class="preference-copy">
<strong>High contrast</strong>
<span>Strengthen text, borders, and selection states in any theme.</span>
</span>
<input
class="toggle-input"
type="checkbox"
role="switch"
checked={themeStore.visualMode === 'high-contrast'}
onchange={(event) =>
void setVisualMode(
(event.currentTarget as HTMLInputElement).checked ? 'high-contrast' : 'default'
)}
/>
<span class="toggle-track" aria-hidden="true"><span></span></span>
</label>
</div>
</div>
</section>

<style>
.appearance-section {
max-width: 960px;
}

.theme-section,
.theme-group {
display: flex;
flex-direction: column;
}

.theme-section {
gap: 28px;
}

.theme-group {
gap: 14px;
}

.group-header {
display: flex;
flex-wrap: wrap;
align-items: baseline;
justify-content: space-between;
gap: 4px 16px;
}

.group-header h3 {
margin: 0;
font-size: 14px;
font-weight: 600;
color: var(--text-default);
}

.field-hint,
.theme-description,
.preference-copy span,
.pairing-copy span {
font-size: 12px;
line-height: 1.45;
color: var(--text-muted);
}

.theme-grid {
display: grid;
grid-template-columns: repeat(3, minmax(0, 1fr));
gap: 12px;
}

.theme-card {
display: flex;
min-width: 0;
flex-direction: column;
gap: 12px;
padding: 12px;
border: 1px solid var(--border-default);
border-radius: 12px;
background: var(--bg-surface);
color: var(--text-default);
cursor: pointer;
text-align: left;
transition:
border-color 120ms ease,
background-color 120ms ease,
box-shadow 120ms ease;
}

.theme-card:hover,
.theme-card:focus-visible {
border-color: var(--border-strong);
outline: none;
background: var(--bg-hover);
box-shadow: 0 8px 22px color-mix(in srgb, var(--bg-default) 86%, transparent);
}

.theme-card.active {
border-color: var(--accent);
box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent);
}

.workspace-preview {
display: grid;
height: 88px;
grid-template-columns: 24% 1fr 22%;
overflow: hidden;
border: 1px solid var(--preview-border);
border-radius: 8px;
background: var(--preview-chrome);
}

.preview-sidebar,
.preview-rail,
.preview-editor {
display: flex;
flex-direction: column;
gap: 7px;
padding: 12px 8px;
}

.preview-sidebar,
.preview-rail {
background: var(--preview-sidebar);
}

.preview-sidebar {
border-right: 1px solid var(--preview-border);
}

.preview-rail {
border-left: 1px solid var(--preview-border);
}

.preview-editor {
padding: 14px 12px;
background: var(--preview-editor);
}

.preview-sidebar span,
.preview-rail span,
.preview-editor span {
display: block;
height: 3px;
border-radius: 999px;
background: color-mix(in srgb, var(--preview-text) 24%, transparent);
}

.preview-sidebar span:first-child {
width: 72%;
background: var(--preview-accent);
}

.preview-sidebar span:last-child,
.preview-rail span:last-child {
width: 64%;
}

.preview-editor span:first-child {
width: 52%;
height: 5px;
background: color-mix(in srgb, var(--preview-text) 72%, transparent);
}

.preview-editor span:nth-child(2) {
width: 88%;
}

.preview-editor span:last-child {
width: 74%;
}

.theme-copy,
.preference-copy,
.pairing-copy {
display: flex;
flex-direction: column;
gap: 3px;
}

.theme-name {
font-size: 14px;
font-weight: 600;
color: var(--text-default);
}

.preference-list {
overflow: hidden;
border: 1px solid var(--border-default);
border-radius: 12px;
background: var(--bg-surface);
}

.preference-row,
.pairing-row {
position: relative;
display: flex;
min-height: 66px;
align-items: center;
justify-content: space-between;
gap: 24px;
padding: 14px 16px;
color: var(--text-default);
}

.preference-row + .preference-row,
.system-pairings + .preference-row {
border-top: 1px solid var(--border-subtle);
}

.preference-copy strong,
.pairing-copy strong {
font-size: 13px;
font-weight: 600;
color: var(--text-default);
}

.toggle-input {
position: absolute;
width: 1px;
height: 1px;
opacity: 0;
color: var(--text-default);
}

.toggle-track {
display: flex;
width: 36px;
height: 20px;
flex: 0 0 auto;
align-items: center;
padding: 2px;
border: 1px solid var(--border-strong);
border-radius: 999px;
background: var(--bg-elevated);
transition:
background-color 120ms ease,
border-color 120ms ease;
}

.toggle-track span {
width: 14px;
height: 14px;
border-radius: 50%;
background: var(--text-muted);
transition:
transform 120ms ease,
background-color 120ms ease;
}

.toggle-input:checked + .toggle-track {
border-color: var(--accent);
background: var(--accent);
}

.toggle-input:checked + .toggle-track span {
transform: translateX(16px);
background: var(--accent-text);
}

.toggle-input:focus-visible + .toggle-track {
outline: 2px solid var(--accent);
outline-offset: 2px;
}

.system-pairings {
border-top: 1px solid var(--border-subtle);
border-bottom: 1px solid var(--border-subtle);
background: var(--bg-secondary);
}

.pairing-row + .pairing-row {
border-top: 1px solid var(--border-subtle);
}

.segmented-control {
display: inline-flex;
padding: 2px;
border: 1px solid var(--border-default);
border-radius: 8px;
background: var(--bg-default);
}

.segmented-control button {
padding: 6px 12px;
border: 0;
border-radius: 6px;
background: transparent;
color: var(--text-muted);
font-size: 12px;
font-weight: 600;
cursor: pointer;
}

.segmented-control button:hover,
.segmented-control button:focus-visible {
outline: none;
color: var(--text-default);
}

.segmented-control button:focus-visible {
box-shadow: 0 0 0 1px var(--accent);
}

.segmented-control button.active {
background: var(--bg-active);
color: var(--text-default);
}

.fixed-choice {
padding: 6px 12px;
border: 1px solid var(--border-default);
border-radius: 8px;
background: var(--bg-default);
color: var(--text-default);
font-size: 12px;
font-weight: 600;
}

@media (max-width: 720px) {
.theme-grid {
grid-template-columns: 1fr;
}

.workspace-preview {
height: 74px;
}

.preference-row,
.pairing-row {
gap: 16px;
}
}
</style>
