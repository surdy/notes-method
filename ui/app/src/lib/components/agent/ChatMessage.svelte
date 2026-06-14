<script lang="ts">
	import type { MessageItem } from '$lib/agent/conversation';

	let { item }: { item: MessageItem } = $props();
</script>

<div class="message" class:user={item.role === 'user'} class:agent={item.role !== 'user'}>
	<span class="role">{item.role === 'user' ? 'You' : 'Agent'}</span>
	<div class="bubble">
		{#if item.text}
			<p class="text">{item.text}</p>
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

	.thinking {
		color: var(--text-muted);
		font-size: 16px;
		letter-spacing: 2px;
	}
</style>
