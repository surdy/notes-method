<script lang="ts">
	import type { SlashCommand } from '$lib/agent/slash-commands';

	let {
		commands,
		selected = 0,
		onselect,
		onhover
	}: {
		commands: SlashCommand[];
		selected?: number;
		onselect: (command: SlashCommand) => void;
		onhover?: (index: number) => void;
	} = $props();
</script>

{#if commands.length > 0}
	<ul class="palette" role="listbox" aria-label="Slash commands">
		{#each commands as command, i (command.name)}
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
						onselect(command);
					}}
					onmousemove={() => onhover?.(i)}
				>
					<span class="name">/{command.name}</span>
					{#if command.source === 'vault'}
						<span class="tag">vault</span>
					{/if}
					{#if command.description}
						<span class="desc">{command.description}</span>
					{/if}
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

	.name {
		font-family: var(--font-mono);
		font-weight: 600;
		color: var(--accent);
		white-space: nowrap;
	}

	.tag {
		padding: 0 6px;
		border-radius: 10px;
		background: var(--badge-bg);
		color: var(--badge-text);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.desc {
		flex: 1;
		min-width: 0;
		color: var(--text-muted);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
