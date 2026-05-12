<script lang="ts">
import { onMount } from 'svelte';
import type { CustomItem } from '$lib/api';
import { buildCommands, OPEN_QUICK_SWITCHER_EVENT } from '$lib/commands';
import CommandPalette from '$lib/components/CommandPalette.svelte';
import ConfigToast from '$lib/components/ConfigToast.svelte';
import MiddlePane from '$lib/components/MiddlePane.svelte';
import NoteEditor from '$lib/components/NoteEditor.svelte';
import NoteToolbar from '$lib/components/NoteToolbar.svelte';
import NoteViewer from '$lib/components/NoteViewer.svelte';
import QuickSwitcher from '$lib/components/QuickSwitcher.svelte';
import RightRail from '$lib/components/RightRail.svelte';
import SidebarViews from '$lib/components/SidebarViews.svelte';
import TabBar from '$lib/components/TabBar.svelte';
import VaultSwitcher from '$lib/components/VaultSwitcher.svelte';
import { registerHotkeys } from '$lib/hotkeys';
import { connectSSE } from '$lib/sse';
import { vaultStore } from '$lib/stores.svelte';

let vaults = $state<string[]>([]);
let showCommandPalette = $state(false);
let showQuickSwitcher = $state(false);
let sseConnection: EventSource | null = null;
let sidebarViewsRef = $state<{ refresh: () => void; reloadConfig: () => void } | null>(null);
let noteEditorRef = $state<
| {
handleExternalChange: (path: string) => void;
refreshSqlBlocks: () => void;
flushSave: () => Promise<void>;
  }
| null
>(null);
let rightRailRef = $state<{ refresh: () => void; toggle: () => void } | null>(null);
let activeMiddlePaneItem = $state<CustomItem | null>(null);
let configToastRef = $state<{ show: (message: string, type: 'info' | 'error') => void } | null>(
	null
);

function showConfigToast(message: string, type: 'info' | 'error') {
	configToastRef?.show(message, type);
}

let commands = $derived.by(() =>
buildCommands(vaultStore.currentVault, (path) => {
vaultStore.selectNote(path);
})
);

function openCommandPalette() {
showCommandPalette = true;
showQuickSwitcher = false;
}

function openQuickSwitcher() {
showQuickSwitcher = true;
showCommandPalette = false;
}

function runCommand(commandId: string) {
const command = commands.find((candidate) => candidate.id === commandId);
if (command) {
void Promise.resolve(command.execute()).catch((error) => {
console.error(`Failed to execute command: ${commandId}`, error);
});
}
}

async function handleToggleView() {
if (vaultStore.activeViewMode === 'live-preview') {
await noteEditorRef?.flushSave();
}
vaultStore.toggleViewMode();
}

