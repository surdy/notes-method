<script lang="ts">
	import { sortSources, type NoteSource } from '$lib/agent/sources';
	import { tabStore } from '$lib/tab-store.svelte';

	let { sources }: { sources: NoteSource[] } = $props();

	let expanded = $state(false);
	let ordered = $derived(sortSources(sources));

	function label(source: NoteSource): string {
		return source.title && source.title.length > 0 ? source.title : source.path;
	}

	/** Build the ranking-explanation string from whichever signals exist. */
	function explanation(source: NoteSource): string {
		const parts: string[] = [];
		if (source.lexicalRank !== null) parts.push(`lexical #${source.lexicalRank}`);
		if (source.semanticRank !== null) parts.push(`semantic #${source.semanticRank}`);
		if (source.score !== null) parts.push(`score ${source.score.toFixed(3)}`);
		return parts.join(' · ');
	}

	function open(source: NoteSource) {
		tabStore.selectNote(source.path);
	}
</script>

<div class="sources">
	<button
		type="button"
		class="toggle"
		aria-expanded={expanded}
		onclick={() => (expanded = !expanded)}
		title="Vault notes that grounded this answer"
	>
		<svg
			xmlns="http://www.w3.org/2000/svg"
			width="13"
			height="13"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			stroke-width="2"
			stroke-linecap="round"
			stroke-linejoin="round"
			aria-hidden="true"
		>
			<path
				d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"
			/>
			<path d="M14 2v5a1 1 0 0 0 1 1h5" />
		</svg>
		<span>{expanded ? 'Hide sources' : 'Show sources'} ({ordered.length})</span>
	</button>

	{#if expanded}
		<ul class="list">
			{#each ordered as source (source.path)}
				<li>
					<button type="button" class="source" onclick={() => open(source)} title={source.path}>
						<span class="name">{label(source)}</span>
						{#if explanation(source)}
							<span class="meta">{explanation(source)}</span>
						{/if}
						{#if source.snippet}
							<span class="snippet">{source.snippet}</span>
						{/if}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.sources {
		margin-top: 4px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.toggle {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		align-self: flex-start;
		padding: 3px 8px;
		border-radius: 999px;
		border: 1px solid var(--border-default);
		background: var(--bg-secondary);
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 600;
		cursor: pointer;
	}

	.toggle:hover,
	.toggle:focus-visible {
		color: var(--text-default);
		border-color: var(--accent);
		outline: none;
	}

	.toggle svg {
		display: block;
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.source {
		display: flex;
		flex-direction: column;
		gap: 2px;
		width: 100%;
		text-align: left;
		padding: 6px 8px;
		border-radius: 8px;
		border: 1px solid var(--border-default);
		background: var(--bg-surface);
		color: var(--text-default);
		cursor: pointer;
	}

	.source:hover,
	.source:focus-visible {
		border-color: var(--accent);
		background: var(--bg-secondary);
		outline: none;
	}

	.name {
		font-size: 12px;
		font-weight: 600;
	}

	.meta {
		font-size: 10px;
		font-family: var(--font-mono);
		color: var(--text-muted);
	}

	.snippet {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.4;
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
	}
</style>
