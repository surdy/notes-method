<script lang="ts">
import type { CustomItem, SidebarSection, SidebarView } from '$lib/api';
import { classifyError } from '$lib/api/error-classify';
import ErrorBanner from '$lib/components/ErrorBanner.svelte';
import { executeSql, getSidebarConfig, reindexVault, createNote, searchNotes } from '$lib/api';
import { createOrOpenFolderNote, folderNotePath } from '$lib/folder-notes';
import { inputPalette } from '$lib/input-palette.svelte';
import FileTree from './FileTree.svelte';
import RecentlyViewedSection from './RecentlyViewedSection.svelte';
import CustomFoldersSection from './CustomFoldersSection.svelte';
import CustomItemsSection from './CustomItemsSection.svelte';
import { vaultStore } from '$lib/stores.svelte';
import { tabStore } from '$lib/tab-store.svelte';
import { sidebarSearchStore } from '$lib/sidebar-search.svelte';
import { filterTree, nextTypeaheadIndex, treeNoteCount, wrapIndex } from '$lib/sidebar-tree';
import { toastStore } from '$lib/toast-store.svelte';

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
let refreshingVault = $state(false);

const trimmedFilter = $derived(fileFilter.trim());
const filteredTree = $derived(trimmedFilter ? filterTree(vaultStore.tree, trimmedFilter) : vaultStore.tree);
const filterHasMatches = $derived(treeNoteCount(filteredTree) > 0);

type ContentHit = { path: string; title: string; snippet?: string };
let contentResults = $state<ContentHit[]>([]);
let contentSearchTimer: ReturnType<typeof setTimeout> | null = null;

function stripMarkup(text: string): string {
	return text.replace(/<[^>]+>/g, '');
}

// Full-text content search behind the same box: debounced call to the
// /search endpoint, which also understands `key:value` filter tokens
// (tag:x, path:x, customer:x, any field:value).
$effect(() => {
	const query = trimmedFilter;
	const vault = vaultStore.currentVault;
	if (contentSearchTimer) clearTimeout(contentSearchTimer);
	if (query.length < 2 || !vault) {
		contentResults = [];
		return;
	}
	contentSearchTimer = setTimeout(async () => {
		try {
			const hits = (await searchNotes(vault, query)) as unknown as ContentHit[];
			if (trimmedFilter === query) contentResults = hits;
		} catch {
			contentResults = [];
		}
	}, 250);
});

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

function openNewNotePalette() {
	const vault = vaultStore.currentVault;
	if (!vault) {
		toastStore.add('Select a vault first.', 'warning');
		return;
	}

	inputPalette.open({
		steps: [
			{
				mode: 'text',
				label: 'Note title',
				placeholder: 'Enter a title...',
				required: true
			},
			{
				mode: 'text',
				label: 'Folder',
				placeholder: 'Inbox',
				defaultValue: 'Inbox'
			}
		],
		onComplete: async ([title, folder]) => {
			const trimmedTitle = title?.trim();
			if (!trimmedTitle) return;

			try {
				const created = await createNote(vault, trimmedTitle, '', folder?.trim() || undefined);
				await vaultStore.loadNotes();
				tabStore.selectNote(created.path);
			} catch (cause) {
				console.error('Failed to create note from files toolbar', cause);
				toastStore.add('Failed to create note.', 'error');
			}
		}
	});
}

