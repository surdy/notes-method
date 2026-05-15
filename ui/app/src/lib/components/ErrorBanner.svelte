<script lang="ts">
	import type { ClassifiedError } from '$lib/api/error-classify';

	let {
		error,
		onAction,
		onDismiss
	}: {
		error: ClassifiedError | null;
		onAction?: () => void;
		onDismiss?: () => void;
	} = $props();
</script>

{#if error}
	<div class="error-banner" role="alert" aria-live="polite">
		<div class="error-content">
			<strong>{error.title}</strong>
			<span>{error.message}</span>
			<span class="hint">{error.hint}</span>
		</div>

		<div class="error-actions">
			{#if error.action}
				<button class="action-btn" type="button" onclick={() => onAction?.()}>
					{error.action.label}
				</button>
			{/if}

			{#if onDismiss}
				<button class="dismiss-btn" type="button" onclick={() => onDismiss?.()} aria-label="Dismiss error">
					×
				</button>
			{/if}
		</div>
	</div>
{/if}

<style>
	.error-banner {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 12px;
		border: 1px solid #7a5a12;
		border-radius: 8px;
		background: #3a2b0b;
		color: #f5e7b6;
	}

	.error-content {
		display: grid;
		gap: 4px;
		font-size: 13px;
		line-height: 1.45;
	}

	.hint {
		color: #f8ddb0;
	}

	.error-actions {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		flex-shrink: 0;
	}

	.action-btn,
	.dismiss-btn {
		border-radius: 6px;
		border: 1px solid rgba(255, 255, 255, 0.15);
		background: rgba(255, 255, 255, 0.08);
		color: inherit;
		cursor: pointer;
	}

	.action-btn {
		padding: 6px 10px;
		font-size: 12px;
		white-space: nowrap;
	}

	.dismiss-btn {
		width: 28px;
		height: 28px;
		font-size: 18px;
		line-height: 1;
	}

	.action-btn:hover,
	.dismiss-btn:hover {
		background: rgba(255, 255, 255, 0.14);
	}

	@media (max-width: 640px) {
		.error-banner {
			flex-direction: column;
		}

		.error-actions {
			width: 100%;
			justify-content: space-between;
		}
	}
</style>