onMount(() => {
const handleOpenQuickSwitcher = () => openQuickSwitcher();
const refreshContextPanels = () => {
sidebarViewsRef?.refresh();
rightRailRef?.refresh();
noteEditorRef?.refreshSqlBlocks();
};

window.addEventListener(OPEN_QUICK_SWITCHER_EVENT, handleOpenQuickSwitcher as EventListener);

void (async () => {
try {
const url = new URL(window.location.href);
const vault = url.searchParams.get('vault') ?? 'work';
vaults = [vault];
vaultStore.currentVault = vault;
vaultStore.restoreTabs();
await vaultStore.loadNotes();

sseConnection = connectSSE(
vault,
(event) => {
const refreshNotes =
event.type.startsWith('note.') ||
event.type === 'inbox.added' ||
event.type === 'daily.created' ||
event.type === 'cache.rebuilt';
if (refreshNotes) {
void vaultStore.loadNotes().finally(() => {
	refreshContextPanels();
});
}
if (event.type === 'note.updated' || event.type === 'note.created') {
noteEditorRef?.handleExternalChange(event.path);
}
if (!refreshNotes && event.type === 'task.updated') {
refreshContextPanels();
}
// Config change events
if (event.type.startsWith('config.')) {
if (event.config?.key === 'sidebar') {
	if (event.config.status === 'error') {
		showConfigToast(
			`Sidebar config error: ${event.config.error ?? 'unknown error'}`,
			'error'
		);
	} else {
		sidebarViewsRef?.reloadConfig();
	}
}
if (event.config?.key === 'vault') {
	if (event.config.status === 'error') {
		showConfigToast(
			`Vault config error: ${event.config.error ?? 'unknown error'}`,
			'error'
		);
	}
	// vault.toml hot-swap will be handled in issue #41
}
}
},
() => {
// Defensive refetch on SSE reconnect — events may have been missed.
sidebarViewsRef?.reloadConfig();
}
);
} catch (error) {
console.error('Failed to initialize Notesmith app shell', error);
}
})();

const unregister = registerHotkeys([
{ key: 'k', meta: true, action: openCommandPalette },
{ key: 'p', meta: true, action: openCommandPalette },
{ key: 'o', meta: true, action: openQuickSwitcher },
{ key: 'w', meta: true, action: () => vaultStore.closeActiveTab() },
{ key: 'n', meta: true, action: () => runCommand('new-note') },
{ key: 'd', meta: true, action: () => runCommand('open-daily') },
{ key: 'a', meta: true, shift: true, action: () => runCommand('archive-current') },
{ key: 'i', meta: true, shift: true, action: () => runCommand('inbox-capture') },
{ key: 'n', meta: true, shift: true, action: () => runCommand('new-from-template') },
{ key: 's', meta: true, action: () => {} },
{ key: 'e', meta: true, action: () => void handleToggleView() },
{ key: 't', meta: true, shift: true, action: () => vaultStore.reopenLastTab() },
{ key: '\\', meta: true, action: () => rightRailRef?.toggle() },
{ key: 'f', meta: true, shift: true, action: openQuickSwitcher }
]);

return () => {
window.removeEventListener(
OPEN_QUICK_SWITCHER_EVENT,
handleOpenQuickSwitcher as EventListener
);
sseConnection?.close();
unregister();
};
});
</script>

<div class="app-layout">
<aside class="sidebar">
<div class="sidebar-header">
<h1 class="app-title">📝 Notesmith</h1>
</div>

{#if vaults.length > 1}
<VaultSwitcher {vaults} />
{/if}

<SidebarViews
bind:this={sidebarViewsRef}
onActivateMiddlePane={(item) => (activeMiddlePaneItem = item)}
onDeactivateMiddlePane={() => (activeMiddlePaneItem = null)}
/>
</aside>

{#if activeMiddlePaneItem}
<MiddlePane
item={activeMiddlePaneItem}
vault={vaultStore.currentVault}
onClose={() => (activeMiddlePaneItem = null)}
/>
{/if}

<main class="content-area">
<TabBar />
<NoteToolbar />
{#if vaultStore.activeViewMode === 'reading'}
<NoteViewer path={vaultStore.selectedPath} />
{:else}
<NoteEditor bind:this={noteEditorRef} />
{/if}
</main>

<aside class="right-rail-shell">
<RightRail bind:this={rightRailRef} />
</aside>
</div>

{#if showCommandPalette}
<CommandPalette commands={commands} onClose={() => (showCommandPalette = false)} />
{/if}

{#if showQuickSwitcher}
<QuickSwitcher onClose={() => (showQuickSwitcher = false)} />
{/if}

<ConfigToast bind:this={configToastRef} />

<style>
.app-layout {
display: flex;
height: 100vh;
overflow: hidden;
}

.sidebar {
width: 280px;
min-width: 200px;
background: var(--sidebar-bg, #252526);
border-right: 1px solid var(--border-color, #333);
display: flex;
flex-direction: column;
overflow: hidden;
}

.sidebar-header {
padding: 12px 16px;
border-bottom: 1px solid var(--border-color, #333);
}

.app-title {
margin: 0;
font-size: 16px;
font-weight: 600;
}

.content-area {
flex: 1;
display: flex;
flex-direction: column;
overflow: hidden;
}

.right-rail-shell {
position: relative;
display: flex;
flex: 0 0 auto;
overflow: visible;
}

@media (max-width: 768px) {
.sidebar {
width: 240px;
}
}

@media (max-width: 480px) {
.app-layout {
flex-direction: column;
}

.sidebar {
width: 100%;
height: 40vh;
border-right: none;
border-bottom: 1px solid var(--border-color, #333);
}
}
</style>
