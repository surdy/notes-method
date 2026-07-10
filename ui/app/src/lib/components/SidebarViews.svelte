<script lang="ts">
import type { CustomItem, SidebarSection, SidebarView } from '$lib/api';
import { classifyError } from '$lib/api/error-classify';
import ErrorBanner from '$lib/components/ErrorBanner.svelte';
import { executeSql, getSidebarConfig, reindexVault, createNote } from '$lib/api';
import FileTree from './FileTree.svelte';
import RecentlyViewedSection from './RecentlyViewedSection.svelte';
import CustomFoldersSection from './CustomFoldersSection.svelte';
import CustomItemsSection from './CustomItemsSection.svelte';
import { vaultStore } from '$lib/stores.svelte';
import { tabStore } from '$lib/tab-store.svelte';
import { sidebarSearchStore } from '$lib/sidebar-search.svelte';
import { filterTree, nextTypeaheadIndex, treeNoteCount, wrapIndex } from '$lib/sidebar-tree';

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

let fileFilter = $state('');
let searchInput = $state<HTMLInputElement | null>(null);
let treeContainer = $state<HTMLElement | null>(null);
let lastFocusNonce = 0;
let typeaheadBuffer = '';
let typeaheadTimer: ReturnType<typeof setTimeout> | null = null;

const trimmedFilter = $derived(fileFilter.trim());
const filteredTree = $derived(trimmedFilter ? filterTree(vaultStore.tree, trimmedFilter) : vaultStore.tree);
const filterHasMatches = $derived(treeNoteCount(filteredTree) > 0);

$effect(() => {
	const nonce = sidebarSearchStore.focusNonce;
	if (nonce === lastFocusNonce) return;
	lastFocusNonce = nonce;
	activeViewId = 'files';
	queueMicrotask(() => {
		searchInput?.focus();
		searchInput?.select();
	});
});

function rowButtons(): HTMLElement[] {
	if (!treeContainer) return [];
	return Array.from(
		treeContainer.querySelectorAll<HTMLElement>('.note-item, .folder-toggle, .folder-name-button')
	);
}

function focusRow(rows: HTMLElement[], index: number) {
	const row = rows[index];
	if (row) row.focus();
}

function folderControl(row: HTMLElement): { toggle: HTMLElement | null; open: boolean } | null {
	const isToggle = row.classList.contains('folder-toggle');
	const isName = row.classList.contains('folder-name-button');
	if (!isToggle && !isName) return null;
	const toggle = isToggle
		? row
		: row.closest('.folder')?.querySelector<HTMLElement>('.folder-disclosure-button') ?? null;
	const open = (toggle ?? row).querySelector('.disclosure')?.classList.contains('open') ?? false;
	return { toggle, open };
}

function handleTreeKeydown(event: KeyboardEvent) {
	const rows = rowButtons();
	if (rows.length === 0) return;
	const active = document.activeElement as HTMLElement | null;
	const current = active ? rows.indexOf(active) : -1;

	switch (event.key) {
		case 'ArrowDown':
			event.preventDefault();
			focusRow(rows, wrapIndex(rows.length, current < 0 ? -1 : current, 1));
			return;
		case 'ArrowUp':
			event.preventDefault();
			focusRow(rows, wrapIndex(rows.length, current < 0 ? 0 : current, -1));
			return;
		case 'ArrowRight': {
			if (current < 0) return;
			const folder = folderControl(rows[current]);
			if (folder && !folder.open) {
				event.preventDefault();
				folder.toggle?.click();
			} else if (current < rows.length - 1) {
				event.preventDefault();
				focusRow(rows, current + 1);
			}
			return;
		}
		case 'ArrowLeft': {
			if (current < 0) return;
			const folder = folderControl(rows[current]);
			if (folder && folder.open) {
				event.preventDefault();
				folder.toggle?.click();
			} else if (current > 0) {
				event.preventDefault();
				focusRow(rows, current - 1);
			}
			return;
		}
		case 'Enter':
			if (current >= 0) {
				event.preventDefault();
				rows[current].click();
			}
			return;
		case 'Escape':
			if (fileFilter) {
				event.preventDefault();
				fileFilter = '';
			}
			return;
	}

	if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
		typeaheadBuffer += event.key;
		if (typeaheadTimer) clearTimeout(typeaheadTimer);
		typeaheadTimer = setTimeout(() => (typeaheadBuffer = ''), 700);
		const labels = rows.map((row) => row.textContent?.trim() ?? '');
		const next = nextTypeaheadIndex(labels, current, typeaheadBuffer);
		if (next !== null) {
			event.preventDefault();
			focusRow(rows, next);
		}
	}
}

