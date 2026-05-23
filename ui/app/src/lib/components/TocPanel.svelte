<script lang="ts">
	import { headingStore } from '$lib/heading-store.svelte';

	let { onScrollTo }: { onScrollTo: (from: number) => void } = $props();

	let headings = $derived(headingStore.headings);
	let activeIndex = $derived(headingStore.activeIndex);
</script>

{#if headings.length === 0}
	<div class="toc-empty">No headings found</div>
{:else}
	<nav class="toc-list" aria-label="Table of contents">
		{#each headings as heading, index (heading.from)}
			<button
				class="toc-item"
				class:active={index === activeIndex}
				style={`padding-left: ${(heading.level - 1) * 16 + 12}px; font-size: ${heading.level <= 2 ? 13 : 12}px;`}
				type="button"
				onclick={() => onScrollTo(heading.from)}
			>
				{heading.text}
			</button>
		{/each}
	</nav>
{/if}

<style>
	.toc-empty {
		padding: 12px 16px;
		color: var(--text-muted);
		font-size: 12px;
	}

	.toc-list {
		display: flex;
		flex-direction: column;
		padding: 8px 0;
	}

	.toc-item {
		width: 100%;
		padding-top: 8px;
		padding-right: 12px;
		padding-bottom: 8px;
		border: none;
		border-left: 2px solid transparent;
		background: transparent;
		color: var(--text-muted);
		text-align: left;
		cursor: pointer;
	}

	.toc-item:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.toc-item.active {
		border-left-color: var(--accent);
		color: var(--accent);
		background: var(--accent-bg);
	}
 </style>
