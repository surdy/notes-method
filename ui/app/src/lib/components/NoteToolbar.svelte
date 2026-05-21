<script lang="ts">
	import { tabStore } from '$lib/tab-store.svelte';

	let activeTab = $derived(tabStore.activeTab);
	let viewMode = $derived(tabStore.activeViewMode);

	function pathSegments(path: string): string[] {
		return path.replace(/\.md$/, '').split('/');
	}

	function handleToggle() {
		tabStore.toggleViewMode();
	}
</script>

{#if activeTab}
	{@const segments = pathSegments(activeTab.path)}
	<div class="note-toolbar">
		<div class="toolbar-left"></div>

		<div class="toolbar-center">
			<span class="note-path">
				{#each segments as segment, i}
					{#if i > 0}<span class="path-separator">/</span>{/if}
					<span class="path-segment" class:path-leaf={i === segments.length - 1}
						>{i === segments.length - 1 ? activeTab.title : segment}</span
					>
				{/each}
			</span>
		</div>

		<div class="toolbar-right">
			<button
				class="view-toggle"
				onclick={handleToggle}
				title={viewMode === 'source' ? 'Switch to Live Preview (⌘E)' : viewMode === 'live-preview' ? 'Switch to Reading View (⌘E)' : 'Switch to Source Mode (⌘E)'}
				aria-label={viewMode === 'source' ? 'Switch to Live Preview' : viewMode === 'live-preview' ? 'Switch to Reading View' : 'Switch to Source Mode'}
			>
				{#if viewMode === 'source'}
					<!-- Code brackets icon for Source Mode -->
					<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<polyline points="16 18 22 12 16 6" />
						<polyline points="8 6 2 12 8 18" />
					</svg>
				{:else if viewMode === 'live-preview'}
					<!-- Pencil icon for Live Preview -->
					<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
						<path d="m15 5 4 4" />
					</svg>
				{:else}
					<!-- Book icon for Reading View -->
					<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
						<path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
					</svg>
				{/if}
			</button>
		</div>
	</div>
{/if}

<style>
	.note-toolbar {
		display: flex;
		align-items: center;
		height: 32px;
		padding: 0 12px;
		background: var(--ns-surface);
		border-bottom: 1px solid var(--ns-border);
		font-size: 12px;
		color: var(--ns-text-muted);
		flex-shrink: 0;
	}

	.toolbar-left,
	.toolbar-right {
		flex: 0 0 40px;
	}

	.toolbar-center {
		flex: 1;
		display: flex;
		justify-content: center;
		overflow: hidden;
	}

	.note-path {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.path-separator {
		margin: 0 3px;
		opacity: 0.5;
	}

	.path-leaf {
		color: var(--ns-text);
		font-weight: 500;
	}

	.toolbar-right {
		display: flex;
		justify-content: flex-end;
	}

	.view-toggle {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 24px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--ns-text-muted);
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}

	.view-toggle:hover {
		background: var(--ns-surface-hover-subtle);
		color: var(--ns-text);
	}
</style>
