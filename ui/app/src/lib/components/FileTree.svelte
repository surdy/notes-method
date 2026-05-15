<script lang="ts">
	import FileTree from './FileTree.svelte';
	import type { NoteSummary } from '$lib/api';
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

	function noteTitle(note: NoteSummary): string {
		if (note.title) return note.title;
		const parts = note.path.split('/');
		return parts[parts.length - 1].replace(/\.md$/, '');
	}

	function typeIcon(type: string): string {
		const icons: Record<string, string> = {
			daily: '📅',
			meeting: '🤝',
			customer: '🏢',
			stream: '🔀',
			note: '📝',
			'account-info': 'ℹ️',
			glossary: '📖',
			milestones: '🏁'
		};
		return icons[type] ?? '📄';
	}
</script>

{#if node.name}
	<div class="folder" style={`padding-left: ${depth * 16}px`}>
		<button class="folder-toggle" onclick={toggle}>
			<span class="folder-icon">{expanded ? '📂' : '📁'}</span>
			<span class="folder-name">{node.name}</span>
		</button>
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
			style={`padding-left: ${(depth + 1) * 16}px`}
			onclick={() => selectNote(note)}
		>
			<span class="note-icon">{typeIcon(note.type)}</span>
			<span class="note-title">{noteTitle(note)}</span>
		</button>
	{/each}
{/if}

<style>
	.folder-toggle {
		display: flex;
		align-items: center;
		gap: 4px;
		width: 100%;
		padding: 4px 8px;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 14px;
		text-align: left;
		color: var(--ns-text);
	}

	.folder-toggle:hover {
		background: var(--ns-surface-hover);
	}

	.note-item {
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
