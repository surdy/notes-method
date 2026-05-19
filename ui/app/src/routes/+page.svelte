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
	import { settingsStore } from '$lib/settings.svelte';
	import { workspaceChromeLayout } from '$lib/workspace-chrome';

	let vaults = $state<string[]>([]);
	let showCommandPalette = $state(false);
	let showQuickSwitcher = $state(false);
	let leftSidebarCollapsed = $state(false);
	let rightRailCollapsed = $state(false);
	let sidebarViewsRef = $state<{ refresh: () => void; reloadConfig: () => void } | null>(null);
	let noteEditorRef = $state<
		| {
				handleExternalChange: (path: string, hash?: string) => void;
				refreshSqlBlocks: () => void;
				flushSave: () => Promise<void>;
		  }
		| null
	>(null);
	let rightRailRef = $state<{ refresh: () => void } | null>(null);
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

	function toggleLeftSidebar() {
		leftSidebarCollapsed = !leftSidebarCollapsed;
	}

	function toggleRightRail() {
		rightRailCollapsed = !rightRailCollapsed;
	}

	const chromeLayout = $derived(
		workspaceChromeLayout({ leftSidebarCollapsed, rightRailCollapsed })
	);
	const workspaceChromeStyle = $derived(
		`--workspace-left-width: ${chromeLayout.leftChromeWidth}; --workspace-right-width: ${chromeLayout.rightChromeWidth};`
	);

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
		onToggleRightRail: toggleRightRail,
		onOpenSettings: () =>
			void goto(`${base}/settings?vault=${encodeURIComponent(vaultStore.currentVault)}`),
		onNotesChanged: refreshContextPanels,
		onExternalNoteChange: (path, hash) => {
			noteEditorRef?.handleExternalChange(path, hash);
		},
		onTaskUpdated: refreshContextPanels,
		onSidebarConfigChanged: () => {
			sidebarViewsRef?.reloadConfig();
		},
		onVaultConfigChanged: () => {
			if (vaultStore.currentVault) {
				void settingsStore.handleExternalConfigChange(vaultStore.currentVault);
			}
		},
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

	$effect(() => {
		const vault = vaultStore.currentVault;
		if (vault) {
			void settingsStore.loadConfig(vault);
		}
	});
</script>

<div class="page-shell" style={workspaceChromeStyle}>
<VersionBanner />

<div class="workspace-chrome" role="toolbar" aria-label="Workspace">
<section class="workspace-chrome-left" class:collapsed={leftSidebarCollapsed} aria-label="Sidebar controls">
<button
	class="chrome-icon-btn"
	type="button"
	onclick={toggleLeftSidebar}
	aria-label={chromeLayout.leftToggleLabel}
	aria-expanded={!leftSidebarCollapsed}
	aria-controls="left-sidebar"
	title={chromeLayout.leftToggleLabel}
>
	<span class={`sidebar-panel-icon ${chromeLayout.leftToggleIcon}`} aria-hidden="true"></span>
