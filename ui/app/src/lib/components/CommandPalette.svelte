<script lang="ts">
import { fuzzyFilter, type FuzzyMatch } from '$lib/fuzzy';
import type { Command } from '$lib/commands';
import { vaultStore } from '$lib/stores.svelte';

let { commands, onClose }: { commands: Command[]; onClose: () => void } = $props();


type CommandSection = {
id: string;
label: string;
items: FuzzyMatch<Command>[];
};

const CATEGORY_ORDER: Command['category'][] = [
'Notes',
'Tasks',
'Templates',
'Navigation',
'Vault'
];

let query = $state('');
let selectedIndex = $state(0);
let inputRef: HTMLInputElement | undefined;
let resultsRef: HTMLDivElement | undefined;
let recentIds = $state<string[]>(loadRecentIds());

function recentCommandsKey(): string | null {
const vault = vaultStore.currentVault;
if (!vault) return null;
return `notesmith:recent-commands:${vault}`;
}

function loadRecentIds(): string[] {
try {
const key = recentCommandsKey();
if (!key) return [];
const stored = localStorage.getItem(key);
if (!stored) return [];
const parsed = JSON.parse(stored);
return Array.isArray(parsed) ? parsed.filter((value): value is string => typeof value === 'string') : [];
} catch {
return [];
}
}

function saveRecentIds(ids: string[]) {
try {
const key = recentCommandsKey();
if (!key) return;
localStorage.setItem(key, JSON.stringify(ids));
} catch {
// Ignore storage failures.
}
}

function groupCommands(items: FuzzyMatch<Command>[]): CommandSection[] {
return CATEGORY_ORDER.map((category) => ({
id: category,
label: category,
items: items.filter((item) => item.item.category === category)
}))
.filter((section) => section.items.length > 0);
}

let sections = $derived.by(() => {
if (!query.trim()) {
const recentMatches = recentIds
.map((id) => commands.find((command) => command.id === id))
.filter((command): command is Command => !!command)
.map((command) => ({ item: command, score: 0, highlights: [] }));

const restMatches = commands
.filter((command) => !recentIds.includes(command.id))
.map((command) => ({ item: command, score: 0, highlights: [] }));

return [
...(recentMatches.length > 0
? [{ id: 'Recent', label: 'Recent', items: recentMatches } satisfies CommandSection]
: []),
...groupCommands(restMatches)
];
}

const recentBoost = new Map(recentIds.map((id, index) => [id, recentIds.length - index]));
const matches = fuzzyFilter(query, commands, (command) => command.label)
.map((match) => ({
...match,
score: match.score + (recentBoost.get(match.item.id) ?? 0)
}))
.sort((left, right) => {
if (right.score !== left.score) {
return right.score - left.score;
}
return left.item.label.localeCompare(right.item.label);
});

return groupCommands(matches);
});

let visibleCommands = $derived.by(() => sections.flatMap((section) => section.items));

function execute(command: Command) {
const nextRecents = [command.id, ...recentIds.filter((id) => id !== command.id)].slice(0, 10);
recentIds = nextRecents;
saveRecentIds(nextRecents);

onClose();
void Promise.resolve(command.execute()).catch((error) => {
console.error('Command failed', error);
});
}

