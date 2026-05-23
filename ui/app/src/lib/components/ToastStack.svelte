<script lang="ts">
	import { fly } from 'svelte/transition';
	import { toastStore } from '$lib/toast-store.svelte';
</script>

{#if toastStore.toasts.length > 0}
	<div aria-atomic="true" aria-live="polite" class="toast-stack">
		{#each toastStore.toasts as toast (toast.id)}
			<div class="toast" class:success={toast.type === 'success'} class:error={toast.type === 'error'} class:warning={toast.type === 'warning'} in:fly={{ x: 24, duration: 180 }} out:fly={{ x: 24, duration: 160 }}>
				<div class="toast-body">
					<p class="toast-message">{toast.message}</p>
				</div>
				<button aria-label="Dismiss notification" class="toast-dismiss" onclick={() => toastStore.dismiss(toast.id)} type="button">
					×
				</button>
			</div>
		{/each}
	</div>
{/if}

<style>
	.toast-stack {
		position: fixed;
		top: 16px;
		right: 16px;
		z-index: 70;
		display: flex;
		flex-direction: column;
		gap: 10px;
		pointer-events: none;
	}

	.toast {
		width: min(360px, calc(100vw - 32px));
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 12px 14px;
		border: 1px solid var(--border-overlay);
		border-left-width: 4px;
		border-radius: 12px;
		background: var(--bg-panel);
		box-shadow: var(--shadow);
		color: var(--text-default);
		pointer-events: auto;
	}

	.toast.success {
		border-left-color: var(--color-success);
	}

	.toast.error {
		border-left-color: var(--color-danger);
	}

	.toast.warning {
		border-left-color: var(--color-warning);
	}

	.toast-body {
		flex: 1;
		min-width: 0;
	}

	.toast-message {
		margin: 0;
		font-size: 14px;
		line-height: 1.4;
	}

	.toast-dismiss {
		border: none;
		background: transparent;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 18px;
		line-height: 1;
		padding: 0;
	}

	.toast-dismiss:hover {
		color: var(--text-default);
	}
</style>
