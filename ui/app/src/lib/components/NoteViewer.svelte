<script lang="ts">
	import { tick } from 'svelte';
	import { getNote, getNoteHtml, toggleTaskStatus, type NoteTask, type TaskMutationStatus } from '$lib/api';
	import { applySyntaxHighlighting } from '$lib/editor/code-highlighting';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let html = $state('');
	let frontmatter = $state<Record<string, unknown>>({});
	let tasks = $state<NoteTask[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let contentElement = $state<HTMLElement | null>(null);

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

	$effect(() => {
		const element = contentElement;
		const renderedHtml = html;
		if (!element || !renderedHtml) return;

		void tick()
			.then(() => {
				if (contentElement === element && html === renderedHtml) {
					return applySyntaxHighlighting(element);
				}
			})
			.catch((cause) => {
				console.error('Failed to highlight reading view code blocks', cause);
			});
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

		const calloutTitle = target.closest('.callout[data-fold] .callout-title') as HTMLElement | null;
		if (calloutTitle) {
			const callout = calloutTitle.closest('.callout') as HTMLElement | null;
			if (callout) {
				event.preventDefault();
				callout.dataset.fold = callout.dataset.fold === 'closed' ? 'open' : 'closed';
			}
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
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="content" bind:this={contentElement} onclick={handleClick}>
			{@html html}
		</div>
	{/if}
</div>

<style>
	.note-viewer {
		flex: 1;
		overflow-y: auto;
		padding: 24px 32px;
		background: var(--ns-editor-bg);
		color: var(--ns-editor-text);
		line-height: var(--ns-line-height-normal);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--ns-editor-text-muted);
	}

	.content :global(p) {
		margin: 0 0 var(--ns-paragraph-spacing);
	}

	.content :global(p:last-child) {
		margin-bottom: 0;
	}

	.content :global(ul),
	.content :global(ol) {
		margin: 0 0 var(--ns-paragraph-spacing);
		padding-left: 1.5em;
	}

	.content :global(li) > :global(p) {
		margin: 0;
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
		color: var(--ns-editor-link);
		cursor: pointer;
		text-decoration: underline;
		text-decoration-style: dotted;
	}

	/* External links (not wikilinks): add an arrow icon to distinguish from
	   internal vault links. Matches common URL schemes plus protocol-relative. */
	.content :global(a[href^='http://']:not(.wikilink))::after,
	.content :global(a[href^='https://']:not(.wikilink))::after,
	.content :global(a[href^='//']:not(.wikilink))::after,
	.content :global(a[href^='mailto:']:not(.wikilink))::after,
	.content :global(a[href^='tel:']:not(.wikilink))::after,
	.content :global(a[href^='ftp://']:not(.wikilink))::after,
	.content :global(a[href^='obsidian://']:not(.wikilink))::after,
	.content :global(a[href^='notesmith://']:not(.wikilink))::after {
		content: '↗';
		display: inline-block;
		margin-left: 0.15em;
		font-size: 0.85em;
		vertical-align: baseline;
		opacity: 0.7;
	}

	.content :global(table) {
		border-collapse: collapse;
		margin: 1em 0;
	}

	.content :global(th),
	.content :global(td) {
		border: 1px solid var(--ns-editor-border);
		padding: 6px 12px;
	}

	.content :global(blockquote) {
		border-left: 3px solid var(--ns-editor-border);
		margin: 1em 0;
		padding: 0.5em 1em;
		color: var(--ns-editor-text-faint);
	}

	.content :global(code) {
		padding: 0.15em 0.35em;
		border-radius: 4px;
		background: var(--ns-panel-bg-strong);
		font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
		font-size: 0.9em;
	}

	.content :global(pre) {
		margin: 1em 0;
		padding: 1em;
		border: 1px solid var(--ns-editor-border);
		border-radius: 8px;
		background: var(--ns-panel-bg-strong);
		overflow-x: auto;
	}

	.content :global(pre code) {
		padding: 0;
		background: transparent;
	}

	.content :global(.callout) {
		--ns-callout-current: var(--ns-callout-note);
		--ns-callout-icon: '✎';
		border: 1px solid color-mix(in srgb, var(--ns-callout-current) 42%, transparent);
		border-left: 4px solid var(--ns-callout-current);
		border-radius: 8px;
		padding: 12px 16px;
		margin: 1em 0;
		background: color-mix(in srgb, var(--ns-callout-current) 13%, var(--ns-editor-bg));
		color: var(--ns-editor-text);
	}

	.content :global(.callout-title) {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--ns-callout-current);
		font-weight: 700;
	}

	.content :global(.callout-title::before) {
		content: var(--ns-callout-icon);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		flex: 0 0 18px;
	}

	.content :global(.callout[data-fold] .callout-title) {
		cursor: pointer;
	}

	.content :global(.callout[data-fold] .callout-title::after) {
		content: '⌄';
		margin-left: auto;
		color: var(--ns-editor-text-muted);
	}

	.content :global(.callout[data-fold='closed'] .callout-title::after) {
		content: '›';
	}

	.content :global(.callout[data-fold='closed'] .callout-body) {
		display: none;
	}

	.content :global(.callout-body > :first-child) {
		margin-top: 0;
	}

	.content :global(.callout-body > :last-child) {
		margin-bottom: 0;
	}

	.content :global(.callout-note) {
		--ns-callout-current: var(--ns-callout-note);
		--ns-callout-icon: '✎';
	}

	.content :global(.callout-abstract) {
		--ns-callout-current: var(--ns-callout-abstract);
		--ns-callout-icon: '☷';
	}

	.content :global(.callout-info) {
		--ns-callout-current: var(--ns-callout-info);
		--ns-callout-icon: 'ⓘ';
	}

	.content :global(.callout-todo) {
		--ns-callout-current: var(--ns-callout-todo);
		--ns-callout-icon: '☑';
	}

	.content :global(.callout-tip) {
		--ns-callout-current: var(--ns-callout-tip);
		--ns-callout-icon: '🔥';
	}

	.content :global(.callout-success) {
		--ns-callout-current: var(--ns-callout-success);
		--ns-callout-icon: '✓';
	}

	.content :global(.callout-question) {
		--ns-callout-current: var(--ns-callout-question);
		--ns-callout-icon: '?';
	}

	.content :global(.callout-warning) {
		--ns-callout-current: var(--ns-callout-warning);
		--ns-callout-icon: '⚠';
	}

	.content :global(.callout-failure) {
		--ns-callout-current: var(--ns-callout-failure);
		--ns-callout-icon: '✕';
	}

	.content :global(.callout-danger) {
		--ns-callout-current: var(--ns-callout-danger);
		--ns-callout-icon: '⚡';
	}

	.content :global(.callout-bug) {
		--ns-callout-current: var(--ns-callout-bug);
		--ns-callout-icon: '◉';
	}

	.content :global(.callout-example) {
		--ns-callout-current: var(--ns-callout-example);
		--ns-callout-icon: '▦';
	}

	.content :global(.callout-quote) {
		--ns-callout-current: var(--ns-callout-quote);
		--ns-callout-icon: '❝';
	}

	.content :global(input[type='checkbox']) {
		cursor: pointer;
		pointer-events: auto;
	}

	.content :global(li) {
		color: var(--ns-editor-text);
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
