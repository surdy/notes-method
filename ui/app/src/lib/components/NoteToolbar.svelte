<script lang="ts">
	import { tabStore } from '$lib/tab-store.svelte';
	import type { ViewMode } from '$lib/tab-state';

	let activeTab = $derived(tabStore.activeTab);
	let viewMode = $derived(tabStore.activeViewMode);

	function pathSegments(path: string): string[] {
		return path.replace(/\.md$/, '').split('/');
	}

	function selectMode(mode: ViewMode) {
		tabStore.setViewMode(mode);
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
			<div class="view-modes" role="group" aria-label="Editor view mode">
				<button
					class="view-mode"
					class:active={viewMode === 'source'}
					aria-pressed={viewMode === 'source'}
					aria-label="Source Mode"
					title="Source Mode (⌘E)"
					onclick={() => selectMode('source')}
				>
					<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<polyline points="16 18 22 12 16 6" />
						<polyline points="8 6 2 12 8 18" />
					</svg>
				</button>
				<button
					class="view-mode"
					class:active={viewMode === 'live-preview'}
					aria-pressed={viewMode === 'live-preview'}
					aria-label="Live Preview"
					title="Live Preview (⌘E)"
					onclick={() => selectMode('live-preview')}
				>
					<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
						<path d="m15 5 4 4" />
					</svg>
				</button>
				<button
					class="view-mode"
					class:active={viewMode === 'reading'}
					aria-pressed={viewMode === 'reading'}
					aria-label="Reading View"
					title="Reading View (⌘E)"
					onclick={() => selectMode('reading')}
				>
					<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
						<path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
					</svg>
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.note-toolbar {
		display: flex;
		align-items: center;
		height: 40px;
		padding: 0 16px;
		background: var(--bg-default);
		border-bottom: 1px solid var(--border-subtle);
		font-size: 12.5px;
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.toolbar-left,
	.toolbar-right {
		flex: 0 0 200px;
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
		color: var(--text-default);
		font-weight: 500;
	}

	.toolbar-right {
		display: flex;
		justify-content: flex-end;
	}

	.view-modes {
		display: flex;
		align-items: center;
		gap: 3px;
		padding: 3px;
		border: 1px solid var(--border-default);
		border-radius: var(--radius-sm);
		background: var(--bg-default);
	}

	.view-mode {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 26px;
		width: 26px;
		padding: 0;
		border: none;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--text-muted);
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}

	.view-mode:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.view-mode.active {
		background: var(--accent-bg);
		color: var(--text-default);
	}
</style>
