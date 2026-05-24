<script lang="ts">
import { executeSql } from '$lib/api';
import { getRecentlyViewed } from '$lib/recently-viewed';
import { tabStore } from '$lib/tab-store.svelte';
import { vaultStore } from '$lib/stores.svelte';

let { mode, limit }: { mode: 'viewed' | 'edited' | 'both'; limit: number } = $props();

interface Item {
path: string;
title: string;
}

let items = $state<Item[]>([]);

$effect(() => {
const vault = vaultStore.currentVault;
const _path = tabStore.selectedPath;
if (!vault) return;
void loadItems(vault);
});

async function loadItems(vault: string) {
try {
if (mode === 'viewed') {
items = getRecentlyViewed(vault, limit);
} else if (mode === 'edited') {
items = await loadEdited(vault);
} else {
const [viewed, edited] = await Promise.all([
Promise.resolve(getRecentlyViewed(vault, limit)),
loadEdited(vault)
]);
items = mergeAndDeduplicate(viewed, edited, limit);
}
} catch (err) {
console.error('Failed to load recently viewed', err);
}
}

async function loadEdited(vault: string): Promise<Item[]> {
const result = await executeSql(
vault,
`SELECT path, title, updated_at FROM v_notes ORDER BY updated_at DESC LIMIT ${limit}`
);
return result.rows
.map((row) => ({
path: String(row.path ?? ''),
title: String(row.title ?? row.path ?? '')
}))
.filter((item) => item.path);
}

function mergeAndDeduplicate(viewed: Item[], edited: Item[], max: number): Item[] {
const seen = new Set<string>();
const merged: Item[] = [];
for (const item of [...viewed, ...edited]) {
if (!seen.has(item.path)) {
seen.add(item.path);
merged.push(item);
}
}
return merged.slice(0, max);
}

function open(path: string) {
tabStore.selectNote(path);
}
</script>

<div class="recently-viewed-list">
{#each items as item (item.path)}
<button
class="item"
class:selected={tabStore.selectedPath === item.path}
onclick={() => open(item.path)}
type="button"
>
<span class="item-title">{item.title}</span>
</button>
{:else}
<div class="empty">No recent notes</div>
{/each}
</div>

<style>
.recently-viewed-list {
display: flex;
flex-direction: column;
}

.item {
display: block;
width: 100%;
padding: 5px 12px;
border: none;
background: none;
color: var(--text-secondary);
font-size: 13px;
text-align: left;
cursor: pointer;
white-space: nowrap;
overflow: hidden;
text-overflow: ellipsis;
}

.item:hover {
background: var(--bg-hover);
}

.item.selected {
background: var(--bg-selected);
color: var(--text-inverse);
}

.item-title {
overflow: hidden;
text-overflow: ellipsis;
}

.empty {
padding: 8px 12px;
font-size: 12px;
color: var(--text-muted);
}
</style>
