<script lang="ts">
	import FileTree from './FileTree.svelte';
	import { createNote, renameFolder, type NoteSummary } from '$lib/api';
	import {
		createOrOpenFolderNote,
		folderNotePath,
		isFolderNoteSelected,
		remapPathAfterFolderRename
	} from '$lib/folder-notes';
	import { inputPalette } from '$lib/input-palette.svelte';
	import { configuredNoteIcon } from '$lib/note-icons';
	import { tabStore } from '$lib/tab-store.svelte';
	import { toastStore } from '$lib/toast-store.svelte';
	import type { FolderNode } from '$lib/tree-builder';
	import { vaultStore } from '$lib/stores.svelte';

	let {
		node,
		depth = 0,
		forceExpand = false
	}: { node: FolderNode; depth?: number; forceExpand?: boolean } = $props();
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

	function renameFolderFromMenu() {
		contextMenu = null;
		if (!vaultStore.currentVault) {
			toastStore.add('Select a vault first.', 'warning');
			return;
		}

		inputPalette.open({
			steps: [
				{
					mode: 'text',
					label: `Rename ${node.name}`,
					placeholder: node.name,
					required: true,
					defaultValue: node.name
				}
			],
			onComplete: async ([name]) => {
				const nextName = name?.trim();
				if (!nextName || nextName === node.name) return;

				try {
					const result = await renameFolder(vaultStore.currentVault, node.path, nextName);
					tabStore.rewritePaths((path) => remapPathAfterFolderRename(path, result));
					await vaultStore.loadNotes();
					toastStore.add('Folder renamed.', 'success');
				} catch (cause) {
					console.error('Failed to rename folder', cause);
					toastStore.add('Failed to rename folder.', 'error');
				}
			}
		});
	}

	function noteTitle(note: NoteSummary): string {
		if (note.title) return note.title;
		const parts = note.path.split('/');
		return parts[parts.length - 1].replace(/\.md$/, '');
	}

	const INDENT = 16;
</script>

{#if node.name}
	<div
		class="folder"
		class:selected={isFolderNoteSelected(node, tabStore.selectedPath)}
		style={`padding-left: ${depth * INDENT}px`}
	>
		<!-- indent guides -->
		{#each Array(depth) as _, i}
			<span class="indent-guide" style={`left: ${i * INDENT + 11}px`}></span>
		{/each}
		{#if node.folderNote}
			<button
				class="folder-disclosure-button"
				class:selected={isFolderNoteSelected(node, tabStore.selectedPath)}
				type="button"
				onclick={toggle}
				oncontextmenu={handleFolderContextMenu}
				aria-label={`${expanded ? 'Collapse' : 'Expand'} ${node.name}`}
				aria-expanded={expanded || forceExpand}
			>
				<span class="disclosure" class:open={expanded || forceExpand}></span>
			</button>
			<button
				class="folder-name-button"
				class:selected={isFolderNoteSelected(node, tabStore.selectedPath)}
				type="button"
				onclick={() => openFolderNote(node.folderNote)}
				oncontextmenu={handleFolderContextMenu}
				title={node.folderNote.path}
			>
				<span class="folder-name">{node.name}</span>
			</button>
		{:else}
		<button class="folder-toggle" type="button" onclick={toggle} oncontextmenu={handleFolderContextMenu} aria-expanded={expanded || forceExpand}>
			<span class="disclosure" class:open={expanded || forceExpand}></span>
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
			<button type="button" role="menuitem" onclick={renameFolderFromMenu}>Rename Folder</button>
		</div>
	{/if}
{/if}

{#if expanded || forceExpand || !node.name}
	{#each node.children as child (child.path)}
		<FileTree node={child} depth={depth + 1} {forceExpand} />
	{/each}

	{#each node.notes as note (note.path)}
		{@const icon = configuredNoteIcon(note)}
		<button
			class="note-item"
			class:selected={tabStore.selectedPath === note.path}
			style={`padding-left: ${(depth + 1) * INDENT + 16}px`}
			onclick={() => selectNote(note)}
		>
			<!-- indent guides -->
			{#each Array(depth + 1) as _, i}
				<span class="indent-guide" style={`left: ${i * INDENT + 11}px`}></span>
			{/each}
			{#if icon}
				<span class="note-icon">{icon}</span>
			{/if}
			<span class="note-title">{noteTitle(note)}</span>
		</button>
	{/each}
{/if}

<style>
	.folder {
		position: relative;
		display: flex;
		align-items: center;
		border-radius: var(--radius-sm);
	}

	.folder:hover {
		background: var(--bg-hover);
	}

	.folder.selected {
		background: var(--bg-selected);
		box-shadow: inset 2px 0 0 var(--accent);
	}

	.folder-toggle,
	.folder-disclosure-button,
	.folder-name-button {
		display: flex;
		align-items: center;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		cursor: pointer;
		font-size: 13px;
		font-weight: 500;
		text-align: left;
		color: var(--text-secondary);
	}

	.folder-toggle {
		gap: 0;
		width: 100%;
		padding: 5px 8px 5px 0;
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

	.folder-disclosure-button.selected,
	.folder-name-button.selected {
		background: transparent;
		color: var(--text-default);
	}

	.folder-disclosure-button.selected {
		box-shadow: none;
	}

	.disclosure {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		transition: transform 0.15s ease;
		flex-shrink: 0;
	}

	/* Drawn with CSS borders rather than a Unicode glyph so the triangle
	   renders consistently across platform font stacks. */
	.disclosure::before {
		content: '';
		width: 0;
		height: 0;
		border-top: 4px solid transparent;
		border-bottom: 4px solid transparent;
		border-left: 5px solid var(--text-muted);
	}

	/* The disclosure fills the 16px icon gutter immediately to the left of
	   the folder name. This keeps a folder's name aligned with a sibling
	   note's title at the same depth while the chevron hugs the name,
	   instead of floating out in the parent's indent channel. */
	.folder-toggle .disclosure {
		flex: 0 0 16px;
	}

	.disclosure.open {
		transform: rotate(90deg);
	}

	.indent-guide {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 1px;
		background: var(--border-subtle);
		pointer-events: none;
	}

	.note-item {
		position: relative;
		display: flex;
		align-items: center;
		gap: 0;
		width: 100%;
		padding: 5px 8px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		cursor: pointer;
		font-size: 13px;
		text-align: left;
		color: var(--text-default);
	}

	.note-item:hover {
		background: var(--bg-hover);
	}

	.note-item.selected {
		background: var(--bg-selected);
		color: var(--text-default);
		font-weight: 600;
		box-shadow: inset 2px 0 0 var(--accent);
	}

	.note-icon {
		width: 16px;
		margin-left: -24px;
		margin-right: var(--space-2);
		flex: 0 0 16px;
		text-align: center;
	}

	.folder-context-menu {
		position: fixed;
		z-index: 1000;
		min-width: 160px;
		padding: 4px;
		border: 1px solid var(--border-default);
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		box-shadow: var(--shadow-pop);
	}

	.folder-context-menu button {
		display: block;
		width: 100%;
		padding: 6px 8px;
		border: none;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--text-default);
		cursor: pointer;
		text-align: left;
	}

	.folder-context-menu button:hover {
		background: var(--bg-hover);
	}
</style>
