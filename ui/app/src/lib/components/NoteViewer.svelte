<script lang="ts">
	import { getNote, getNoteHtml } from '$lib/api';
	import { vaultStore } from '$lib/stores.svelte';

	let html = $state('');
	let frontmatter = $state<Record<string, unknown>>({});
	let loading = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		const path = vaultStore.selectedPath;
		if (path) {
			void loadNote(path);
		} else {
			html = '';
			frontmatter = {};
		}
	});

	async function loadNote(path: string) {
		loading = true;
		error = null;

		try {
			const [htmlRes, noteRes] = await Promise.all([
				getNoteHtml(vaultStore.currentVault, path),
				getNote(vaultStore.currentVault, path)
			]);
			if (vaultStore.selectedPath !== path) return;

			html = htmlRes;
			frontmatter = noteRes.frontmatter ?? {};
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Failed to load note';
		} finally {
			if (vaultStore.selectedPath === path) {
				loading = false;
			}
		}
	}

	function handleClick(event: MouseEvent) {
		const target = event.target as HTMLElement;
		const link = target.closest('a.wikilink') as HTMLAnchorElement | null;
		if (!link) return;

		event.preventDefault();
		const noteTarget = link.dataset.target;
		if (!noteTarget) return;

		const match = vaultStore.notes.find(
			(note) =>
				note.path.includes(noteTarget) ||
				note.title === noteTarget ||
				note.path.endsWith(`${noteTarget}.md`)
		);
		if (match) {
			vaultStore.selectNote(match.path);
		}
	}
</script>

<div class="note-viewer">
	{#if !vaultStore.selectedPath}
		<div class="empty-state">
			<p>Select a note from the sidebar to view it</p>
		</div>
	{:else if loading}
		<div class="loading">Loading...</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else}
		{#if Object.keys(frontmatter).length > 0}
			<div class="frontmatter">
				<table>
					<tbody>
						{#each Object.entries(frontmatter) as [key, value] (key)}
							<tr>
								<td class="fm-key">{key}</td>
								<td class="fm-value">
									{typeof value === 'object' ? JSON.stringify(value) : String(value)}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}

		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="content" onclick={handleClick}>
			{@html html}
		</div>
	{/if}
</div>

<style>
	.note-viewer {
		flex: 1;
		overflow-y: auto;
		padding: 24px 32px;
		color: var(--text-primary, #e0e0e0);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--text-muted, #888);
	}

	.frontmatter {
		background: var(--surface-bg, #1e1e1e);
		border: 1px solid var(--border-color, #333);
		border-radius: 6px;
		padding: 12px 16px;
		margin-bottom: 20px;
		font-size: 13px;
	}

	.frontmatter table {
		width: 100%;
		border-collapse: collapse;
	}

	.fm-key {
		font-weight: 600;
		padding: 2px 12px 2px 0;
		color: var(--text-accent, #7ec8e3);
		white-space: nowrap;
	}

	.fm-value {
		padding: 2px 0;
	}

	.content :global(h1) {
		font-size: 1.8em;
		margin: 0.5em 0;
	}

	.content :global(h2) {
		font-size: 1.4em;
		margin: 0.8em 0 0.4em;
	}

	.content :global(h3) {
		font-size: 1.15em;
		margin: 0.6em 0 0.3em;
	}

	.content :global(a.wikilink) {
		color: var(--link-color, #7ec8e3);
		cursor: pointer;
		text-decoration: underline;
		text-decoration-style: dotted;
	}

	.content :global(table) {
		border-collapse: collapse;
		margin: 1em 0;
	}

	.content :global(th),
	.content :global(td) {
		border: 1px solid var(--border-color, #444);
		padding: 6px 12px;
	}

	.content :global(blockquote) {
		border-left: 3px solid var(--border-color, #444);
		margin: 1em 0;
		padding: 0.5em 1em;
		color: var(--text-muted, #aaa);
	}

	.content :global(.callout) {
		border-radius: 6px;
		padding: 12px 16px;
		margin: 1em 0;
		border-left: 4px solid;
	}

	.content :global(.callout-info) {
		border-color: #4a9eff;
		background: #1a3a5c;
	}

	.content :global(.callout-warning) {
		border-color: #ffb347;
		background: #3d3018;
	}

	.content :global(.callout-tip) {
		border-color: #50c878;
		background: #1a3d2a;
	}

	.loading,
	.error {
		padding: 20px;
		text-align: center;
	}

	.error {
		color: #ff6b6b;
	}
</style>
