<script lang="ts">
	import FileTree from './FileTree.svelte';
	import type { NoteSummary } from '$lib/api';
	import { noteIcon } from '$lib/note-icons';
	import { tabStore } from '$lib/tab-store.svelte';
	import type { FolderNode } from '$lib/tree-builder';

	let { node, depth = 0 }: { node: FolderNode; depth?: number } = $props();
	let expanded = $state(false);
	let seeded = false;

	$effect(() => {
		if (seeded) return;
		expanded = depth < 2;
		seeded = true;
	});

	function toggle() {
		expanded = !expanded;
	}

	function selectNote(note: NoteSummary) {
		tabStore.selectNote(note.path);
	}

	function openFolderNote(note: NoteSummary | undefined) {
		if (!note) return;
		selectNote(note);
	}

	function noteTitle(note: NoteSummary): string {
		if (note.title) return note.title;
		const parts = note.path.split('/');
		return parts[parts.length - 1].replace(/\.md$/, '');
	}

	const INDENT = 16;
</script>

{#if node.name}
	<div class="folder" style={`padding-left: ${depth * INDENT}px`}>
		<!-- indent guides -->
		{#each Array(depth) as _, i}
			<span class="indent-guide" style={`left: ${i * INDENT + 11}px`}></span>
		{/each}
		{#if node.folderNote}
			<button
				class="folder-disclosure-button"
				type="button"
				onclick={toggle}
				aria-label={`${expanded ? 'Collapse' : 'Expand'} ${node.name}`}
				aria-expanded={expanded}
			>
				<span class="disclosure" class:open={expanded}>▸</span>
			</button>
			<button
				class="folder-name-button"
				type="button"
				onclick={() => openFolderNote(node.folderNote)}
				title={node.folderNote.path}
			>
				<span class="folder-name">{node.name}</span>
			</button>
		{:else}
		<button class="folder-toggle" onclick={toggle}>
			<span class="disclosure" class:open={expanded}>▸</span>
			<span class="folder-name">{node.name}</span>
		</button>
		{/if}
	</div>
{/if}

{#if expanded || !node.name}
	{#each node.children as child (child.path)}
		<FileTree node={child} depth={depth + 1} />
	{/each}

	{#each node.notes as note (note.path)}
		<button
			class="note-item"
			class:selected={tabStore.selectedPath === note.path}
			style={`padding-left: ${(depth + 1) * INDENT}px`}
			onclick={() => selectNote(note)}
		>
			<!-- indent guides -->
			{#each Array(depth + 1) as _, i}
				<span class="indent-guide" style={`left: ${i * INDENT + 11}px`}></span>
			{/each}
			<span class="note-icon">{noteIcon(note)}</span>
			<span class="note-title">{noteTitle(note)}</span>
		</button>
	{/each}
{/if}

<style>
	.folder {
		position: relative;
		display: flex;
		align-items: center;
	}

	.folder-toggle,
	.folder-disclosure-button,
	.folder-name-button {
		display: flex;
		align-items: center;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 14px;
		text-align: left;
		color: var(--ns-text);
	}

	.folder-toggle {
		gap: 4px;
		width: 100%;
		padding: 4px 8px;
	}

	.folder-disclosure-button {
		justify-content: center;
		padding: 4px 0 4px 8px;
		flex: 0 0 24px;
	}

	.folder-name-button {
		flex: 1;
		min-width: 0;
		padding: 4px 8px 4px 0;
	}

	.folder-toggle:hover,
	.folder-disclosure-button:hover,
	.folder-name-button:hover {
		background: var(--ns-surface-hover);
	}

	.disclosure {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		font-size: 10px;
		color: var(--ns-text-muted);
		transition: transform 0.15s ease;
		flex-shrink: 0;
	}

	.disclosure.open {
		transform: rotate(90deg);
	}

	.indent-guide {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 1px;
		background: var(--ns-border);
		pointer-events: none;
	}

	.note-item {
		position: relative;
		display: flex;
		align-items: center;
		gap: 4px;
		width: 100%;
		padding: 3px 8px;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 13px;
		text-align: left;
		color: var(--ns-text-secondary);
	}

	.note-item:hover {
		background: var(--ns-surface-hover);
	}

	.note-item.selected {
		background: var(--ns-selected-bg);
		color: var(--ns-text-inverse);
	}
</style>
