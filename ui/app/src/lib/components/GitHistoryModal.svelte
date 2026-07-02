<!--
	GitHistoryModal — full-height overlay hosting the live git-history island for
	the active vault. Opened from the status-bar changed-files badge.
-->
<script lang="ts">
	import GitHistoryLive from '$lib/git-island/GitHistoryLive.svelte';
	import { gitCheckpoint } from '$lib/git-checkpoint.svelte';

	let {
		vault,
		onClose,
		onToast
	}: {
		vault: string;
		onClose: () => void;
		onToast: (message: string, type: 'info' | 'error') => void;
	} = $props();

	let reloadKey = $state(0);

	async function commitNow(): Promise<void> {
		try {
			const result = await gitCheckpoint.commitNow();
			if (result?.committed) {
				onToast(`Checkpoint committed (${result.files.length} files)`, 'info');
				reloadKey += 1; // Re-mount the live view so the new commit appears.
			} else {
				onToast('Nothing to commit', 'info');
			}
		} catch (err) {
			onToast(`Checkpoint failed: ${err instanceof Error ? err.message : String(err)}`, 'error');
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		}
	}
</script>

<div
	class="modal-backdrop"
	role="dialog"
	aria-modal="true"
	aria-labelledby="git-history-title"
	tabindex="-1"
	onclick={(event) => event.target === event.currentTarget && onClose()}
	onkeydown={handleKeydown}
>
	<div class="modal-sheet">
		<header class="modal-head">
			<h2 id="git-history-title" class="modal-title">History · {vault}</h2>
			<div class="modal-head-actions">
				<button
					type="button"
					class="btn-secondary"
					disabled={gitCheckpoint.committing}
					onclick={() => void commitNow()}
				>
					{gitCheckpoint.committing ? 'Committing…' : 'Commit now'}
				</button>
				<button type="button" class="btn-close" aria-label="Close" onclick={onClose}>✕</button>
			</div>
		</header>
		<div class="modal-body">
			{#key reloadKey}
				<GitHistoryLive {vault} />
			{/key}
		</div>
	</div>
</div>

<style>
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.modal-sheet {
		display: flex;
		flex-direction: column;
		background: var(--bg-elevated);
		color: var(--text-default);
		border: 1px solid var(--border-default);
		border-radius: 8px;
		width: min(1000px, 92vw);
		height: min(720px, 88vh);
		box-shadow: 0 10px 40px rgba(0, 0, 0, 0.4);
		overflow: hidden;
	}

	.modal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border-default);
		flex-shrink: 0;
	}

	.modal-title {
		margin: 0;
		font-size: 14px;
		font-weight: 600;
	}

	.modal-head-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.modal-body {
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.btn-secondary {
		padding: 5px 12px;
		border-radius: 4px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid var(--border-default);
		background: transparent;
		color: var(--text-muted);
	}

	.btn-secondary:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.btn-secondary:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.btn-close {
		padding: 4px 8px;
		border-radius: 4px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 13px;
		cursor: pointer;
	}

	.btn-close:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}
</style>
