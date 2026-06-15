<script lang="ts">
	import type { MessageItem } from '$lib/agent/conversation';
	import { renderMarkdown } from '$lib/agent/markdown';

	let { item }: { item: MessageItem } = $props();

	let isAgent = $derived(item.role !== 'user');
	let html = $derived(isAgent && item.text ? renderMarkdown(item.text) : '');
</script>

<div class="message" class:user={item.role === 'user'} class:agent={item.role !== 'user'}>
	<span class="role">{item.role === 'user' ? 'You' : 'Agent'}</span>
	<div class="bubble">
		{#if item.text}
			{#if isAgent}
				<!-- renderMarkdown HTML-escapes all input before injecting tags -->
				<div class="text markdown">{@html html}</div>
			{:else}
				<p class="text">{item.text}</p>
			{/if}
		{:else if item.streaming}
			<span class="thinking" aria-label="Agent is responding">…</span>
		{/if}
	</div>
</div>

<style>
	.message {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
	}

	.message.user {
		align-items: flex-end;
	}

	.role {
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--text-muted);
	}

	.bubble {
		max-width: 90%;
		padding: 8px 12px;
		border-radius: 10px;
		border: 1px solid var(--border-default);
		background: var(--bg-surface);
		color: var(--text-default);
	}

	.message.user .bubble {
		background: var(--accent-bg);
		color: var(--accent-text);
		border-color: var(--accent-bg);
	}

	.text {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.markdown {
		white-space: normal;
	}

	.markdown :global(p) {
		margin: 0 0 8px;
	}

	.markdown :global(p:last-child) {
		margin-bottom: 0;
	}

	.markdown :global(ul),
	.markdown :global(ol) {
		margin: 4px 0;
		padding-left: 20px;
	}

	.markdown :global(li) {
		margin: 2px 0;
	}

	.markdown :global(h1),
	.markdown :global(h2),
	.markdown :global(h3),
	.markdown :global(h4),
	.markdown :global(h5),
	.markdown :global(h6) {
		margin: 8px 0 4px;
		font-size: 13px;
		font-weight: 700;
	}

	.markdown :global(a) {
		color: var(--accent-bg);
		text-decoration: underline;
	}

	.markdown :global(code) {
		font-family: var(--font-mono);
		font-size: 0.92em;
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
	}

	.markdown :global(pre) {
		margin: 6px 0;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
		overflow-x: auto;
	}

	.markdown :global(pre code) {
		padding: 0;
		border: none;
		background: none;
	}

	.thinking {
		color: var(--text-muted);
		font-size: 16px;
		letter-spacing: 2px;
	}
</style>
