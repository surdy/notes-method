<script lang="ts">
	import { getNote, getNoteHtml, toggleTaskStatus, type NoteTask, type TaskMutationStatus } from '$lib/api';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let html = $state('');
	let frontmatter = $state<Record<string, unknown>>({});
	let tasks = $state<NoteTask[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);

	interface Props {
		path: string | null;
	}

	let { path }: Props = $props();

	$effect(() => {
		if (path) {
			void loadNote(path);
		} else {
			html = '';
			frontmatter = {};
			tasks = [];
		}
	});

	async function loadNote(notePath: string) {
		loading = true;
		error = null;

		try {
			const [htmlRes, noteRes] = await Promise.all([
				getNoteHtml(vaultStore.currentVault, notePath),
				getNote(vaultStore.currentVault, notePath)
			]);
			if (path !== notePath) return;

			html = htmlRes;
			frontmatter = noteRes.frontmatter ?? {};
			tasks = noteRes.tasks ?? [];
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Failed to load note';
		} finally {
			if (path === notePath) {
				loading = false;
			}
		}
	}

	function handleClick(event: MouseEvent) {
		const target = event.target as HTMLElement;

		// Handle task checkbox clicks
		if (target instanceof HTMLInputElement && target.type === 'checkbox') {
			event.preventDefault();
			void handleTaskToggle(target);
			return;
		}

		// Handle wikilink clicks
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
			tabStore.selectNote(match.path);
		}
	}

	async function handleTaskToggle(checkbox: HTMLInputElement) {
		if (!path) return;

		// Find this checkbox's index among all checkboxes in the rendered HTML
		const container = checkbox.closest('.content');
		if (!container) return;

		const allCheckboxes = Array.from(container.querySelectorAll('input[type="checkbox"]'));
		const checkboxIndex = allCheckboxes.indexOf(checkbox);
		if (checkboxIndex < 0 || checkboxIndex >= tasks.length) return;

		const task = tasks[checkboxIndex];
		if (!task?.content_hash) return;

		const newStatus: TaskMutationStatus = task.status === 'done' ? 'todo' : 'done';

		try {
			await toggleTaskStatus(vaultStore.currentVault, path, task.content_hash, newStatus);
			await loadNote(path);
		} catch (cause) {
			console.error('Failed to toggle task', cause);
		}
	}
</script>

<div class="note-viewer">
	{#if !path}
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
		color: var(--ns-text);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--ns-text-muted);
	}

	.frontmatter {
		background: var(--ns-surface);
		border: 1px solid var(--ns-border);
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
		color: var(--ns-accent);
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
		color: var(--ns-link);
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
		border: 1px solid var(--ns-border-strong);
		padding: 6px 12px;
	}

	.content :global(blockquote) {
		border-left: 3px solid var(--ns-border-strong);
		margin: 1em 0;
		padding: 0.5em 1em;
		color: var(--ns-text-faint);
	}

	.content :global(.callout) {
		border-radius: 6px;
		padding: 12px 16px;
		margin: 1em 0;
		border-left: 4px solid;
	}

	.content :global(.callout-info) {
		border-color: var(--ns-info);
		background: var(--ns-info-bg);
	}

	.content :global(.callout-warning) {
		border-color: var(--ns-warning-callout-border);
		background: var(--ns-warning-callout-bg);
	}

	.content :global(.callout-tip) {
		border-color: var(--ns-success);
		background: var(--ns-success-bg);
	}

	.content :global(input[type='checkbox']) {
		cursor: pointer;
		pointer-events: auto;
	}

	.content :global(li) {
		color: var(--ns-text);
	}

	.loading,
	.error {
		padding: 20px;
		text-align: center;
	}

	.error {
		color: var(--ns-danger);
	}
</style>
