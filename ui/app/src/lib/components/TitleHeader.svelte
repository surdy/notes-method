<script lang="ts">
	import { tick } from 'svelte';
	import { ApiError, renameNote } from '$lib/api';
	import { displayTitleFor } from '$lib/display-title';
	import { computeStem, hasFrontmatterTitle, validateName } from '$lib/title-rename';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	interface Props {
		path: string;
		frontmatter: Record<string, unknown> | null;
		variant?: 'editor' | 'viewer';
	}

	let { path, frontmatter, variant = 'editor' }: Props = $props();

	let editing = $state(false);
	let draft = $state('');
	let inputEl = $state<HTMLInputElement | null>(null);
	let error = $state<string | null>(null);
	let saving = $state(false);

	let title = $derived(displayTitleFor({ path, frontmatter: frontmatter ?? undefined }));
	let frontmatterTitle = $derived(hasFrontmatterTitle(frontmatter));
	let canEdit = $derived(!frontmatterTitle && Boolean(path));

	async function startEdit() {
		if (!canEdit || editing) return;
		const stem = computeStem(path);
		draft = stem ?? title;
		error = null;
		editing = true;
		await tick();
		inputEl?.focus();
		inputEl?.select();
	}

	function cancelEdit() {
		editing = false;
		error = null;
		draft = '';
	}

	async function commit() {
		if (!editing || saving) return;
		const trimmed = draft.trim();
		const currentStem = computeStem(path) ?? '';
		if (!trimmed || trimmed === currentStem) {
			cancelEdit();
			return;
		}
		const validation = validateName(trimmed);
		if (validation) {
			error = validation;
			return;
		}
		saving = true;
		error = null;
		try {
			const response = await renameNote(vaultStore.currentVault, path, trimmed);
			tabStore.rewritePaths((p) => (p === response.from ? response.to : p));
			await vaultStore.loadNotes();
			editing = false;
		} catch (err) {
			if (err instanceof ApiError) {
				error = err.message || `Failed to rename (status ${err.status})`;
			} else {
				error = err instanceof Error ? err.message : 'Failed to rename';
			}
		} finally {
			saving = false;
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			void commit();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			cancelEdit();
		}
	}

	function onHeaderKeydown(event: KeyboardEvent) {
		if (!canEdit) return;
		if (event.key === 'F2' || event.key === 'Enter') {
			event.preventDefault();
			void startEdit();
		}
	}
</script>

{#if editing}
	<div class={`title-header title-header--editing title-header--${variant}`}>
		<!-- svelte-ignore a11y_autofocus -->
		<input
			bind:this={inputEl}
			bind:value={draft}
			class="title-input"
			type="text"
			disabled={saving}
			onkeydown={onKeydown}
			onblur={() => void commit()}
			aria-label="Note name"
		/>
		{#if error}
			<div class="title-error" role="alert">{error}</div>
		{/if}
	</div>
{:else}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div
		class={`title-header title-header--${variant}`}
		class:title-header--readonly={!canEdit}
		title={frontmatterTitle ? 'Edit `title:` in frontmatter to change the displayed title' : 'Click or press F2 to rename'}
		tabindex={canEdit ? 0 : -1}
		role={canEdit ? 'button' : undefined}
		onclick={() => void startEdit()}
		onkeydown={onHeaderKeydown}
	>
		{title}
	</div>
{/if}

<style>
	.title-header {
		padding: 16px 32px 8px;
		font-size: 2em;
		font-weight: 700;
		line-height: 1.2;
		color: var(--ns-editor-text);
		border-bottom: 1px solid var(--ns-border);
		margin-bottom: 4px;
		user-select: text;
		cursor: text;
		outline: none;
	}

	.title-header--viewer {
		padding: 0 0 0.3em;
		margin: 0 0 0.6em;
		border-bottom: 1px solid var(--ns-border);
	}

	.title-header--readonly {
		cursor: default;
	}

	.title-header:not(.title-header--readonly):hover {
		background: var(--ns-surface-hover);
	}

	.title-header:focus-visible {
		box-shadow: inset 0 0 0 2px var(--ns-accent);
	}

	.title-input {
		width: 100%;
		font: inherit;
		color: var(--ns-editor-text);
		background: transparent;
		border: none;
		padding: 0;
		outline: none;
	}

	.title-error {
		font-size: 0.5em;
		font-weight: 400;
		color: var(--ns-danger);
		margin-top: 4px;
	}
</style>
