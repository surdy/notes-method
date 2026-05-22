<script lang="ts">
import type { CustomItem, SidebarSection, SidebarView } from '$lib/api';
import { classifyError } from '$lib/api/error-classify';
import ErrorBanner from '$lib/components/ErrorBanner.svelte';
import OnboardingCard from '$lib/components/OnboardingCard.svelte';
import { executeSql, getSidebarConfig, reindexVault } from '$lib/api';
import FileTree from './FileTree.svelte';
import RecentlyViewedSection from './RecentlyViewedSection.svelte';
import CustomFoldersSection from './CustomFoldersSection.svelte';
import CustomItemsSection from './CustomItemsSection.svelte';
import { vaultStore } from '$lib/stores.svelte';

let {
onActivateMiddlePane = (_item: CustomItem) => {},
onDeactivateMiddlePane = () => {}
}: {
onActivateMiddlePane?: (item: CustomItem) => void;
onDeactivateMiddlePane?: () => void;
} = $props();

let views = $state<SidebarView[]>([]);
let activeViewId = $state('files');
let badges = $state<Record<string, number>>({});
let collapsedSections = $state<Record<string, boolean>>({});
let activeItemNames = $state<Record<string, string | null>>({});
let filesError = $derived(vaultStore.error ? classifyError(vaultStore.error, 'list-notes') : null);

$effect(() => {
const vault = vaultStore.currentVault;
if (!vault) return;

activeViewId = 'files';
views = [];
badges = {};
collapsedSections = {};
activeItemNames = {};
onDeactivateMiddlePane();
void loadConfig(vault);
});

async function loadConfig(vault: string) {
try {
const config = await getSidebarConfig(vault);
if (vault !== vaultStore.currentVault) return;
views = config.views;
for (const view of config.views) {
if (view.badge_query) {
void loadBadge(vault, view);
}
}
} catch (err) {
console.error('Failed to load sidebar config', err);
}
}

async function loadBadge(vault: string, view: SidebarView) {
if (!view.badge_query) return;
try {
const result = await executeSql(vault, view.badge_query);
if (vault !== vaultStore.currentVault) return;
const firstColumn = result.columns[0];
const firstValue = firstColumn ? result.rows[0]?.[firstColumn] : undefined;
badges[view.id] = parseBadgeValue(firstValue);
} catch {
if (vault !== vaultStore.currentVault) return;
badges[view.id] = 0;
}
}

function parseBadgeValue(value: unknown): number {
if (typeof value === 'number' && Number.isFinite(value)) return value;
if (typeof value === 'string') {
const parsed = Number(value);
return Number.isFinite(parsed) ? parsed : 0;
}
return 0;
}

function selectView(viewId: string) {
if (activeViewId !== viewId) {
onDeactivateMiddlePane();
}
activeViewId = viewId;
}

function collapseKey(viewId: string, sectionIndex: number): string {
return `${viewId}:${sectionIndex}`;
}

function isSectionCollapsed(vault: string, viewId: string, sectionIndex: number): boolean {
const lsKey = `notesmith:section-collapsed:${vault}:${viewId}:${sectionIndex}`;
const key = collapseKey(viewId, sectionIndex);
if (key in collapsedSections) return collapsedSections[key];
try {
return localStorage.getItem(lsKey) === 'true';
} catch {
return false;
}
}

function toggleSection(vault: string, viewId: string, sectionIndex: number) {
const key = collapseKey(viewId, sectionIndex);
const current = isSectionCollapsed(vault, viewId, sectionIndex);
collapsedSections[key] = !current;
try {
const lsKey = `notesmith:section-collapsed:${vault}:${viewId}:${sectionIndex}`;
localStorage.setItem(lsKey, String(!current));
} catch {
// ignore
}
}

function handleActivateItem(viewId: string, sectionIndex: number, item: CustomItem) {
const key = collapseKey(viewId, sectionIndex);
if (activeItemNames[key] === item.name) {
activeItemNames[key] = null;
onDeactivateMiddlePane();
} else {
activeItemNames[key] = item.name;
onActivateMiddlePane(item);
}
}

function sectionLabel(section: SidebarSection): string {
return section.label;
}

function handleFilesErrorAction() {
	if (filesError?.action?.type === 'update') {
		window.location.reload();
		return;
	}

	void vaultStore.loadNotes();
}

async function handleRefreshVault() {
	const vault = vaultStore.currentVault;
	if (!vault) return;

	try {
		await reindexVault(vault);
		await vaultStore.loadNotes();
	} catch (err) {
		console.error('Failed to refresh vault', err);
	}
}

export function refresh() {
const vault = vaultStore.currentVault;
if (!vault) return;
for (const view of views) {
if (view.badge_query) {
void loadBadge(vault, view);
}
}
}

export function reloadConfig() {
const vault = vaultStore.currentVault;
if (!vault) return;

const previousActiveViewId = activeViewId;
void loadConfig(vault).then(() => {
// Preserve the active tab if it still exists after reload.
const viewIds = ['files', ...views.map((v) => v.id)];
if (viewIds.includes(previousActiveViewId)) {
activeViewId = previousActiveViewId;
} else {
activeViewId = 'files';
onDeactivateMiddlePane();
}
});
}
</script>

