<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import type { CustomItem } from '$lib/api';
	import { createAppShell } from '$lib/app-shell.svelte';
	import { buildCommands } from '$lib/commands';
	import { themeStore } from '$lib/theme.svelte';
	import ConfigToast from '$lib/components/ConfigToast.svelte';
	import InputPalette from '$lib/components/InputPalette.svelte';
	import MiddlePane from '$lib/components/MiddlePane.svelte';
	import NoteEditor from '$lib/components/NoteEditor.svelte';
	import NoteToolbar from '$lib/components/NoteToolbar.svelte';
	import NoteViewer from '$lib/components/NoteViewer.svelte';
	import RightRail from '$lib/components/RightRail.svelte';
	import SidebarViews from '$lib/components/SidebarViews.svelte';
	import StatusBar from '$lib/components/StatusBar.svelte';
	import TabBar from '$lib/components/TabBar.svelte';
	import ToastStack from '$lib/components/ToastStack.svelte';
	import UnifiedPalette from '$lib/components/UnifiedPalette.svelte';
	import VaultMenu from '$lib/components/VaultMenu.svelte';
	import VersionBanner from '$lib/components/VersionBanner.svelte';
	import OpenFolderAsVaultModal from '$lib/components/OpenFolderAsVaultModal.svelte';
	import { versionMismatch } from '$lib/api/core';
	import { inputPalette } from '$lib/input-palette.svelte';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { settingsStore } from '$lib/settings.svelte';
	import { workspaceChromeLayout } from '$lib/workspace-chrome';
	import { pushWindowTitle } from '$lib/window-title';

	let vaults = $state<string[]>([]);
	let paletteMode = $state<'files' | 'commands' | null>(null);
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
	let showOpenFolderModal = $state(false);
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

	function openPalette(mode: 'files' | 'commands') {
		paletteMode = mode;
	}

	async function handleToggleView() {
		if (tabStore.activeViewMode === 'live-preview') {
			await noteEditorRef?.flushSave();
		}

		tabStore.toggleViewMode();
	}

	const shell = createAppShell({
		onOpenCommandPalette: () => openPalette('commands'),
		onOpenQuickSwitcher: () => openPalette('files'),
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

		const openFolderHandler = () => {
			showOpenFolderModal = true;
		};
		window.addEventListener('notesmith://open-folder-as-vault', openFolderHandler);

		// Listen for settings menu event from Tauri
		let unlistenSettings: (() => void) | null = null;
		const tauri = (window as unknown as { __TAURI__?: { event?: { listen: (event: string, handler: (e: unknown) => void) => Promise<() => void> } } }).__TAURI__;
		if (tauri?.event?.listen) {
			void tauri.event.listen('notesmith://open-settings', () => {
				void goto(`${base}/settings?vault=${encodeURIComponent(vaultStore.currentVault)}`);
			}).then((fn) => { unlistenSettings = fn; });
		}

		return () => {
			window.removeEventListener('notesmith://open-folder-as-vault', openFolderHandler);
			unlistenSettings?.();
			shell.teardown();
		};
	});

	$effect(() => {
		const vault = vaultStore.currentVault;
		if (vault) {
			void settingsStore.loadConfig(vault);
		}
	});

	$effect(() => {
		const config = settingsStore.serverConfig;
		if (config?.appearance) {
			themeStore.applyFromConfig(config.appearance);
		}
	});

	$effect(() => {
		const vault = vaultStore.currentVault;
		const activeTitle = tabStore.activeTab?.title ?? null;
		void pushWindowTitle(vault, activeTitle);
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
<VaultMenu {vaults} currentVault={vaultStore.currentVault} />
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

<main class="content-area editor-surface">
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

{#if paletteMode}
<UnifiedPalette
	{commands}
	initialMode={paletteMode}
	onClose={() => (paletteMode = null)}
	onSelectNote={(path) => tabStore.selectNote(path)}
	onCreateNote={async (title) => {
		const { createNote } = await import('$lib/api');
		try {
			const created = await createNote(vaultStore.currentVault, title, `# ${title}\n`, 'Inbox');
			await vaultStore.loadNotes();
			tabStore.selectNote(created.path);
		} catch (error) {
			console.error('Failed to create note from palette', error);
		}
	}}
/>
{/if}

{#if showOpenFolderModal}
<OpenFolderAsVaultModal onClose={() => (showOpenFolderModal = false)} />
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
border-bottom: 1px solid var(--border-default);
background: var(--bg-panel);
}

.workspace-chrome-left,
.workspace-chrome-right {
display: flex;
align-items: center;
gap: 8px;
padding: 0 8px;
background: var(--bg-secondary);
overflow: hidden;
transition:
	flex-basis 180ms ease,
	width 180ms ease;
}

.workspace-chrome-left {
flex: 0 0 var(--workspace-left-width);
width: var(--workspace-left-width);
border-right: 1px solid var(--border-default);
}

.workspace-chrome-right {
flex: 0 0 var(--workspace-right-width);
width: var(--workspace-right-width);
justify-content: space-between;
border-left: 1px solid var(--border-default);
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
color: var(--text-muted);
cursor: pointer;
flex-shrink: 0;
}

.chrome-icon-btn:hover,
.chrome-icon-btn:focus-visible {
background: var(--bg-hover);
color: var(--text-default);
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
border-radius: 1px;
}

/* Left sidebar closed: vertical line divider on left side */
.sidebar-panel-icon.panel-left-closed::before {
left: 5px;
width: 1.5px;
background: currentColor;
}

/* Left sidebar open: filled panel on left side */
.sidebar-panel-icon.panel-left-open::before {
left: 2px;
width: 6px;
background: currentColor;
border-radius: 2px;
}

/* Right sidebar closed: vertical line divider on right side */
.sidebar-panel-icon.panel-right-closed::before {
right: 5px;
width: 1.5px;
background: currentColor;
}

/* Right sidebar open: filled panel on right side */
.sidebar-panel-icon.panel-right-open::before {
right: 2px;
width: 6px;
background: currentColor;
border-radius: 2px;
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
background: var(--bg-secondary);
border-right: 1px solid var(--border-default);
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

.chrome-section-title {
color: var(--text-secondary);
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
border-bottom: 1px solid var(--border-default);
}

.workspace-chrome-right {
display: none;
}
}
</style>
