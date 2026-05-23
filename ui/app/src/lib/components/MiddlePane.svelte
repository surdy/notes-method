<script lang="ts">
import type {
CustomItem,
FolderNoteItem,
FolderSource,
QuerySource,
SqlQueryResult
} from '$lib/api';
import { executeSql, getFolderNotes } from '$lib/api';
import { tabStore } from '$lib/tab-store.svelte';
import { vaultStore } from '$lib/stores.svelte';

let {
item,
vault,
onClose
}: {
item: CustomItem;
vault: string;
onClose: () => void;
} = $props();

type ListItem =
| { kind: 'folder'; note: FolderNoteItem }
| {
kind: 'query';
row: Record<string, unknown>;
titleCol: string;
subtitleCol?: string;
badgeCols?: string[];
  };

let listItems = $state<ListItem[]>([]);
let loading = $state(false);
let error = $state<string | null>(null);

const STORAGE_WIDTH_PREFIX = 'notesmith:middle-pane-width';
const DEFAULT_WIDTH = 300;
const MIN_WIDTH = 200;

function getStorageKey() {
return `${STORAGE_WIDTH_PREFIX}:${vault}:${item.name}`;
}

function loadWidth(): number {
try {
const stored = localStorage.getItem(getStorageKey());
if (stored) {
const parsed = parseInt(stored, 10);
if (!Number.isNaN(parsed) && parsed >= MIN_WIDTH) return parsed;
}
} catch {
// ignore
}
return DEFAULT_WIDTH;
}

let paneWidth = $state(loadWidth());

$effect(() => {
const currentItem = item;
const currentVault = vault;
paneWidth = loadWidth();
void loadData(currentVault, currentItem);
});

async function loadData(currentVault: string, currentItem: CustomItem) {
loading = true;
error = null;
listItems = [];
try {
const source = currentItem.source;
if ('folder' in source) {
const folderSource = source as FolderSource;
const notes = await getFolderNotes(currentVault, {
path: folderSource.folder,
recursive: folderSource.recursive,
sort: folderSource.sort,
sort_dir: folderSource.sort_dir
});
listItems = notes.map((note) => ({ kind: 'folder' as const, note }));
} else {
const querySource = source as QuerySource;
const result: SqlQueryResult = await executeSql(currentVault, querySource.query);
const titleCol = querySource.title_column ?? result.columns[0] ?? 'title';
const subtitleCol = querySource.subtitle_column;
const badgeCols = querySource.badge_columns;
listItems = result.rows.map((row) => ({
kind: 'query' as const,
row,
titleCol,
subtitleCol,
badgeCols
}));
}
} catch (err) {
error = err instanceof Error ? err.message : 'Failed to load';
} finally {
loading = false;
}
}

function openNote(path: string, line?: number) {
tabStore.selectNote(path);
if (line !== undefined) {
window.dispatchEvent(new CustomEvent('notesmith:scroll-to-line', { detail: { path, line } }));
}
}

function titleOf(row: Record<string, unknown>, col: string): string {
const value = row[col];
if (typeof value === 'string') return value;
if (typeof value === 'number') return String(value);
return '';
}

let dragging = false;
let dragStartX = 0;
let dragStartWidth = 0;

function onDragStart(event: MouseEvent) {
dragging = true;
dragStartX = event.clientX;
dragStartWidth = paneWidth;
event.preventDefault();
}

function onMouseMove(event: MouseEvent) {
if (!dragging) return;
const maxWidth = Math.floor(window.innerWidth * 0.5);
const delta = event.clientX - dragStartX;
paneWidth = Math.max(MIN_WIDTH, Math.min(maxWidth, dragStartWidth + delta));
}

function onMouseUp() {
if (!dragging) return;
dragging = false;
try {
localStorage.setItem(getStorageKey(), String(paneWidth));
} catch {
// ignore
}
}
</script>

<svelte:window onmousemove={onMouseMove} onmouseup={onMouseUp} />

<div class="middle-pane" style="width: {paneWidth}px">
<div class="pane-header">
<span class="pane-icon">{item.icon}</span>
<span class="pane-title">{item.name}</span>
<button class="close-btn" onclick={onClose} type="button" aria-label="Close">✕</button>
</div>

