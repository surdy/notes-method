<script lang="ts">
	import type { Attachment, AttachmentKind } from '$lib/agent/context-attachments';

	let {
		items,
		kind = null,
		selected = 0,
		onselect,
		onhover
	}: {
		items: Attachment[];
		kind?: AttachmentKind | null;
		selected?: number;
		onselect: (attachment: Attachment) => void;
		onhover?: (index: number) => void;
	} = $props();

	const kindLabel = $derived(kind ?? 'note');
</script>

{#if items.length > 0}
	<ul class="palette" role="listbox" aria-label="Mention {kindLabel}s">
		{#each items as item, i (item.kind + ':' + item.value)}
			<li role="presentation">
				<button
					type="button"
					role="option"
					aria-selected={i === selected}
					class="item"
					class:active={i === selected}
					onmousedown={(e) => {
						// Keep textarea focus; mousedown fires before blur.
						e.preventDefault();
						onselect(item);
					}}
					onmousemove={() => onhover?.(i)}
				>
					<span class="badge">{item.kind}</span>
					<span class="label">{item.label}</span>
				</button>
			</li>
		{/each}
	</ul>
{/if}

<style>
	.palette {
		list-style: none;
		margin: 0;
		padding: 4px;
		max-height: 220px;
		overflow-y: auto;
		border: 1px solid var(--border-default);
		border-radius: 8px;
		background: var(--bg-elevated);
		box-shadow: var(--shadow-popover);
	}

	.item {
		display: flex;
		align-items: baseline;
		gap: 8px;
		width: 100%;
		padding: 6px 8px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--text-default);
		text-align: left;
		font-size: 13px;
		cursor: pointer;
	}

	.item:hover,
	.item.active {
		background: var(--bg-hover);
	}

	.badge {
		padding: 0 6px;
		border-radius: 10px;
		background: var(--badge-bg);
		color: var(--badge-text);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		white-space: nowrap;
	}

	.label {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
