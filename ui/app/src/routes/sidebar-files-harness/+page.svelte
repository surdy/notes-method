<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import InputPalette from '$lib/components/InputPalette.svelte';
	import SidebarViews from '$lib/components/SidebarViews.svelte';
	import ToastStack from '$lib/components/ToastStack.svelte';
	import { inputPalette } from '$lib/input-palette.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import type { NoteSummary } from '$lib/api';

	const notes: NoteSummary[] = [
		{
			path: 'Projects/Quill Rail.md',
			title: 'Quill Rail',
			tags: [],
			frontmatter: null
		}
	];
	let ready = $state(false);

	onMount(() => {
		vaultStore.currentVault = 'harness';
		vaultStore.notes = notes;
		ready = true;
	});

	onDestroy(() => {
		vaultStore.currentVault = '';
		vaultStore.notes = [];
	});
</script>

<div class="harness">
	{#if ready}
		<SidebarViews />
	{/if}
</div>
{#if inputPalette.request}
	<InputPalette />
{/if}
<ToastStack />

<style>
	.harness {
		width: 320px;
		height: 640px;
		display: flex;
		background: var(--bg-panel);
	}
</style>