<div class="pane-body">
{#if loading}
<div class="state-msg">Loading…</div>
{:else if error}
<div class="state-msg error">{error}</div>
{:else if listItems.length === 0}
<div class="state-msg">No items</div>
{:else}
{#each listItems as listItem, index (index)}
{#if listItem.kind === 'folder'}
{@const note = listItem.note}
<button
class="list-item"
class:selected={tabStore.selectedPath === note.path}
onclick={() => openNote(note.path)}
type="button"
>
<span class="item-primary">{note.title}</span>
{#if note.snippet}
<span class="item-secondary">{note.snippet}</span>
{/if}
</button>
{:else}
{@const { row, titleCol, subtitleCol, badgeCols } = listItem}
<button
class="list-item"
class:selected={typeof row.path === 'string' && tabStore.selectedPath === row.path}
onclick={() => {
if (typeof row.path === 'string' && row.path) {
const line = typeof row.line === 'number' ? row.line : undefined;
openNote(row.path, line);
}
}}
type="button"
>
<span class="item-primary">{titleOf(row, titleCol)}</span>
{#if subtitleCol && row[subtitleCol] != null}
<span class="item-secondary">{String(row[subtitleCol])}</span>
{/if}
{#if badgeCols && badgeCols.length > 0}
<span class="item-badges">
{#each badgeCols as col (col)}
{#if row[col] != null}
<span class="badge">{String(row[col])}</span>
{/if}
{/each}
</span>
{/if}
</button>
{/if}
{/each}
{/if}
</div>

<div
class="drag-handle"
role="separator"
aria-label="Resize middle pane"
onmousedown={onDragStart}
></div>
</div>

<style>
.middle-pane {
position: relative;
display: flex;
flex-direction: column;
background: var(--bg-secondary);
border-right: 1px solid var(--border-default);
overflow: hidden;
flex-shrink: 0;
}

.pane-header {
display: flex;
align-items: center;
gap: 8px;
padding: 10px 12px;
border-bottom: 1px solid var(--border-default);
flex-shrink: 0;
}

.pane-icon {
font-size: 14px;
}

.pane-title {
flex: 1;
font-size: 13px;
font-weight: 600;
color: var(--text-default);
overflow: hidden;
text-overflow: ellipsis;
white-space: nowrap;
}

.close-btn {
background: none;
border: none;
color: var(--text-muted);
cursor: pointer;
font-size: 14px;
padding: 2px 6px;
line-height: 1;
border-radius: 4px;
}

.close-btn:hover {
background: var(--bg-hover);
color: var(--text-default);
}

.pane-body {
flex: 1;
overflow-y: auto;
padding: 4px 0;
}

.list-item {
display: flex;
flex-direction: column;
align-items: flex-start;
gap: 2px;
width: 100%;
padding: 7px 12px;
border: none;
background: none;
color: var(--text-secondary);
text-align: left;
cursor: pointer;
}

.list-item:hover {
background: var(--bg-hover);
}

.list-item.selected {
background: var(--bg-selected);
color: var(--text-inverse);
}

.item-primary {
font-size: 13px;
font-weight: 500;
overflow: hidden;
text-overflow: ellipsis;
white-space: nowrap;
max-width: 100%;
}

.item-secondary {
font-size: 11px;
color: var(--text-muted);
overflow: hidden;
text-overflow: ellipsis;
white-space: nowrap;
max-width: 100%;
}

.list-item.selected .item-secondary {
color: var(--text-inverse);
}

.item-badges {
display: flex;
flex-wrap: wrap;
gap: 4px;
margin-top: 2px;
}

.badge {
display: inline-block;
padding: 1px 6px;
border-radius: 999px;
background: color-mix(in srgb, currentColor 15%, transparent);
font-size: 10px;
}

.state-msg {
padding: 16px 12px;
font-size: 12px;
color: var(--text-muted);
text-align: center;
}

.state-msg.error {
color: var(--color-danger);
}

.drag-handle {
position: absolute;
top: 0;
right: 0;
width: 4px;
height: 100%;
cursor: col-resize;
z-index: 10;
}

.drag-handle:hover {
background: var(--border-default);
}
</style>
