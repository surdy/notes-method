<script lang="ts">
import { fuzzyFilter } from '$lib/fuzzy';
import type { NoteSummary } from '$lib/api';
import { tabStore } from '$lib/tab-store.svelte';
import { vaultStore } from '$lib/stores.svelte';

let { onClose }: { onClose: () => void } = $props();

let query = $state('');
let selectedIndex = $state(0);
let inputRef: HTMLInputElement | undefined;
let resultsRef: HTMLDivElement | undefined;

let filtered = $derived.by(() => {
if (!query.trim()) {
return vaultStore.notes.slice(0, 20);
}

return fuzzyFilter(query, vaultStore.notes, (note) => `${note.title} ${note.path}`)
.slice(0, 20)
.map((match) => match.item);
});

function openNote(note: NoteSummary) {
tabStore.selectNote(note.path);
onClose();
}

function typeIcon(type: string): string {
const icons: Record<string, string> = {
daily: '📅',
meeting: '🤝',
customer: '🏢',
stream: '🔀',
note: '📝',
'account-info': 'ℹ️',
glossary: '📖',
milestones: '🏁'
};
return icons[type] ?? '📄';
}

function folderPath(path: string): string {
const parts = path.split('/');
return parts.slice(0, -1).join('/') || 'Vault root';
}

function handleKeydown(event: KeyboardEvent) {
switch (event.key) {
case 'ArrowDown':
event.preventDefault();
selectedIndex =
	filtered.length === 0 ? 0 : Math.min(selectedIndex + 1, filtered.length - 1);
break;
case 'ArrowUp':
event.preventDefault();
selectedIndex = filtered.length === 0 ? 0 : Math.max(selectedIndex - 1, 0);
break;
case 'Enter':
event.preventDefault();
if (filtered[selectedIndex]) {
openNote(filtered[selectedIndex]);
}
break;
case 'Escape':
event.preventDefault();
onClose();
break;
}
}

$effect(() => {
query;
selectedIndex = 0;
});

$effect(() => {
if (selectedIndex >= filtered.length) {
selectedIndex = Math.max(filtered.length - 1, 0);
}
});

$effect(() => {
inputRef?.focus();
});

$effect(() => {
selectedIndex;
const selected = resultsRef?.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`);
selected?.scrollIntoView({ block: 'nearest' });
});
</script>

<div
class="switcher-backdrop"
onclick={onClose}
onkeydown={handleKeydown}
role="dialog"
aria-modal="true"
>
<div class="switcher" onclick={(event) => event.stopPropagation()}>
<input
bind:this={inputRef}
bind:value={query}
class="switcher-input"
placeholder="Open a note..."
type="text"
/>

<div bind:this={resultsRef} class="switcher-results">
{#if filtered.length === 0}
<div class="no-results">No matching notes</div>
{:else}
{#each filtered as note, index (note.path)}
<button
class="switcher-item"
class:selected={index === selectedIndex}
data-index={index}
onclick={() => openNote(note)}
onmouseenter={() => (selectedIndex = index)}
type="button"
>
<span class="note-icon">{typeIcon(note.type)}</span>
<span class="note-body">
<span class="note-title">{note.title || note.path.split('/').at(-1)?.replace(/\.md$/, '')}</span>
<span class="note-meta">{folderPath(note.path)}</span>
</span>
</button>
{/each}
{/if}
</div>
</div>
</div>

<style>
.switcher-backdrop {
position: fixed;
inset: 0;
background: rgb(0 0 0 / 65%);
display: flex;
justify-content: center;
align-items: flex-start;
padding: min(18vh, 140px) 16px 16px;
z-index: 50;
}

.switcher {
width: min(640px, 100%);
max-height: min(60vh, 720px);
display: flex;
flex-direction: column;
background: #2d2d2d;
border: 1px solid rgb(255 255 255 / 8%);
border-radius: 16px;
box-shadow: 0 24px 60px rgb(0 0 0 / 45%);
overflow: hidden;
}

.switcher-input {
width: 100%;
padding: 18px 20px;
border: none;
outline: none;
background: #2d2d2d;
color: var(--text-primary, #e0e0e0);
font-size: 17px;
border-bottom: 1px solid rgb(255 255 255 / 8%);
}

.switcher-results {
overflow-y: auto;
padding: 8px;
}

.switcher-item {
width: 100%;
display: flex;
align-items: center;
gap: 14px;
padding: 12px 14px;
border: none;
border-radius: 10px;
background: transparent;
color: var(--text-primary, #e0e0e0);
cursor: pointer;
text-align: left;
}

.switcher-item:hover,
.switcher-item.selected {
background: var(--hover-bg, #3a3a3a);
}

.note-icon {
font-size: 18px;
line-height: 1;
}

.note-body {
display: flex;
flex: 1;
flex-direction: column;
gap: 2px;
min-width: 0;
}

.note-title {
font-size: 14px;
font-weight: 500;
}

.note-meta {
font-size: 12px;
color: var(--text-muted, #989898);
text-overflow: ellipsis;
overflow: hidden;
white-space: nowrap;
}

.no-results {
padding: 24px;
text-align: center;
color: var(--text-muted, #8f8f8f);
}
</style>
