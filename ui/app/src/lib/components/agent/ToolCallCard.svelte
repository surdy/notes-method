<script lang="ts">
	import type { ToolItem } from '$lib/agent/conversation';

	let { item }: { item: ToolItem } = $props();
	let expanded = $state(false);

	function formatArgs(args: unknown): string {
		if (args == null) return '';
		if (typeof args === 'string') return args;
		try {
			return JSON.stringify(args, null, 2);
		} catch {
			return String(args);
		}
	}
</script>

<div class="tool" class:error={item.result?.isError}>
	<button
		class="tool-head"
		type="button"
		onclick={() => (expanded = !expanded)}
		aria-expanded={expanded}
	>
		<span class="caret" class:open={expanded} aria-hidden="true">▸</span>
		<span class="name">{item.name}</span>
		<span class="status">
			{#if item.result === null}
				running…
			{:else if item.result.isError}
				failed
			{:else}
				done
			{/if}
		</span>
	</button>
	{#if expanded}
		<div class="tool-body">
			{#if formatArgs(item.args)}
				<pre class="block">{formatArgs(item.args)}</pre>
			{/if}
			{#if item.result}
				<pre class="block" class:err={item.result.isError}>{item.result.content}</pre>
			{/if}
		</div>
	{/if}
</div>

<style>
	.tool {
		margin: 6px 12px;
		border: 1px solid var(--border-default);
		border-radius: 8px;
		background: var(--bg-secondary);
		overflow: hidden;
	}

	.tool.error {
		border-color: var(--danger-border);
	}

	.tool-head {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 8px 10px;
		border: none;
		background: transparent;
		color: var(--text-default);
		text-align: left;
		cursor: pointer;
		font-size: 12px;
	}

	.tool-head:hover {
		background: var(--bg-hover);
	}

	.caret {
		color: var(--text-muted);
		transition: transform 120ms ease;
	}

	.caret.open {
		transform: rotate(90deg);
	}

	.name {
		font-weight: 600;
		font-family: var(--font-mono);
	}

	.status {
		margin-left: auto;
		font-size: 11px;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.tool.error .status {
		color: var(--danger-text);
	}

	.tool-body {
		border-top: 1px solid var(--border-default);
		padding: 8px 10px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.block {
		margin: 0;
		padding: 8px;
		border-radius: 6px;
		background: var(--bg-default);
		color: var(--text-secondary);
		font-family: var(--font-mono);
		font-size: 11px;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		max-height: 240px;
		overflow: auto;
	}

	.block.err {
		color: var(--danger-text);
	}
</style>
