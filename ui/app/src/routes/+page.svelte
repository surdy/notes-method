<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import type { CustomItem } from '$lib/api';
	import { createAppShell } from '$lib/app-shell.svelte';
	import { buildCommands } from '$lib/commands';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import ConfigToast from '$lib/components/ConfigToast.svelte';
	import InputPalette from '$lib/components/InputPalette.svelte';
	import MiddlePane from '$lib/components/MiddlePane.svelte';
	import NoteEditor from '$lib/components/NoteEditor.svelte';
	import NoteToolbar from '$lib/components/NoteToolbar.svelte';
	import NoteViewer from '$lib/components/NoteViewer.svelte';
	import QuickSwitcher from '$lib/components/QuickSwitcher.svelte';
	import RightRail from '$lib/components/RightRail.svelte';
	import SidebarViews from '$lib/components/SidebarViews.svelte';
	import StatusBar from '$lib/components/StatusBar.svelte';
	import TabBar from '$lib/components/TabBar.svelte';
	import ToastStack from '$lib/components/ToastStack.svelte';
	import VersionBanner from '$lib/components/VersionBanner.svelte';
	import VaultSwitcher from '$lib/components/VaultSwitcher.svelte';
	import { versionMismatch } from '$lib/api/core';
	import { inputPalette } from '$lib/input-palette.svelte';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let vaults = $state<string[]>([]);
	let showCommandPalette = $state(false);
	let showQuickSwitcher = $state(false);
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
	let configToastRef = $state<
		{ show: (message: string, type: 'info' | 'error') => void } | null
	>(null);

	function showConfigToast(message: string, type: 'info' | 'error') {
		configToastRef?.show(message, type);
	}

	function refreshContextPanels() {
		sidebarViewsRef?.refresh();
		rightRailRef?.refresh();
		noteEditorRef?.refreshSqlBlocks();
	}

	let commands = $derived.by(() =>
		buildCommands(vaultStore.currentVault, (path) => {
			tabStore.selectNote(path);
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

	async function handleToggleView() {
		if (tabStore.activeViewMode === 'live-preview') {
			await noteEditorRef?.flushSave();
		}

		tabStore.toggleViewMode();
	}

	const shell = createAppShell({
		onOpenCommandPalette: openCommandPalette,
		onOpenQuickSwitcher: openQuickSwitcher,
		onToggleView: handleToggleView,
		onToggleRightRail: () => rightRailRef?.toggle(),
		onOpenSettings: () =>
			void goto(`${base}/settings?vault=${encodeURIComponent(vaultStore.currentVault)}`),
		onNotesChanged: refreshContextPanels,
		onExternalNoteChange: (path) => {
			noteEditorRef?.handleExternalChange(path);
		},
		onTaskUpdated: refreshContextPanels,
		onSidebarConfigChanged: () => {
			sidebarViewsRef?.reloadConfig();
		},
		onVaultConfigChanged: () => {},
		onConfigError: (error) => {
			showConfigToast(error, 'error');
		},
		commands: () => commands
	});

	onMount(() => {
		const url = new URL(window.location.href);
		const vaultParam = url.searchParams.get('vault');

		void shell.init(vaultParam, vaults);

		return shell.teardown;
	});
</script>

<div class="page-shell">
<VersionBanner />

<div class="app-layout">
<aside class="sidebar">
<div class="sidebar-header">
<h1 class="app-title">📝 Notesmith</h1>
<button class="gear-btn" type="button" onclick={() => void goto(`${base}/settings?vault=${encodeURIComponent(vaultStore.currentVault)}`)} aria-label="Open settings" title="Settings (⌘,)">⚙</button>
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
{#if tabStore.activeViewMode === 'reading'}
<NoteViewer path={tabStore.selectedPath} />
{:else}
<NoteEditor bind:this={noteEditorRef} />
{/if}
</main>

<aside class="right-rail-shell">
<RightRail bind:this={rightRailRef} />
</aside>
</div>

<StatusBar
	currentVault={vaultStore.currentVault}
	onToast={showConfigToast}
	restartRequired={Boolean($versionMismatch)}
/>
</div>

{#if showCommandPalette}
<CommandPalette commands={commands} onClose={() => (showCommandPalette = false)} />
{/if}

{#if showQuickSwitcher}
<QuickSwitcher onClose={() => (showQuickSwitcher = false)} />
{/if}

<ConfigToast bind:this={configToastRef} />

{#if inputPalette.request}
<InputPalette />
{/if}

<ToastStack />

<style>
.page-shell {
display: flex;
flex-direction: column;
height: 100vh;
overflow: hidden;
}

.app-layout {
display: flex;
flex: 1;
min-height: 0;
overflow: hidden;
}

.sidebar {
width: 280px;
min-width: 200px;
background: var(--ns-sidebar-bg);
border-right: 1px solid var(--ns-border);
display: flex;
flex-direction: column;
overflow: hidden;
}

.sidebar-header {
padding: 12px 16px;
border-bottom: 1px solid var(--ns-border);
display: flex;
align-items: center;
justify-content: space-between;
}

.app-title {
margin: 0;
font-size: 16px;
font-weight: 600;
}

.gear-btn {
background: none;
border: none;
color: var(--ns-text-muted);
font-size: 16px;
cursor: pointer;
padding: 2px 6px;
border-radius: 4px;
}

.gear-btn:hover {
background: var(--ns-surface-hover);
color: var(--ns-text);
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
border-bottom: 1px solid var(--ns-border);
}
}
</style>