<div class="sidebar-views">
{#if views.length > 0}
<div class="tab-bar">
<button
class="tab-button"
class:active={activeViewId === 'files'}
onclick={() => selectView('files')}
type="button"
>
<span class="tab-icon">📁</span>
<span class="tab-label">Files</span>
</button>

{#each views as view (view.id)}
<button
class="tab-button"
class:active={activeViewId === view.id}
onclick={() => selectView(view.id)}
type="button"
>
<span class="tab-icon">{view.icon}</span>
<span class="tab-label">{view.name}</span>
{#if view.badge_query}
<span class="tab-badge">{badges[view.id] ?? 0}</span>
{/if}
</button>
{/each}
</div>
{/if}

<div class="view-content">
<OnboardingCard />
{#if activeViewId === 'files'}
{#if vaultStore.loading && vaultStore.notes.length === 0}
<div class="state-msg">Loading…</div>
{:else if vaultStore.error}
<ErrorBanner
error={filesError}
onAction={handleFilesErrorAction}
onDismiss={() => vaultStore.clearError()}
/>
{:else if vaultStore.notes.length === 0}
<div class="state-msg">
<div>No notes found.</div>
<button class="refresh-btn" type="button" onclick={() => void handleRefreshVault()}>
Refresh Vault
</button>
</div>
{:else}
<FileTree node={vaultStore.tree} />
{/if}
{:else}
{@const activeView = views.find((view) => view.id === activeViewId)}
{#if activeView}
{#each activeView.sections as section, sectionIndex (sectionIndex)}
{#if sectionIndex > 0}
<hr class="section-separator" />
{/if}

{@const collapsed = isSectionCollapsed(vaultStore.currentVault, activeView.id, sectionIndex)}

<div class="section">
<button
class="section-header"
onclick={() => toggleSection(vaultStore.currentVault, activeView.id, sectionIndex)}
type="button"
aria-expanded={!collapsed}
>
<span class="section-chevron">{collapsed ? '›' : '⌄'}</span>
<span class="section-label">{sectionLabel(section)}</span>
</button>

{#if !collapsed}
<div class="section-body">
{#if section.type === 'recently-viewed'}
<RecentlyViewedSection mode={section.mode} limit={section.limit} />
{:else if section.type === 'custom-folders'}
<CustomFoldersSection folders={section.folders} />
{:else if section.type === 'custom-items'}
{@const itemKey = collapseKey(activeView.id, sectionIndex)}
<CustomItemsSection
items={section.items}
activeItemName={activeItemNames[itemKey] ?? null}
onActivateItem={(it) => handleActivateItem(activeView.id, sectionIndex, it)}
/>
{/if}
</div>
{/if}
</div>
{/each}
{/if}
{/if}
</div>
</div>

<style>
.sidebar-views {
display: flex;
flex: 1;
flex-direction: column;
min-height: 0;
}

.tab-bar {
display: grid;
grid-template-columns: 1fr 1fr;
gap: 1px;
padding: 6px;
border-bottom: 1px solid var(--ns-border);
background: var(--ns-sidebar-bg);
}

.tab-button {
display: flex;
align-items: center;
justify-content: center;
gap: 5px;
padding: 6px 8px;
border: 1px solid transparent;
border-radius: 6px;
background: transparent;
color: var(--ns-text-secondary);
font-size: 12px;
font-weight: 500;
cursor: pointer;
white-space: nowrap;
overflow: hidden;
}

.tab-button:hover {
background: var(--ns-surface-hover);
}

.tab-button.active {
background: var(--ns-selected-bg);
color: var(--ns-text-inverse);
border-color: var(--ns-selected-border);
}

.tab-icon {
font-size: 13px;
flex-shrink: 0;
}

.tab-label {
overflow: hidden;
text-overflow: ellipsis;
}

.tab-badge {
min-width: 16px;
padding: 1px 5px;
border-radius: 999px;
background: color-mix(in srgb, currentColor 20%, transparent);
font-size: 10px;
text-align: center;
flex-shrink: 0;
}

.view-content {
flex: 1;
overflow-y: auto;
padding: 4px 0;
}

.section-separator {
margin: 4px 0;
border: none;
border-top: 1px solid var(--ns-border);
}

.section {
display: flex;
flex-direction: column;
}

.section-header {
display: flex;
align-items: center;
gap: 6px;
width: 100%;
padding: 6px 10px 4px;
border: none;
background: none;
cursor: pointer;
text-align: left;
}

.section-header:hover {
background: var(--ns-surface-hover);
}

.section-chevron {
font-size: 12px;
color: var(--ns-text-muted);
width: 12px;
flex-shrink: 0;
}

.section-label {
font-size: 11px;
font-weight: 700;
letter-spacing: 0.08em;
text-transform: uppercase;
color: var(--ns-text-muted);
}

.section-body {
display: flex;
flex-direction: column;
}

.state-msg {
padding: 16px;
text-align: center;
color: var(--ns-text-muted);
font-size: 12px;
}

.refresh-btn {
margin-top: 10px;
padding: 6px 10px;
border: 1px solid var(--ns-border);
border-radius: 6px;
background: var(--ns-panel-bg);
color: var(--ns-text);
font-size: 12px;
cursor: pointer;
}

.refresh-btn:hover {
background: var(--ns-panel-hover);
}


</style>