</button>
{#if !leftSidebarCollapsed}
<h1 class="app-title">📝 Notesmith</h1>
<button
	class="chrome-icon-btn"
	type="button"
	onclick={() => void goto(`${base}/settings?vault=${encodeURIComponent(vaultStore.currentVault)}`)}
	aria-label="Open settings"
	title="Settings (⌘,)"
>
	⚙
</button>
{/if}
</section>

<section class="workspace-chrome-main" aria-label="Open notes">
<TabBar />
</section>

<section class="workspace-chrome-right" class:collapsed={rightRailCollapsed} aria-label="Right sidebar controls">
{#if !rightRailCollapsed}
<span class="chrome-section-title">Context</span>
{/if}
<button
	class="chrome-icon-btn"
	type="button"
	onclick={toggleRightRail}
	aria-label={chromeLayout.rightToggleLabel}
	aria-expanded={!rightRailCollapsed}
	aria-controls="right-rail"
	title={chromeLayout.rightToggleLabel}
>
	<span class={`sidebar-panel-icon ${chromeLayout.rightToggleIcon}`} aria-hidden="true"></span>
</button>
</section>
</div>

<div class="app-layout">
<aside
	id="left-sidebar"
	class="sidebar"
	class:collapsed={leftSidebarCollapsed}
	aria-hidden={leftSidebarCollapsed}
>
<div class="sidebar-body">

{#if vaults.length > 1}
<VaultSwitcher {vaults} />
{/if}

<SidebarViews
bind:this={sidebarViewsRef}
onActivateMiddlePane={(item) => (activeMiddlePaneItem = item)}
onDeactivateMiddlePane={() => (activeMiddlePaneItem = null)}
/>
</div>
</aside>

{#if activeMiddlePaneItem}
<MiddlePane
item={activeMiddlePaneItem}
vault={vaultStore.currentVault}
onClose={() => (activeMiddlePaneItem = null)}
/>
{/if}

<main class="content-area">
<NoteToolbar />
{#if tabStore.activeViewMode === 'reading'}
<NoteViewer path={tabStore.selectedPath} />
{:else}
<NoteEditor bind:this={noteEditorRef} />
{/if}
</main>

<aside
	id="right-rail"
	class="right-rail-shell"
	class:collapsed={rightRailCollapsed}
	aria-hidden={rightRailCollapsed}
>
<RightRail bind:this={rightRailRef} collapsed={rightRailCollapsed} />
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
--workspace-left-width: 280px;
--workspace-right-width: 260px;
display: flex;
flex-direction: column;
height: 100vh;
overflow: hidden;
}

.workspace-chrome {
display: flex;
min-height: 38px;
border-bottom: 1px solid var(--ns-border);
background: var(--ns-panel-bg-strong);
}

.workspace-chrome-left,
.workspace-chrome-right {
display: flex;
align-items: center;
gap: 8px;
padding: 0 8px;
background: var(--ns-sidebar-bg);
overflow: hidden;
transition:
	flex-basis 180ms ease,
	width 180ms ease;
}

.workspace-chrome-left {
flex: 0 0 var(--workspace-left-width);
width: var(--workspace-left-width);
border-right: 1px solid var(--ns-border);
}

.workspace-chrome-right {
flex: 0 0 var(--workspace-right-width);
width: var(--workspace-right-width);
justify-content: space-between;
border-left: 1px solid var(--ns-border);
}

.workspace-chrome-left.collapsed,
.workspace-chrome-right.collapsed {
justify-content: center;
padding: 0;
}

.workspace-chrome-main {
display: flex;
flex: 1;
min-width: 0;
overflow: hidden;
}

.chrome-icon-btn {
display: inline-flex;
align-items: center;
justify-content: center;
width: 28px;
height: 28px;
padding: 0;
border: 1px solid transparent;
border-radius: 6px;
background: transparent;
color: var(--ns-text-muted);
cursor: pointer;
flex-shrink: 0;
}

.chrome-icon-btn:hover,
.chrome-icon-btn:focus-visible {
background: var(--ns-surface-hover);
color: var(--ns-text);
outline: none;
}

.sidebar-panel-icon {
position: relative;
display: inline-block;
width: 18px;
height: 18px;
border: 1.5px solid currentColor;
border-radius: 4px;
}

.sidebar-panel-icon::before {
content: '';
position: absolute;
top: 2px;
bottom: 2px;
width: 1.5px;
border-radius: 999px;
background: currentColor;
}

.sidebar-panel-icon.panel-left::before {
left: 5px;
}

.sidebar-panel-icon.panel-right::before {
right: 5px;
}

.app-layout {
display: flex;
flex: 1;
min-height: 0;
overflow: hidden;
}

.sidebar {
width: var(--workspace-left-width);
min-width: var(--workspace-left-width);
flex: 0 0 var(--workspace-left-width);
background: var(--ns-sidebar-bg);
border-right: 1px solid var(--ns-border);
display: flex;
flex-direction: column;
overflow: hidden;
transition:
	flex-basis 180ms ease,
	width 180ms ease,
	min-width 180ms ease;
}

.sidebar-body {
display: flex;
flex: 1;
flex-direction: column;
min-height: 0;
overflow: hidden;
}

.sidebar.collapsed .sidebar-body {
display: none;
}

.app-title {
margin: 0;
font-size: 16px;
font-weight: 600;
white-space: nowrap;
overflow: hidden;
text-overflow: ellipsis;
}

.chrome-section-title {
color: var(--ns-text-secondary);
font-size: 12px;
font-weight: 700;
letter-spacing: 0.08em;
text-transform: uppercase;
white-space: nowrap;
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
width: var(--workspace-right-width);
flex: 0 0 var(--workspace-right-width);
overflow: hidden;
transition:
	flex-basis 180ms ease,
	width 180ms ease;
}

@media (max-width: 768px) {
.page-shell {
--workspace-left-width: min(240px, 35vw);
}
}

@media (max-width: 480px) {
.app-layout {
flex-direction: column;
}

.sidebar {
width: 100%;
min-width: 0;
flex-basis: auto;
height: 40vh;
border-right: none;
border-bottom: 1px solid var(--ns-border);
}

.workspace-chrome-right {
display: none;
}
}
</style>
