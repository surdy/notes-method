<!--
  Popup shown when a clicked [[wikilink]] has no confident match (UX review:
  "Link navigation should help create and disambiguate"). Offers fuzzy
  candidates ("did you mean…") and a "Create note" action instead of the old
  silent no-op. Positioned at the click and dismissed on outside-click/Escape.
-->
<script lang="ts">
	import type { NoteSummary } from '$lib/api';
	import { displayTitleFor } from '$lib/display-title';

	let {
		target,
		candidates,
		x,
		y,
		onNavigate,
		onCreate,
		onClose
	}: {
		target: string;
		candidates: NoteSummary[];
		x: number;
		y: number;
		onNavigate: (path: string) => void;
		onCreate: () => void;
		onClose: () => void;
	} = $props();

	function onWindowKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		}
	}
</script>

<svelte:window onkeydown={onWindowKeydown} />

<button
	type="button"
	class="backdrop"
	aria-label="Close menu"
	onclick={onClose}
	oncontextmenu={(e) => {
		e.preventDefault();
		onClose();
	}}
></button>

<div class="menu" role="menu" style="left: {x}px; top: {y}px;">
	{#if candidates.length > 0}
		<p class="menu-label">Did you mean…</p>
		{#each candidates as candidate (candidate.path)}
			<button
				type="button"
				class="menu-item"
				role="menuitem"
				onclick={() => onNavigate(candidate.path)}
			>
				<span class="item-title">{displayTitleFor(candidate)}</span>
				<span class="item-path">{candidate.path}</span>
			</button>
		{/each}
		<div class="divider"></div>
	{:else}
		<p class="menu-label">No note named “{target}”</p>
	{/if}
	<button type="button" class="menu-item create" role="menuitem" onclick={onCreate}>
		＋ Create note “{target}”
	</button>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 1000;
		background: transparent;
		border: none;
		padding: 0;
		margin: 0;
		cursor: default;
	}

	.menu {
		position: fixed;
		z-index: 1001;
		min-width: 220px;
		max-width: 340px;
		padding: 5px;
		background: var(--bg-elevated);
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		box-shadow: var(--shadow-pop);
	}

	.menu-label {
		margin: 4px 8px 6px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
	}

	.menu-item {
		display: flex;
		flex-direction: column;
		gap: 1px;
		width: 100%;
		padding: 7px 9px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 5px;
		color: var(--text-secondary);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
	}

	.menu-item:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.menu-item:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -2px;
		color: var(--text-default);
	}

	.item-title {
		font-weight: 500;
	}

	.item-path {
		font-size: 11px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.divider {
		height: 1px;
		margin: 5px 4px;
		background: var(--border-default);
	}

	.menu-item.create {
		color: var(--accent);
		font-weight: 500;
	}
</style>
