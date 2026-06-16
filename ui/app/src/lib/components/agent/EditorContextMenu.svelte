<!--
  Right-click menu for the note editor (issue 195). Shows the six inline AI
  commands when text is selected; each runs against the active selection through
  the shared chat agent session (no chat panel expansion required). Positioned at
  the cursor and dismissed on outside-click or Escape.
-->
<script lang="ts">
	import { INLINE_COMMANDS } from '$lib/agent/inline-commands';
	import { runInlineEditorCommand } from '$lib/commands';

	let {
		x,
		y,
		onClose
	}: {
		x: number;
		y: number;
		onClose: () => void;
	} = $props();

	async function run(id: (typeof INLINE_COMMANDS)[number]['id']) {
		onClose();
		await runInlineEditorCommand(id);
	}

	function onWindowKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		}
	}
</script>

<svelte:window onkeydown={onWindowKeydown} />

<!-- Backdrop closes the menu on any outside interaction. -->
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
	{#each INLINE_COMMANDS as cmd (cmd.id)}
		<button type="button" class="menu-item" role="menuitem" onclick={() => run(cmd.id)}>
			{cmd.label}
		</button>
	{/each}
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
		min-width: 180px;
		padding: 5px;
		background: var(--bg-elevated);
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		box-shadow: 0 10px 26px rgba(0, 0, 0, 0.4);
	}

	.menu-item {
		display: flex;
		align-items: center;
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
</style>
