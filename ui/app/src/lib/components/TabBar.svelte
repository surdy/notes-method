<script lang="ts">
	import { tabStore, type Tab } from '$lib/tab-store.svelte';

	let dragIndex = $state<number | null>(null);
	let dropIndex = $state<number | null>(null);
	let tabs = $derived(tabStore.tabs as Tab[]);

	function handleClose(event: MouseEvent, index: number) {
		event.stopPropagation();
		tabStore.closeTab(index);
	}

	function handleMousedown(event: MouseEvent, index: number) {
		if (event.button === 1) {
			event.preventDefault();
			tabStore.closeTab(index);
		}
	}

	function handleKeydown(event: KeyboardEvent, index: number) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			tabStore.switchToTab(index);
		}
	}

	function handleDragStart(event: DragEvent, index: number) {
		dragIndex = index;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', tabs[index]?.path ?? '');
		}
	}

	function handleDragOver(event: DragEvent, index: number) {
		event.preventDefault();
		dropIndex = index;
	}

	function handleDrop(event: DragEvent, index: number) {
		event.preventDefault();
		if (dragIndex !== null && dragIndex !== index) {
			tabStore.moveTab(dragIndex, index);
		}
		dragIndex = null;
		dropIndex = null;
	}

	function handleDragEnd() {
		dragIndex = null;
		dropIndex = null;
	}
</script>

{#if tabs.length > 0}
	<div class="tab-bar" role="tablist" aria-label="Open notes">
		{#each tabs as tab, index (tab.path)}
			<div
				class="tab-shell"
				class:active={index === tabStore.activeTabIndex}
				class:drag-over={dropIndex === index && dragIndex !== index}
			>
				<div
					class="tab"
					role="tab"
					tabindex={index === tabStore.activeTabIndex ? 0 : -1}
					aria-selected={index === tabStore.activeTabIndex}
					draggable="true"
					onclick={() => tabStore.switchToTab(index)}
					onkeydown={(event) => handleKeydown(event, index)}
					ondragstart={(event) => handleDragStart(event, index)}
					ondragover={(event) => handleDragOver(event, index)}
					ondrop={(event) => handleDrop(event, index)}
					ondragend={handleDragEnd}
					onmousedown={(event) => handleMousedown(event, index)}
				>
					<span class="tab-title">
						{#if tab.dirty}
							<span class="dirty-dot">●</span>
						{/if}
						<span class="tab-label">{tab.title}</span>
					</span>
				</div>
				<button
					class="tab-close"
					onclick={(event) => handleClose(event, index)}
					type="button"
					aria-label={`Close ${tab.title}`}
				>
					×
				</button>
			</div>
		{/each}
	</div>
{/if}

<style>
	.tab-bar {
		display: flex;
		align-items: stretch;
		gap: 1px;
		min-height: 36px;
		padding: 0 8px;
		overflow-x: auto;
		overflow-y: hidden;
		background: #2d2d2d;
		border-bottom: 1px solid var(--border-color, #333);
		scrollbar-width: thin;
	}

	.tab-shell {
		position: relative;
		display: flex;
		align-items: stretch;
		flex: 0 0 auto;
		min-width: 0;
		max-width: 240px;
		margin-top: 1px;
		background: #252526;
		border-top: 2px solid transparent;
		transition:
			background-color 120ms ease,
			border-color 120ms ease;
	}

	.tab-shell.active {
		background: #1e1e1e;
		border-top-color: var(--text-accent, #7ec8e3);
	}

	.tab-shell.drag-over::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		border-bottom: 2px solid var(--text-accent, #7ec8e3);
	}

	.tab {
		display: flex;
		align-items: center;
		min-width: 0;
		padding: 0 32px 0 12px;
		color: var(--text-primary, #e0e0e0);
		cursor: pointer;
		outline: none;
	}

	.tab:focus-visible {
		box-shadow: inset 0 0 0 1px var(--text-accent, #7ec8e3);
	}

	.tab-title {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		font-size: 13px;
	}

	.tab-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.dirty-dot {
		color: #ffb347;
		font-size: 12px;
		line-height: 1;
	}

	.tab-close {
		position: absolute;
		top: 50%;
		right: 8px;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		padding: 0;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--text-muted, #888);
		cursor: pointer;
		opacity: 0;
		transform: translateY(-50%);
		transition:
			opacity 120ms ease,
			background-color 120ms ease,
			color 120ms ease;
	}

	.tab-shell:hover .tab-close,
	.tab-shell.active .tab-close,
	.tab-close:focus-visible {
		opacity: 1;
	}

	.tab-close:hover,
	.tab-close:focus-visible {
		background: rgba(255, 255, 255, 0.08);
		color: var(--text-primary, #e0e0e0);
		outline: none;
	}
</style>
