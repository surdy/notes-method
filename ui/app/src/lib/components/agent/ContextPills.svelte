<script lang="ts">
	import type { Attachment } from '$lib/agent/context-attachments';

	let {
		attachments,
		includeActiveNote = true,
		activeNotePath = null,
		onremove,
		ontoggleactivenote
	}: {
		attachments: Attachment[];
		includeActiveNote?: boolean;
		activeNotePath?: string | null;
		onremove: (attachment: Attachment) => void;
		ontoggleactivenote?: (next: boolean) => void;
	} = $props();

	const hasUrl = $derived(attachments.some((a) => a.kind === 'url'));
	const hasAny = $derived(attachments.length > 0 || activeNotePath !== null);
</script>

{#if hasAny}
	<div class="pills" aria-label="Attached context">
		{#if activeNotePath}
			<label
				class="pill toggle"
				class:off={!includeActiveNote}
				title="Auto-include the active note as context"
			>
				<input
					type="checkbox"
					checked={includeActiveNote}
					onchange={(e) => ontoggleactivenote?.(e.currentTarget.checked)}
				/>
				<span class="kind">active</span>
				<span class="value">{activeNotePath}</span>
			</label>
		{/if}

		{#each attachments as attachment (attachment.kind + ':' + attachment.value)}
			<span class="pill">
				<span class="kind">{attachment.kind}</span>
				<span class="value">{attachment.label}</span>
				<button
					type="button"
					class="remove"
					aria-label="Remove {attachment.kind} {attachment.label}"
					onclick={() => onremove(attachment)}
				>
					×
				</button>
			</span>
		{/each}
	</div>

	{#if hasUrl}
		<p class="hint">URLs are passed to the agent as text; fetching is not yet performed.</p>
	{/if}
{/if}

<style>
	.pills {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		max-width: 100%;
		padding: 2px 6px 2px 8px;
		border: 1px solid var(--border-default);
		border-radius: 12px;
		background: var(--bg-surface);
		color: var(--text-default);
		font-size: 11px;
	}

	.pill.toggle {
		cursor: pointer;
	}

	.pill.toggle.off {
		opacity: 0.55;
	}

	.pill.toggle input {
		margin: 0;
		accent-color: var(--accent-bg);
	}

	.kind {
		color: var(--text-muted);
		text-transform: uppercase;
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.04em;
	}

	.value {
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 220px;
	}

	.remove {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		padding: 0;
		border: none;
		border-radius: 50%;
		background: transparent;
		color: var(--text-muted);
		font-size: 13px;
		line-height: 1;
		cursor: pointer;
	}

	.remove:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.hint {
		margin: 4px 0 0;
		color: var(--text-muted);
		font-size: 10px;
	}
</style>