function openNewFolderPalette() {
	const vault = vaultStore.currentVault;
	if (!vault) {
		toastStore.add('Select a vault first.', 'warning');
		return;
	}

	inputPalette.open({
		steps: [
			{
				mode: 'text',
				label: 'Folder path',
				placeholder: 'Projects/New folder',
				required: true
			}
		],
		onComplete: async ([folder]) => {
			const folderPath = folder?.trim().replace(/^\/+|\/+$/g, '');
			if (!folderPath) return;
			const expectedPath = folderNotePath(folderPath);
			if (!expectedPath) {
				toastStore.add('Choose a visible, non-hidden folder path.', 'warning');
				return;
			}

			try {
				await vaultStore.loadNotes();
				if (vaultStore.error) {
					throw new Error('Could not refresh the vault before creating the folder.');
				}

				let result;
				try {
					result = await createOrOpenFolderNote({
						vault,
						folderPath,
						notes: vaultStore.notes,
						createNote
					});
				} catch (cause) {
					// Another client may have created the folder note after our
					// refresh. Reload once and treat the now-existing note as
					// success instead of surfacing a misleading conflict.
					await vaultStore.loadNotes();
					const existing = vaultStore.notes.find((note) => note.path === expectedPath);
					if (!existing) throw cause;
					result = { path: existing.path, created: false };
				}

				await vaultStore.loadNotes();
				tabStore.selectNote(result.path);
				if (!result.created) {
					toastStore.add('Folder already exists; opened its folder note.', 'success');
				}
			} catch (cause) {
				console.error('Failed to create folder from files toolbar', cause);
				toastStore.add('Failed to create folder.', 'error');
			}
		}
	});
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
	if (!vault || refreshingVault) return;

	refreshingVault = true;
	try {
		await reindexVault(vault);
		await vaultStore.loadNotes();
	} catch (err) {
		console.error('Failed to refresh vault', err);
		toastStore.add('Failed to refresh vault.', 'error');
	} finally {
		refreshingVault = false;
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
<div class="file-toolbar">
<div class="file-search">
<svg class="file-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
<path d="M17 17L21 21" />
<path d="M19 11C19 6.58 15.42 3 11 3S3 6.58 3 11s3.58 8 8 8 8-3.58 8-8Z" />
</svg>
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
{/if}
</div>
<button class="file-action" type="button" title="New note" aria-label="New note" onclick={openNewNotePalette}>
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
<path d="M4 12v2.54c0 3.25 0 4.87.89 5.97.18.22.38.42.6.6C6.59 22 8.21 22 11.46 22c.7 0 1.05 0 1.38-.11.07-.03.13-.05.19-.09.31-.14.56-.39 1.06-.89l4.74-4.74c.58-.58.87-.87 1.02-1.23.15-.37.15-.78.15-1.6V10c0-3.77 0-5.66-1.17-6.83C17.77 2.11 16.13 2.01 13.03 2M13 21.5V21c0-2.83 0-4.24.88-5.12C14.76 15 16.17 15 19 15h.5" />
<path d="M12 6H4M8 2v8" />
</svg>
</button>
<button class="file-action" type="button" title="New folder" aria-label="New folder" onclick={openNewFolderPalette}>
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
<path d="M13 21h-1c-4.71 0-7.07 0-8.54-1.46C2 18.07 2 15.71 2 11V7.94c0-1.81 0-2.72.38-3.4.27-.49.67-.89 1.16-1.16C4.22 3 5.13 3 6.94 3c1.17 0 1.75 0 2.26.19 1.16.44 1.64 1.49 2.17 2.54L12 7h4.75c2.11 0 3.16 0 3.92.51.32.21.61.49.82.82.49.73.51 1.73.51 3.67" />
<path d="M18 13v8M22 17h-8" />
</svg>
</button>
<span class="file-toolbar-separator" aria-hidden="true"></span>
<button
class="file-action"
class:refreshing={refreshingVault}
type="button"
title="Refresh"
aria-label="Refresh"
aria-busy={refreshingVault}
disabled={refreshingVault}
onclick={() => void handleRefreshVault()}
>
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
<path d="M20.49 15A9 9 0 1 1 20.29 8.5" />
<path d="M15 9h3c1.41 0 2.12 0 2.56-.44C21 8.12 21 7.41 21 6V3" />
</svg>
</button>
</div>
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
{#if trimmedFilter && contentResults.length}
<div class="content-matches">
<div class="content-matches-header">Content matches</div>
{#each contentResults as hit (hit.path)}
<button class="content-match" type="button" onclick={() => tabStore.selectNote(hit.path)}>
<span class="content-match-title">{hit.title}</span>
<span class="content-match-path">{hit.path}</span>
{#if hit.snippet}
<span class="content-match-snippet">{stripMarkup(hit.snippet)}</span>
{/if}
</button>
{/each}
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

.file-toolbar {
display: flex;
align-items: center;
gap: 4px;
margin: 4px 8px 8px;
padding-bottom: 8px;
border-bottom: 1px solid var(--border-subtle);
}

.file-search {
display: flex;
flex: 1;
min-width: 0;
align-items: center;
gap: 6px;
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
width: 15px;
height: 15px;
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

.file-action {
display: inline-flex;
align-items: center;
justify-content: center;
width: 28px;
height: 28px;
flex-shrink: 0;
padding: 0;
border: none;
border-radius: var(--radius-sm);
background: transparent;
color: var(--text-secondary);
cursor: pointer;
}

.file-action:hover {
background: var(--bg-hover);
color: var(--text-default);
}

.file-action:focus-visible {
outline: 1px solid var(--accent);
outline-offset: 1px;
}

.file-action:disabled {
cursor: default;
opacity: 0.55;
}

.file-action svg {
width: 21px;
height: 21px;
}

.file-action.refreshing svg {
animation: file-refresh-spin 0.8s linear infinite;
}

.file-toolbar-separator {
width: 1px;
height: 20px;
margin: 0 2px;
background: var(--border-default);
flex-shrink: 0;
}

.file-tree-container:focus-visible {
outline: none;
}

@keyframes file-refresh-spin {
to {
transform: rotate(360deg);
}
}


.content-matches {
	border-top: 1px solid var(--border-muted);
	margin-top: 0.5rem;
	padding: 0.5rem 0.25rem 0.25rem;
}

.content-matches-header {
	color: var(--text-muted);
	font-size: 0.7rem;
	font-weight: 600;
	letter-spacing: 0.04em;
	padding: 0 0.5rem 0.25rem;
	text-transform: uppercase;
}

.content-match {
	background: none;
	border: none;
	border-radius: 4px;
	color: var(--text-default);
	cursor: pointer;
	display: flex;
	flex-direction: column;
	gap: 0.1rem;
	padding: 0.3rem 0.5rem;
	text-align: left;
	width: 100%;
}

.content-match:hover {
	background: var(--bg-hover, rgba(128, 128, 128, 0.12));
}

.content-match-title {
	font-size: 0.82rem;
}

.content-match-path {
	color: var(--text-muted);
	font-size: 0.7rem;
}

.content-match-snippet {
	color: var(--text-muted);
	display: -webkit-box;
	font-size: 0.72rem;
	-webkit-line-clamp: 2;
	line-clamp: 2;
	-webkit-box-orient: vertical;
	overflow: hidden;
}
</style>
