<script lang="ts">
	import type { SaveState } from '$lib/save-queue';

	let {
		state,
		onRetry = () => {}
	}: {
		state: SaveState;
		onRetry?: () => void;
	} = $props();

	let label = $derived.by(() => {
		switch (state) {
			case 'saving':
				return 'Saving...';
			case 'saved':
				return '✓ Saved';
			case 'failed-retrying':
				return 'Save failed - Retrying...';
			case 'failed':
				return 'Save failed - Click to retry';
			default:
				return '';
		}
	});

	function handleClick() {
		if (state === 'failed') {
			onRetry();
		}
	}
</script>

<button
	type="button"
	class={`save-indicator ${state === 'idle' ? 'hidden' : state}`}
	onclick={handleClick}
	disabled={state !== 'failed'}
	aria-hidden={state === 'idle'}
	aria-live="polite"
>
	{#if state === 'saving'}
		<span class="spinner" aria-hidden="true"></span>
	{:else if state === 'failed-retrying'}
		<span class="icon" aria-hidden="true">⚠</span>
	{:else if state === 'failed'}
		<span class="icon" aria-hidden="true">!</span>
	{/if}
	<span>{label}</span>
</button>

<style>
	.save-indicator {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		align-self: flex-end;
		margin: 12px 16px 0;
		padding: 8px 12px;
		border-radius: 999px;
		border: 1px solid transparent;
		background: var(--ns-overlay-panel);
		color: var(--ns-text);
		font-size: 12px;
		line-height: 1;
		transition:
			opacity 160ms ease,
			transform 160ms ease,
			background-color 160ms ease,
			border-color 160ms ease,
			margin 160ms ease,
			padding 160ms ease;
	}

	.save-indicator:disabled {
		cursor: default;
	}

	.save-indicator.hidden {
		opacity: 0;
		transform: translateY(-4px);
		pointer-events: none;
		max-height: 0;
		overflow: hidden;
		margin-top: 0;
		margin-bottom: 0;
		padding-top: 0;
		padding-bottom: 0;
		border-width: 0;
	}

	.save-indicator.saved {
		background: var(--ns-success-surface);
		border-color: var(--ns-success-border);
		color: var(--ns-success-text);
	}

	.save-indicator.failed-retrying {
		background: var(--ns-warning-surface);
		border-color: var(--ns-warning-surface-border);
		color: var(--ns-warning-text);
	}

	.save-indicator.failed {
		background: var(--ns-danger-surface);
		border-color: var(--ns-danger-surface-border);
		color: var(--ns-danger-text);
		cursor: pointer;
	}

	.save-indicator.failed:hover {
		filter: brightness(1.08);
	}

	.spinner {
		width: 12px;
		height: 12px;
		border: 2px solid var(--ns-border-translucent-soft);
		border-top-color: currentColor;
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	.icon {
		font-weight: 700;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
