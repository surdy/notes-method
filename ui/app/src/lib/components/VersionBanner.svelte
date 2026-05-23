<script lang="ts">
	import { restartDaemon } from '$lib/api/status';
	import { versionMismatch } from '$lib/api/core';

	let restarting = $state(false);
	let restartError = $state('');

	let message = $derived.by(() => {
		const mismatch = $versionMismatch;
		if (!mismatch) return '';
		if (mismatch.direction === 'service-outdated') {
			return `Background service is out of date (service ${mismatch.serverVersion}, app ${mismatch.clientVersion}).`;
		}
		if (mismatch.direction === 'app-outdated') {
			return `Please update the Notesmith app (service ${mismatch.serverVersion}, app ${mismatch.clientVersion}).`;
		}
		return `Notesmith version mismatch detected (service ${mismatch.serverVersion}, app ${mismatch.clientVersion}).`;
	});

	let showRestart = $derived.by(() => $versionMismatch?.direction === 'service-outdated');

	$effect(() => {
		if (!$versionMismatch) {
			restarting = false;
			restartError = '';
		}
	});

	async function handleRestart() {
		if (restarting) return;

		restarting = true;
		restartError = '';
		try {
			await restartDaemon();
		} catch (error) {
			restartError = error instanceof Error ? error.message : 'Failed to restart service';
		} finally {
			restarting = false;
		}
	}
</script>

{#if $versionMismatch}
	<div class="version-banner" role="status" aria-live="polite">
		<div class="banner-copy">
			<span>{message}</span>
			{#if restartError}
				<span class="banner-error">{restartError}</span>
			{/if}
		</div>

		{#if showRestart}
			<button class="banner-action" type="button" onclick={() => void handleRestart()}>
				{restarting ? 'Restarting…' : 'Restart Service'}
			</button>
		{/if}
	</div>
{/if}

<style>
	.version-banner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 16px;
		background: var(--info-banner);
		color: var(--info-banner-text);
		font-size: 13px;
		border-bottom: 1px solid var(--border-translucent);
	}

	.banner-copy {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		align-items: center;
	}

	.banner-error {
		color: var(--info-banner-text-muted);
		opacity: 0.9;
	}

	.banner-action {
		border: 1px solid var(--border-translucent-strong);
		border-radius: 999px;
		background: var(--surface-translucent-strong);
		color: inherit;
		padding: 4px 10px;
		font-size: 12px;
		cursor: pointer;
		white-space: nowrap;
	}

	.banner-action:hover {
		background: var(--surface-translucent-emphasis);
	}

	@media (max-width: 768px) {
		.version-banner {
			flex-direction: column;
			align-items: flex-start;
		}
	}
</style>