function handleKeydown(event: KeyboardEvent) {
switch (event.key) {
case 'ArrowDown':
event.preventDefault();
selectedIndex =
	visibleCommands.length === 0
		? 0
		: Math.min(selectedIndex + 1, visibleCommands.length - 1);
break;
case 'ArrowUp':
event.preventDefault();
selectedIndex = visibleCommands.length === 0 ? 0 : Math.max(selectedIndex - 1, 0);
break;
case 'Enter':
event.preventDefault();
if (visibleCommands[selectedIndex]) {
execute(visibleCommands[selectedIndex].item);
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
if (selectedIndex >= visibleCommands.length) {
selectedIndex = Math.max(visibleCommands.length - 1, 0);
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
class="palette-backdrop"
onclick={(event) => event.target === event.currentTarget && onClose()}
onkeydown={handleKeydown}
role="dialog"
aria-modal="true"
tabindex="-1"
>
<div class="palette">
<input
bind:this={inputRef}
bind:value={query}
class="palette-input"
placeholder="Type a command..."
type="text"
/>

<div bind:this={resultsRef} class="palette-results">
{#if visibleCommands.length === 0}
<div class="no-results">No matching commands</div>
{:else}
{@const selectedCommandId = visibleCommands[selectedIndex]?.item.id}
{#each sections as section (section.id)}
<section class="palette-section">
<h2 class="section-title">{section.label}</h2>
{#each section.items as match, index (match.item.id)}
{@const flatIndex = visibleCommands.findIndex((candidate) => candidate.item.id === match.item.id)}
<button
class="palette-item"
class:selected={selectedCommandId === match.item.id}
data-index={flatIndex}
onclick={() => execute(match.item)}
onmouseenter={() => (selectedIndex = flatIndex)}
type="button"
>
<span class="item-label">{match.item.label}</span>
<span class="item-meta">
<span class="item-category">{match.item.category}</span>
{#if match.item.shortcut}
<kbd class="item-shortcut">{match.item.shortcut}</kbd>
{/if}
</span>
</button>
{/each}
</section>
{/each}
{/if}
</div>

<div class="palette-footer">
<span class="hint"><kbd>↑↓</kbd> navigate</span>
<span class="hint"><kbd>Enter</kbd> select</span>
<span class="hint"><kbd>Esc</kbd> cancel</span>
</div>
</div>
</div>

<style>
.palette-backdrop {
position: fixed;
inset: 0;
background: var(--ns-overlay);
display: flex;
justify-content: center;
align-items: flex-start;
padding: min(18vh, 140px) 16px 16px;
z-index: 50;
}

.palette {
width: min(600px, 100%);
max-height: min(60vh, 720px);
display: flex;
flex-direction: column;
background: var(--ns-panel-bg-strong);
border: 1px solid var(--ns-border-overlay);
border-radius: 16px;
box-shadow: var(--ns-shadow);
overflow: hidden;
}

.palette-input {
width: 100%;
padding: 18px 20px;
border: none;
outline: none;
background: var(--ns-panel-bg-strong);
color: var(--ns-text);
font-size: 17px;
border-bottom: 1px solid var(--ns-border-overlay);
}

.palette-results {
overflow-y: auto;
padding: 8px;
}

.palette-section + .palette-section {
margin-top: 10px;
}

.section-title {
margin: 0;
padding: 6px 10px;
font-size: 12px;
font-weight: 700;
letter-spacing: 0.08em;
text-transform: uppercase;
color: var(--ns-text-muted-soft);
}

.palette-item {
width: 100%;
display: flex;
align-items: center;
justify-content: space-between;
gap: 12px;
padding: 12px 14px;
border: none;
border-radius: 10px;
background: transparent;
color: var(--ns-text);
cursor: pointer;
text-align: left;
}

.palette-item:hover,
.palette-item.selected {
background: var(--ns-surface-hover-strong);
}

.item-label {
font-size: 14px;
font-weight: 500;
}

.item-meta {
display: flex;
align-items: center;
gap: 10px;
white-space: nowrap;
}

.item-category {
font-size: 12px;
color: var(--ns-text-muted-strong);
}

.item-shortcut {
padding: 3px 8px;
border-radius: 999px;
background: var(--ns-surface-hover-strong);
border: 1px solid var(--ns-kbd-border);
font-size: 12px;
color: var(--ns-text);
}

.no-results {
padding: 24px;
text-align: center;
color: var(--ns-text-muted-soft);
}

.palette-footer {
display: flex;
align-items: center;
gap: 12px;
flex-wrap: wrap;
padding: 12px 16px;
border-top: 1px solid var(--ns-border-overlay);
background: var(--ns-surface-translucent-subtle);
}

.hint {
display: inline-flex;
align-items: center;
gap: 6px;
font-size: 12px;
color: var(--ns-text-muted-strong);
}

.hint kbd {
padding: 3px 8px;
border-radius: 999px;
background: var(--ns-kbd-bg);
border: 1px solid var(--ns-kbd-border);
font-size: 12px;
color: var(--ns-text);
}
</style>
