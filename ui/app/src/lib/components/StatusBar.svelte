<script lang="ts">
	import ConnectionStatus from '$lib/components/ConnectionStatus.svelte';
	import ConnectionSwitcher from '$lib/components/ConnectionSwitcher.svelte';
	import SaveIndicator from '$lib/components/SaveIndicator.svelte';
	import { editorStatus } from '$lib/editor-status.svelte';
	import { saveQueue, saveState } from '$lib/save-queue';

	let {
		currentVault,
		onToast,
		restartRequired
	}: {
		currentVault: string;
		onToast: (message: string, type: 'info' | 'error') => void;
		restartRequired: boolean;
	} = $props();

	let wordLabel = $derived.by(() =>
		editorStatus.wordCount === 1 ? '1 word' : `${editorStatus.wordCount} words`
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
		height: 28px;
		padding: 0 12px;
		background: var(--bg-secondary);
		border-top: 1px solid var(--border-default);
		font-size: 12px;
		color: var(--text-muted);
		flex-shrink: 0;
		gap: 16px;
	}

	.status-left {
		display: flex;
		align-items: center;
		gap: 6px;
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
		gap: 12px;
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}
</style>
