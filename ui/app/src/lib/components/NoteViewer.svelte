<script lang="ts">
	import { tick } from 'svelte';
	import { createNote, getNote, getNoteHtml, toggleTaskStatus, type NoteTask, type TaskMutationStatus } from '$lib/api';
	import { applySyntaxHighlighting } from '$lib/editor/code-highlighting';
	import { displayTitleFor } from '$lib/display-title';
	import TitleHeader from '$lib/components/TitleHeader.svelte';
	import WikilinkResolvePopup from '$lib/components/WikilinkResolvePopup.svelte';
	import type { UnresolvedWikilink } from '$lib/editor/ofm-decorations';
	import { resolveWikilink, splitWikilinkTarget } from '$lib/editor/wikilink-resolver';
	import { stripFirstH1IfMatchesTitle } from '$lib/duplicate-h1';
	import { settingsStore } from '$lib/settings.svelte';
	import { tabStore } from '$lib/tab-store.svelte';
	import { toastStore } from '$lib/toast-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let html = $state('');
	let frontmatter = $state<Record<string, unknown>>({});
	let tasks = $state<NoteTask[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let contentElement = $state<HTMLElement | null>(null);
	let wikilinkPopup = $state<UnresolvedWikilink | null>(null);

	interface Props {
		path: string | null;
	}

	let { path }: Props = $props();

	let title = $derived(displayTitleFor({ path: path ?? '', frontmatter }));
	let renderedHtml = $derived(
		(settingsStore.draftConfig?.editor.hide_duplicate_h1 ?? true)
			? stripFirstH1IfMatchesTitle(html, title)
			: html
	);

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

		const resolution = resolveWikilink(noteTarget, vaultStore.notes);
		if (resolution.path) {
			tabStore.selectNote(resolution.path);
			return;
		}
		wikilinkPopup = {
			target: resolution.name,
			candidates: resolution.candidates,
			x: event.clientX,
			y: event.clientY
		};
	}

	// Create a note for a clicked dead wikilink, then open it.
	async function createNoteFromWikilink(target: string) {
		wikilinkPopup = null;
		const vault = vaultStore.currentVault;
		if (!vault) return;
		const { folder, title } = splitWikilinkTarget(target);
		if (!title) return;
		try {
			const created = await createNote(vault, title, '', folder ?? 'Inbox');
			await vaultStore.loadNotes();
			tabStore.selectNote(created.path);
			toastStore.add(`Created “${title}”.`);
		} catch (err) {
			toastStore.add(err instanceof Error ? err.message : 'Failed to create note.', 'error');
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
			<TitleHeader path={path ?? ''} frontmatter={frontmatter} variant="viewer" />
			{@html renderedHtml}
		</div>
	{/if}
</div>

{#if wikilinkPopup}
	<WikilinkResolvePopup
		target={wikilinkPopup.target}
		candidates={wikilinkPopup.candidates}
		x={wikilinkPopup.x}
		y={wikilinkPopup.y}
		onNavigate={(p) => {
			wikilinkPopup = null;
			tabStore.selectNote(p);
		}}
		onCreate={() => createNoteFromWikilink(wikilinkPopup?.target ?? '')}
		onClose={() => (wikilinkPopup = null)}
	/>
{/if}

<style>
	.note-viewer {
		flex: 1;
		overflow-y: auto;
		padding: 40px 32px 72px;
		background: var(--editor-bg);
		color: var(--editor-text);
		line-height: var(--reading-line-height);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--editor-text-muted);
	}

	.content {
		max-width: var(--reading-measure);
		margin: 0 auto;
		font-family: var(--font-reading);
		font-size: var(--reading-font-size);
		line-height: var(--reading-line-height);
	}

	.content :global(p) {
		margin: 0 0 var(--paragraph-spacing);
	}

	.content :global(p:last-child) {
		margin-bottom: 0;
	}

	.content :global(ul),
	.content :global(ol) {
		margin: 0 0 var(--paragraph-spacing);
		padding-left: 1.5em;
	}

	.content :global(li) > :global(p) {
		margin: 0;
	}

	.content :global(h1) {
		font-size: var(--reading-h1-size);
		line-height: var(--reading-h1-line);
		font-weight: var(--reading-h1-weight);
		letter-spacing: var(--reading-h1-tracking);
		margin: 0.4em 0 0.35em;
	}

	.content :global(h2) {
		font-size: var(--reading-h2-size);
		line-height: var(--reading-h2-line);
		margin: 1.4em 0 0.4em;
	}

	.content :global(h3) {
		font-size: var(--reading-h3-size);
		margin: 1.1em 0 0.3em;
	}

	/* Code stays in the monospace family, not the reading serif. */
	.content :global(code),
	.content :global(pre),
	.content :global(kbd) {
		font-family: var(--font-mono);
	}

	.content :global(a.wikilink) {
		color: var(--editor-link);
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
		border: 1px solid var(--editor-border);
		padding: 6px 12px;
	}

	.content :global(blockquote) {
		border-left: 3px solid var(--editor-border);
		margin: 1em 0;
		padding: 0.5em 1em;
		color: var(--editor-text-faint);
	}

	.content :global(code) {
		padding: 0.15em 0.35em;
		border-radius: 4px;
		background: var(--bg-panel);
		font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
		font-size: 0.9em;
	}

	.content :global(pre) {
		margin: 1em 0;
		padding: 1em;
		border: 1px solid var(--editor-border);
		border-radius: var(--radius-sm);
		background: var(--bg-panel);
		overflow-x: auto;
	}

	.content :global(pre code) {
		padding: 0;
		background: transparent;
	}

	.content :global(.callout) {
		--callout-current: var(--callout-note);
		--callout-icon: '✎';
		border: 1px solid color-mix(in srgb, var(--callout-current) 42%, transparent);
		border-left: 4px solid var(--callout-current);
		border-radius: var(--radius-sm);
		padding: 12px 16px;
		margin: 1em 0;
		background: color-mix(in srgb, var(--callout-current) 13%, var(--editor-bg));
		color: var(--editor-text);
	}

	.content :global(.callout-title) {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--callout-current);
		font-weight: 700;
	}

	.content :global(.callout-title::before) {
		content: var(--callout-icon);
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
		color: var(--editor-text-muted);
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
		--callout-current: var(--callout-note);
		--callout-icon: '✎';
	}

	.content :global(.callout-abstract) {
		--callout-current: var(--callout-abstract);
		--callout-icon: '☷';
	}

	.content :global(.callout-info) {
		--callout-current: var(--callout-info);
		--callout-icon: 'ⓘ';
	}

	.content :global(.callout-todo) {
		--callout-current: var(--callout-todo);
		--callout-icon: '☑';
	}

	.content :global(.callout-tip) {
		--callout-current: var(--callout-tip);
		--callout-icon: '🔥';
	}

	.content :global(.callout-success) {
		--callout-current: var(--callout-success);
		--callout-icon: '✓';
	}

	.content :global(.callout-question) {
		--callout-current: var(--callout-question);
		--callout-icon: '?';
	}

	.content :global(.callout-warning) {
		--callout-current: var(--callout-warning);
		--callout-icon: '⚠';
	}

	.content :global(.callout-failure) {
		--callout-current: var(--callout-failure);
		--callout-icon: '✕';
	}

	.content :global(.callout-danger) {
		--callout-current: var(--callout-danger);
		--callout-icon: '⚡';
	}

	.content :global(.callout-bug) {
		--callout-current: var(--callout-bug);
		--callout-icon: '◉';
	}

	.content :global(.callout-example) {
		--callout-current: var(--callout-example);
		--callout-icon: '▦';
	}

	.content :global(.callout-quote) {
		--callout-current: var(--callout-quote);
		--callout-icon: '❝';
	}

	.content :global(input[type='checkbox']) {
		cursor: pointer;
		pointer-events: auto;
	}

	.content :global(li) {
		color: var(--editor-text);
	}

	.loading,
	.error {
		padding: 20px;
		text-align: center;
	}

	.error {
		color: var(--color-danger);
	}
</style>
