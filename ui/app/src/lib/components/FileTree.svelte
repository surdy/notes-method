<script lang="ts">
	import FileTree from './FileTree.svelte';
	import { createNote, type NoteSummary } from '$lib/api';
	import { createOrOpenFolderNote, folderNotePath } from '$lib/folder-notes';
	import { noteIcon } from '$lib/note-icons';
	import { tabStore } from '$lib/tab-store.svelte';
	import { toastStore } from '$lib/toast-store.svelte';
	import type { FolderNode } from '$lib/tree-builder';
	import { vaultStore } from '$lib/stores.svelte';

	let { node, depth = 0 }: { node: FolderNode; depth?: number } = $props();
	let expanded = $state(false);
	let seeded = false;
	let contextMenu = $state<{ x: number; y: number } | null>(null);

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

	function openFolderNoteFromMenu(note: NoteSummary | undefined) {
		contextMenu = null;
		openFolderNote(note);
	}

	function handleFolderContextMenu(event: MouseEvent) {
		if (!folderNotePath(node.path)) return;
		event.preventDefault();
		contextMenu = { x: event.clientX, y: event.clientY };
	}

	async function createFolderNoteFromMenu() {
		contextMenu = null;
		if (!vaultStore.currentVault) {
			toastStore.add('Select a vault first.', 'warning');
			return;
		}

		try {
			const result = await createOrOpenFolderNote({
				vault: vaultStore.currentVault,
				folderPath: node.path,
				notes: vaultStore.notes,
				createNote
			});
			if (!result.created) {
				toastStore.add('Folder note already exists.', 'success');
			}
			await vaultStore.loadNotes();
			tabStore.selectNote(result.path);
		} catch (cause) {
			console.error('Failed to create folder note', cause);
			toastStore.add('Failed to create folder note.', 'error');
		}
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
				oncontextmenu={handleFolderContextMenu}
				aria-label={`${expanded ? 'Collapse' : 'Expand'} ${node.name}`}
				aria-expanded={expanded}
			>
				<span class="disclosure" class:open={expanded}>▸</span>
			</button>
			<button
				class="folder-name-button"
				type="button"
				onclick={() => openFolderNote(node.folderNote)}
				oncontextmenu={handleFolderContextMenu}
				title={node.folderNote.path}
			>
				<span class="folder-name">{node.name}</span>
			</button>
		{:else}
		<button class="folder-toggle" type="button" onclick={toggle} oncontextmenu={handleFolderContextMenu}>
			<span class="disclosure" class:open={expanded}>▸</span>
			<span class="folder-name">{node.name}</span>
		</button>
		{/if}
	</div>
	{#if contextMenu}
		<div
			class="folder-context-menu"
			role="menu"
			tabindex="-1"
			style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px`}
			onmouseleave={() => (contextMenu = null)}
		>
			{#if node.folderNote}
				<button type="button" role="menuitem" onclick={() => openFolderNoteFromMenu(node.folderNote)}>
					Open Folder Note
				</button>
			{:else}
				<button type="button" role="menuitem" onclick={createFolderNoteFromMenu}>Create Folder Note</button>
			{/if}
		</div>
	{/if}
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

	.folder-context-menu {
		position: fixed;
		z-index: 1000;
		min-width: 160px;
		padding: 4px;
		border: 1px solid var(--ns-border);
		border-radius: 6px;
		background: var(--ns-surface-elevated);
		box-shadow: var(--ns-shadow-soft);
	}

	.folder-context-menu button {
		display: block;
		width: 100%;
		padding: 6px 8px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--ns-text);
		cursor: pointer;
		text-align: left;
	}

	.folder-context-menu button:hover {
		background: var(--ns-surface-hover);
	}
</style>
