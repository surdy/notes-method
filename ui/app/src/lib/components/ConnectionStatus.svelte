<script lang="ts">
	import { onMount } from 'svelte';
	import { connectionState, daemonShuttingDown } from '$lib/sse';
	import {
		fetchDaemonStatus,
		fetchLogTail,
		reindexVault,
		restartDaemon,
		type DaemonStatus,
		type VaultStatus
	} from '$lib/api/status';
	import { queuedCount } from '$lib/save-queue';

	type StatusVisualState =
		| 'connected'
		| 'reconnecting'
		| 'disconnected'
		| 'restart-required'
		| 'rebuilding';

	const STATUS_POLL_MS = 30_000;

	let {
		currentVault,
		onToast = (_message: string, _type: 'info' | 'error') => {},
		restartRequired = false
	}: {
		currentVault: string;
		onToast?: (message: string, type: 'info' | 'error') => void;
		restartRequired?: boolean;
	} = $props();

	let container = $state<HTMLElement | null>(null);
	let status = $state<DaemonStatus | null>(null);
	let showPopover = $state(false);
	let showLogs = $state(false);
	let logContent = $state('');
	let logError = $state('');
	let loadingLogs = $state(false);
	let lastStatusCheckAt = $state<number | null>(null);
	let statusError = $state('');
	let pollTimer = $state<number | null>(null);
	let reindexing = $state(false);
	let restarting = $state(false);

	let currentVaultStatus = $derived.by<VaultStatus | null>(() => status?.vaults[currentVault] ?? null);
	let serverRebuilding = $derived.by(() =>
		Object.values(status?.vaults ?? {}).some((vault) => vault.state === 'rebuilding')
	);
	let isRebuilding = $derived.by(() => reindexing || serverRebuilding);
	let hasRecentStatus = $derived.by(
		() => lastStatusCheckAt !== null && Date.now() - lastStatusCheckAt < STATUS_POLL_MS
	);
	let visualState = $derived.by<StatusVisualState>(() => {
		if (isRebuilding) return 'rebuilding';
		if (restartRequired) return 'restart-required';
		if ($connectionState === 'reconnecting') return 'reconnecting';
		if ($connectionState !== 'connected' || !hasRecentStatus) return 'disconnected';
		return 'connected';
	});
	let pillLabel = $derived.by(() => {
		if (isRebuilding) return 'Rebuilding index';
		if (restarting || $daemonShuttingDown) return 'Restarting…';
		switch (visualState) {
			case 'connected':
				return 'Connected';
			case 'reconnecting':
				return 'Reconnecting';
			case 'restart-required':
				return 'Restart required';
			case 'rebuilding':
				return 'Rebuilding index';
			default:
				return 'Disconnected';
		}
	});

	$effect(() => {
		if (!showPopover) {
			showLogs = false;
		}
	});

	$effect(() => {
		if (restarting && !$daemonShuttingDown && $connectionState === 'connected' && hasRecentStatus) {
			restarting = false;
		}
	});

	async function refreshStatus() {
		try {
			const nextStatus = await fetchDaemonStatus();
			status = nextStatus;
			lastStatusCheckAt = Date.now();
			statusError = '';
		} catch (error) {
			lastStatusCheckAt = null;
			statusError = error instanceof Error ? error.message : 'Status check failed';
		}
	}

	function togglePopover() {
		showPopover = !showPopover;
	}

	function closePopover() {
		showPopover = false;
	}

	async function handleRestart() {
		if (restarting) return;
		try {
			restarting = true;
			await restartDaemon();
			onToast('Restarting daemon…', 'info');
		} catch (error) {
			restarting = false;
			onToast(error instanceof Error ? error.message : 'Failed to restart daemon', 'error');
		}
	}

	async function handleReindex() {
		if (isRebuilding || !currentVault) return;
		try {
			reindexing = true;
			await reindexVault(currentVault);
			await refreshStatus();
			onToast(`Rebuilt index for ${currentVault}`, 'info');
		} catch (error) {
			onToast(error instanceof Error ? error.message : 'Failed to rebuild index', 'error');
		} finally {
			reindexing = false;
		}
	}

	async function handleViewLogs() {
		if (showLogs) {
			showLogs = false;
			return;
		}

		loadingLogs = true;
		showLogs = true;
		logError = '';
		try {
			logContent = await fetchLogTail(200);
		} catch (error) {
			logError = error instanceof Error ? error.message : 'Failed to load logs';
		} finally {
			loadingLogs = false;
		}
	}

	function handleWindowClick(event: MouseEvent) {
		if (!showPopover || !(event.target instanceof Node) || !container) {
			return;
		}
		if (!container.contains(event.target)) {
			closePopover();
		}
	}

	function handleWindowKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			closePopover();
		}
	}

	onMount(() => {
		void refreshStatus();
		pollTimer = window.setInterval(() => {
			void refreshStatus();
		}, STATUS_POLL_MS);
		window.addEventListener('click', handleWindowClick);
		window.addEventListener('keydown', handleWindowKeydown);

		return () => {
			if (pollTimer) {
				clearInterval(pollTimer);
				pollTimer = null;
			}
			window.removeEventListener('click', handleWindowClick);
			window.removeEventListener('keydown', handleWindowKeydown);
		};
	});

	function formatUptime(seconds: number): string {
		if (seconds < 60) return `${seconds}s`;
		if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
		const hours = Math.floor(seconds / 3600);
		const mins = Math.floor((seconds % 3600) / 60);
		return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
		return `${Math.round(bytes / (1024 * 1024))} MB`;
	}
