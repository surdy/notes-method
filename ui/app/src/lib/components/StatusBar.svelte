<script lang="ts">
	import ConnectionStatus from '$lib/components/ConnectionStatus.svelte';
	import ConnectionSwitcher from '$lib/components/ConnectionSwitcher.svelte';
	import SaveIndicator from '$lib/components/SaveIndicator.svelte';
	import { editorStatus } from '$lib/editor-status.svelte';
	import { saveQueue, saveState } from '$lib/save-queue';
	import { gitCheckpoint } from '$lib/git-checkpoint.svelte';

	let {
		currentVault,
		onToast,
		restartRequired,
		onOpenGitHistory
	}: {
		currentVault: string;
		onToast: (message: string, type: 'info' | 'error') => void;
		restartRequired: boolean;
		onOpenGitHistory: () => void;
	} = $props();

	let wordLabel = $derived.by(() =>
		editorStatus.wordCount === 1 ? '1 word' : `${editorStatus.wordCount} words`
	);

	let changedLabel = $derived.by(() =>
		gitCheckpoint.changedCount === 1
			? '1 changed file'
			: `${gitCheckpoint.changedCount} changed files`
	);
</script>

<div class="status-bar">
	<div class="status-left">
		<ConnectionSwitcher {currentVault} />
		<ConnectionStatus
			{currentVault}
			{onToast}
			{restartRequired}
			variant="inline"
			showQueueBadge={false}
		/>
	</div>

	<div class="status-center" title={currentVault}>{currentVault || 'No vault selected'}</div>

	<div class="status-right">
		{#if gitCheckpoint.gitEnabled && gitCheckpoint.changedCount > 0}
			<button
				type="button"
				class="git-badge"
				title={`${changedLabel} — click to view history`}
				onclick={onOpenGitHistory}
			>
				<span class="git-dot" aria-hidden="true"></span>
				{gitCheckpoint.committing ? 'Committing…' : `${gitCheckpoint.changedCount} changed`}
			</button>
		{/if}
		<span>Ln {editorStatus.line}, Col {editorStatus.col}</span>
		<span>{wordLabel}</span>
		<SaveIndicator
			state={$saveState}
			onRetry={() => void saveQueue.retryAll()}
			variant="inline"
		/>
	</div>
</div>

<style>
	.status-bar {
		display: flex;
		align-items: center;
		height: 32px;
		padding: 0 16px;
		background: var(--bg-secondary);
		border-top: 1px solid var(--border-subtle);
		font-size: 12px;
		color: var(--text-muted);
		flex-shrink: 0;
		gap: 20px;
	}

	.status-left {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}

	.status-center {
		flex: 1;
		text-align: center;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.status-right {
		display: flex;
		align-items: center;
		gap: 18px;
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}

	.git-badge {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 3px 10px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		color: var(--text-muted);
		font-size: 12px;
		font-family: inherit;
		cursor: pointer;
	}

	.git-badge:hover:not(:disabled) {
		color: var(--text-default);
		border-color: var(--accent);
	}

	.git-badge:disabled {
		cursor: default;
		opacity: 0.7;
	}

	.git-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--accent);
	}
</style>