function handleSearchKeydown(event: KeyboardEvent) {
	if (event.key === 'ArrowDown') {
		event.preventDefault();
		focusRow(rowButtons(), 0);
	} else if (event.key === 'Escape') {
		fileFilter = '';
	} else if (event.key === 'Enter' && trimmedFilter && !filterHasMatches) {
		event.preventDefault();
		void createFilteredNote();
	}
}

async function createFilteredNote() {
	const title = trimmedFilter;
	if (!title || !vaultStore.currentVault) return;
	try {
		const created = await createNote(vaultStore.currentVault, title, `# ${title}\n`, 'Inbox');
		await vaultStore.loadNotes();
		fileFilter = '';
		tabStore.selectNote(created.path);
	} catch (error) {
		console.error('Failed to create note from sidebar search', error);
	}
}

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
<div class="file-search">
<span class="file-search-icon" aria-hidden="true">⌕</span>
<input
bind:this={searchInput}
bind:value={fileFilter}
class="file-search-input"
type="text"
placeholder="Search notes…"
aria-label="Search notes"
autocapitalize="off"
autocorrect="off"
autocomplete="off"
spellcheck="false"
onkeydown={handleSearchKeydown}
/>
{#if fileFilter}
<button class="file-search-clear" type="button" aria-label="Clear search" onclick={() => (fileFilter = '')}>×</button>
{:else}
<kbd class="file-search-kbd">⌘⇧F</kbd>
{/if}
</div>
{#if trimmedFilter && !filterHasMatches}
<div class="state-msg file-search-empty">
<div>No matches for “{trimmedFilter}”.</div>
<button class="refresh-btn" type="button" onclick={() => void createFilteredNote()}>
Create “{trimmedFilter}”
</button>
</div>
{:else}
<div
class="file-tree-container"
role="tree"
tabindex="-1"
aria-label="Notes"
bind:this={treeContainer}
onkeydown={handleTreeKeydown}
>
<FileTree node={filteredTree ?? vaultStore.tree} forceExpand={!!trimmedFilter} />
</div>
{/if}
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
border-bottom: 1px solid var(--border-default);
background: var(--bg-secondary);
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
color: var(--text-secondary);
font-size: 12px;
font-weight: 500;
cursor: pointer;
white-space: nowrap;
overflow: hidden;
}

.tab-button:hover {
background: var(--bg-hover);
}

.tab-button.active {
background: var(--accent-bg);
color: var(--text-default);
border-color: var(--accent);
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
border-top: 1px solid var(--border-default);
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
background: var(--bg-hover);
}

.section-chevron {
font-size: 12px;
color: var(--text-muted);
width: 12px;
flex-shrink: 0;
}

.section-label {
font-size: 11px;
font-weight: 700;
letter-spacing: 0.08em;
text-transform: uppercase;
color: var(--text-muted);
}

.section-body {
display: flex;
flex-direction: column;
}

.state-msg {
padding: 16px;
text-align: center;
color: var(--text-muted);
font-size: 12px;
}

.refresh-btn {
margin-top: 10px;
padding: 6px 10px;
border: 1px solid var(--border-default);
border-radius: 6px;
background: var(--bg-panel);
color: var(--text-default);
font-size: 12px;
cursor: pointer;
}

.refresh-btn:hover {
background: var(--bg-hover);
}

.file-search {
display: flex;
align-items: center;
gap: 6px;
margin: 4px 8px 8px;
padding: 4px 8px;
border: 1px solid var(--border-default);
border-radius: 8px;
background: var(--bg-panel);
transition: border-color 0.12s ease, box-shadow 0.12s ease;
}

.file-search:focus-within {
border-color: var(--accent);
box-shadow: 0 0 0 1px var(--accent);
}

.file-search-icon {
color: var(--text-muted);
font-size: 13px;
flex-shrink: 0;
}

.file-search-input {
flex: 1;
min-width: 0;
border: none;
background: none;
color: var(--text-default);
font-size: 13px;
outline: none;
}

.file-search-input::placeholder {
color: var(--text-muted);
}

.file-search-clear {
flex-shrink: 0;
border: none;
background: none;
color: var(--text-muted);
font-size: 15px;
line-height: 1;
cursor: pointer;
padding: 0 2px;
}

.file-search-clear:hover {
color: var(--text-default);
}

.file-search-kbd {
flex-shrink: 0;
padding: 1px 5px;
border: 1px solid var(--border-default);
border-radius: 4px;
color: var(--text-muted);
font-family: var(--font-mono);
font-size: 10px;
}

.file-tree-container:focus-visible {
outline: none;
}

</style>
