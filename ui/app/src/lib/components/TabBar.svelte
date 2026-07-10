<script lang="ts">
	import { noteIcon } from '$lib/note-icons';
	import { vaultStore } from '$lib/stores.svelte';
	import { tabStore, type Tab } from '$lib/tab-store.svelte';

	let dragIndex = $state<number | null>(null);
	let dropIndex = $state<number | null>(null);
	let contextMenu = $state<{ x: number; y: number; index: number } | null>(null);
	let tabs = $derived(tabStore.tabs as Tab[]);
	let notesByPath = $derived.by(
		() => new Map(vaultStore.notes.map((note) => [note.path, note] as const))
	);

	function handleClose(event: MouseEvent, index: number) {
		event.stopPropagation();
		tabStore.closeTab(index);
	}

	function handleContextMenu(event: MouseEvent, index: number) {
		event.preventDefault();
		contextMenu = { x: event.clientX, y: event.clientY, index };
	}

	function closeFromMenu() {
		if (contextMenu) {
			tabStore.closeTab(contextMenu.index);
		}
		contextMenu = null;
	}

	function closeOthersFromMenu() {
		if (contextMenu) {
			tabStore.closeOtherTabs(contextMenu.index);
		}
		contextMenu = null;
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
			{@const note = notesByPath.get(tab.path)}
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
					oncontextmenu={(event) => handleContextMenu(event, index)}
				>
					<span class="tab-title">
						<span class="tab-icon">{note ? noteIcon(note) : '📄'}</span>
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
	{#if contextMenu}
		<div
			class="tab-context-menu"
			role="menu"
			tabindex="-1"
			style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px`}
			onmouseleave={() => (contextMenu = null)}
		>
			<button type="button" role="menuitem" onclick={closeFromMenu}>Close</button>
			<button
				type="button"
				role="menuitem"
				onclick={closeOthersFromMenu}
				disabled={tabs.length <= 1}>Close Others</button
			>
		</div>
	{/if}
{/if}

<style>
	.tab-bar {
		display: flex;
		align-items: stretch;
		flex: 1;
		gap: 4px;
		min-height: 36px;
		padding: 4px 8px 0;
		overflow-x: auto;
		overflow-y: hidden;
		background: transparent;
		scrollbar-width: thin;
	}

	.tab-shell {
		position: relative;
		display: flex;
		align-items: stretch;
		flex: 0 0 auto;
		min-width: 0;
		max-width: 240px;
		overflow: hidden;
		background: var(--bg-elevated);
		border-top: 2px solid transparent;
		border-radius: 10px 10px 0 0;
		transition:
			background-color 120ms ease,
			border-color 120ms ease;
	}

	.tab-shell.active {
		background: var(--editor-bg);
		border-top-color: var(--accent);
	}

	.tab-shell.drag-over::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		border-bottom: 2px solid var(--accent);
	}

	.tab {
		display: flex;
		align-items: center;
		min-width: 0;
		padding: 0 32px 0 12px;
		color: var(--text-default);
		cursor: pointer;
		outline: none;
	}

	.tab:focus-visible {
		box-shadow: inset 0 0 0 1px var(--accent);
	}

	.tab-title {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		font-size: 13px;
	}

	.tab-icon {
		flex-shrink: 0;
		line-height: 1;
	}

	.tab-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.dirty-dot {
		color: var(--dirty-dot);
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
		color: var(--text-muted);
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
		background: var(--surface-translucent);
		color: var(--text-default);
		outline: none;
	}

	.tab-context-menu {
		position: fixed;
		z-index: 1000;
		min-width: 160px;
		padding: 4px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--bg-elevated);
		box-shadow: var(--shadow-soft);
	}

	.tab-context-menu button {
		display: block;
		width: 100%;
		padding: 6px 8px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--text-default);
		cursor: pointer;
		text-align: left;
	}

	.tab-context-menu button:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.tab-context-menu button:disabled {
		color: var(--text-muted);
		cursor: default;
	}
</style>
