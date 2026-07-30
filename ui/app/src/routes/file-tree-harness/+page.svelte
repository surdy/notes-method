<script lang="ts">
	import { onMount } from 'svelte';
	import FileTree from '$lib/components/FileTree.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { buildTree } from '$lib/tree-builder';
	import type { NoteSummary } from '$lib/api';

	const notes: NoteSummary[] = [
		{
			path: 'Projects/Quill Rail.md',
			title: 'Quill Rail',
			tags: [],
			frontmatter: null
		},
		{
			path: 'Projects/Custom Icon.md',
			title: 'Custom Icon',
			tags: [],
			frontmatter: { _icon: '🔬' }
		},
		{
			path: 'Projects/Research/Structure.md',
			title: 'Structure',
			tags: [],
			frontmatter: null
		}
	];

	const tree = buildTree(notes);

	onMount(() => {
		vaultStore.currentVault = 'harness';
		vaultStore.notes = notes;
	});
</script>

<div class="harness">
	<FileTree node={tree} />
</div>

<style>
	.harness {
		width: 300px;
		min-height: 100vh;
		padding: var(--space-3);
		/* Matches the real sidebar surface, so guide and marker contrast in
		   the harness reads the same as it does in the app. */
		background: var(--bg-secondary);
	}
</style>