</script>

<div class="status-footer" bind:this={container}>
	{#if showPopover}
		<div class="status-popover" role="dialog" aria-label="Connection status details">
			{#if showLogs}
				{#if loadingLogs}
					<div class="state-text">Loading logs…</div>
				{:else if logError}
					<div class="state-text error">{logError}</div>
				{:else}
					<pre class="log-output">{logContent || 'No logs available.'}</pre>
				{/if}
			{:else}
				<div class="summary-line">
					Version: {status?.version ?? 'Unavailable'} • Uptime:
					{status ? formatUptime(status.uptime_seconds) : 'Unavailable'}
				</div>
				<hr class="divider" />

				{#if currentVaultStatus}
					<div class="detail-section">
						<div class="section-heading">Vault: {currentVault}</div>
						<div class="detail-line">
							{currentVaultStatus.notes} notes • {currentVaultStatus.tasks} tasks
						</div>
						<div class="detail-line">
							Index: {currentVaultStatus.search_indexed ? '✓' : '✗'} • Watcher:
							{#if currentVaultStatus.watcher_health === 'polling'}
								⚠ Polling
							{:else if currentVaultStatus.watcher_health === 'degraded'}
								⚠ Degraded
							{:else}
								{currentVaultStatus.watcher_active ? '✓' : '✗'}
							{/if}
						</div>
						{#if currentVaultStatus.watcher_message}
							<div class="detail-line hint">{currentVaultStatus.watcher_message}</div>
						{/if}
					</div>
				{:else}
					<div class="state-text">
						{status ? `No status reported for ${currentVault}.` : statusError || 'Checking daemon status…'}
					</div>
				{/if}

				<hr class="divider" />

				{#if status}
					<div class="detail-line">
						Memory: {formatBytes(status.resources.memory_rss_bytes)} • SSE:
						{status.resources.sse_connections}
					</div>
					<div class="detail-line">
						Watchers: {status.watchers.active}/{status.watchers.total} • Indexes:
						{status.indexes.caches_ok}/{status.indexes.search_ok}
					</div>
				{:else if statusError}
					<div class="state-text error">{statusError}</div>
				{/if}
			{/if}

			<hr class="divider" />

			<div class="actions">
				<button
					class="action-btn"
					type="button"
					onclick={() => void handleRestart()}
					disabled={isRebuilding || restarting}
				>
					{restarting || $daemonShuttingDown ? 'Restarting…' : 'Restart Service'}
				</button>
				<button
					class="action-btn"
					type="button"
					onclick={() => void handleReindex()}
					disabled={!currentVault || isRebuilding || restarting}
				>
					{isRebuilding ? 'Rebuilding…' : 'Rebuild Index'}
				</button>
				<button
					class="action-btn"
					type="button"
					onclick={() => void handleViewLogs()}
					disabled={loadingLogs}
				>
					{showLogs ? 'Back to Status' : loadingLogs ? 'Loading Logs…' : 'View Logs'}
				</button>
			</div>
		</div>
	{/if}

	<button
		class="status-pill"
		type="button"
		onclick={togglePopover}
		aria-expanded={showPopover}
		aria-haspopup="dialog"
	>
		<span class={`status-dot ${visualState}`} aria-hidden="true"></span>
		<span class="status-label">{pillLabel}</span>
		{#if $queuedCount > 0}
			<span class="queue-badge" aria-label={`${$queuedCount} queued saves`}>{$queuedCount}</span>
		{/if}
	</button>
</div>

<style>
	.status-footer {
		position: relative;
		margin-top: auto;
		padding: 8px 12px 12px;
		border-top: 1px solid var(--border-color, #333);
		flex-shrink: 0;
	}

	.status-pill {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		cursor: pointer;
		border-radius: 4px;
		font-size: 12px;
		color: var(--text-muted, #888);
		border: none;
		background: none;
		width: 100%;
		text-align: left;
	}

	.status-pill:hover {
		background: var(--hover-bg, #2a2d2e);
	}

	.status-label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.queue-badge {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 18px;
		height: 18px;
		padding: 0 6px;
		border-radius: 999px;
		background: #ff9800;
		color: #1f1f1f;
		font-size: 11px;
		font-weight: 700;
		flex-shrink: 0;
	}

	.status-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.status-dot.connected {
		background: #4caf50;
	}

	.status-dot.reconnecting {
		background: #ff9800;
		animation: pulse 1.5s infinite;
	}

	.status-dot.disconnected {
		background: #f44336;
	}

	.status-dot.restart-required {
		background: #2196f3;
	}

	.status-dot.rebuilding {
		background: #9e9e9e;
	}

	.status-popover {
		position: absolute;
		bottom: 100%;
		left: 0;
		right: 0;
		margin-bottom: 4px;
		background: var(--sidebar-bg, #252526);
		border: 1px solid var(--border-color, #333);
		border-radius: 8px;
		padding: 12px;
		box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.3);
		z-index: 100;
		max-height: 400px;
		overflow-y: auto;
		color: var(--text-primary, #e0e0e0);
		font-size: 12px;
	}

	.summary-line,
	.section-heading,
	.detail-line,
	.state-text {
		line-height: 1.5;
	}

	.section-heading {
		font-weight: 600;
	}

	.detail-line.hint {
		color: var(--text-muted, #888);
	}

	.detail-section {
		display: grid;
		gap: 2px;
	}

	.state-text {
		color: var(--text-muted, #888);
	}

	.state-text.error {
		color: #ff6b6b;
	}

	.divider {
		border: 0;
		border-top: 1px solid var(--border-color, #333);
		margin: 10px 0;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.action-btn {
		border: 1px solid var(--border-color, #333);
		border-radius: 6px;
		background: var(--hover-bg, #2a2d2e);
		color: var(--text-primary, #e0e0e0);
		font-size: 12px;
		padding: 6px 10px;
		cursor: pointer;
	}

	.action-btn:hover:not(:disabled) {
		filter: brightness(1.1);
	}

	.action-btn:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.log-output {
		margin: 0;
		max-height: 240px;
		overflow: auto;
		padding: 8px;
		border-radius: 6px;
		background: rgba(0, 0, 0, 0.2);
		border: 1px solid var(--border-color, #333);
		color: var(--text-primary, #e0e0e0);
		font-family:
			ui-monospace, SFMono-Regular, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono',
			'Courier New', monospace;
		font-size: 11px;
		white-space: pre-wrap;
		word-break: break-word;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}

		50% {
			opacity: 0.4;
		}
	}
</style>
