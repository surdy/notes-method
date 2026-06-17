<script lang="ts">
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { goto } from '$app/navigation';
	import { settingsRoute } from '$lib/vault-menu';
	import {
		createConnectionClient,
		LOCAL_IDENTITY,
		type ConnectionIdentity,
		type ConnectionList,
		type ConnectionTestResult
	} from '$lib/connection/connection-client';
	import { connectionBadge, connectionDetail } from '$lib/connection/badge-view';

	let { currentVault = '' }: { currentVault?: string } = $props();

	const client = createConnectionClient();

	// The badge reflects *this window's* own connection (ADR 0017 C.2) — it is a
	// pure indicator and never retargets the app. Switching servers is the job of
	// the File → New Window menu (grouped by server, with each server's real
	// vaults); the badge only links to "Manage servers…".
	let identity = $state<ConnectionIdentity>(LOCAL_IDENTITY);
	let list = $state<ConnectionList>({ active_id: LOCAL_IDENTITY.id, servers: [] });
	let status = $state<ConnectionTestResult | null>(null);
	let checking = $state(false);
	let open = $state(false);

	let serverUrl = $derived(list.servers.find((entry) => entry.id === identity.id)?.url ?? null);
	let badge = $derived(connectionBadge(identity, status, checking));
	let detail = $derived(connectionDetail(identity, status, checking, serverUrl));

	onMount(() => {
		if (!client.available()) return;
		void load();
		// Reflect server-list edits (rename / reachability) in the badge detail
		// without a reload.
		const unsubscribe = client.onChanged((next) => {
			list = next;
		});
		return unsubscribe;
	});

	async function load() {
		try {
			identity = await client.windowInfo();
		} catch {
			identity = LOCAL_IDENTITY;
		}
		try {
			list = await client.list();
		} catch {
			// Keep the default empty list — the badge must never break the bar.
		}
		await probe();
	}

	/** Best-effort reachability probe of this window's remote, for the dot. */
	async function probe() {
		if (!identity.remote) {
			status = null;
			checking = false;
			return;
		}
		const server = list.servers.find((entry) => entry.id === identity.id);
		if (!server) {
			status = null;
			checking = false;
			return;
		}
		checking = true;
		try {
			status = await client.test(server.url, null);
		} catch {
			status = null;
		} finally {
			checking = false;
		}
	}

	function toggle() {
		open = !open;
	}

	function close() {
		open = false;
	}

	function manage() {
		close();
		void goto(settingsRoute(base, currentVault, 'connection'));
	}
</script>

<svelte:window
	onclick={(event) => {
		if (open && !(event.target as HTMLElement)?.closest('.connection-switcher')) close();
	}}
	onkeydown={(event) => {
		if (open && event.key === 'Escape') close();
	}}
/>

<div class="connection-switcher">
	<button
		type="button"
		class="pill"
		class:open
		aria-haspopup="dialog"
		aria-expanded={open}
		title={badge.title}
		onclick={toggle}
	>
		<span class="pill-icon" aria-hidden="true">{badge.icon}</span>
		{#if badge.dot !== 'none'}
			<span class="dot {badge.dot}" aria-hidden="true"></span>
		{/if}
		<span class="pill-label">{badge.label}</span>
		<span class="pill-caret" aria-hidden="true">▾</span>
	</button>

	{#if open}
		<div class="menu" role="dialog" aria-label="Connection details">
			<div class="detail">
				<div class="detail-head">
					<span class="detail-icon" aria-hidden="true">{badge.icon}</span>
					<span class="detail-name">{detail.name}</span>
				</div>
				<div class="detail-kind">{detail.kindLabel}</div>
				<div class="detail-status">
					{#if detail.dot !== 'none'}
						<span class="dot {detail.dot}" aria-hidden="true"></span>
					{/if}
					<span>{detail.statusLabel}</span>
				</div>
				{#if detail.url}
					<div class="detail-url" title={detail.url}>{detail.url}</div>
				{/if}
			</div>

			<div class="menu-divider" role="separator"></div>

			<button type="button" class="menu-item manage" role="menuitem" onclick={manage}>
				Manage servers…
			</button>
		</div>
	{/if}
</div>

<style>
	.connection-switcher {
		position: relative;
		display: flex;
		align-items: center;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		max-width: 240px;
		padding: 2px 8px;
		border: 1px solid var(--border-default);
		border-radius: 12px;
		background: var(--button-bg);
		color: var(--text-default);
		font-size: 12px;
		cursor: pointer;
	}

	.pill:hover,
	.pill.open {
		background: var(--button-hover);
	}

	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.dot.live {
		background: var(--status-connected);
	}

	.dot.offline {
		background: var(--status-disconnected);
	}

	.dot.checking {
		background: var(--status-reconnecting);
	}

	.pill-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.pill-caret {
		font-size: 9px;
		color: var(--text-muted);
	}

	.menu {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 0;
		min-width: 240px;
		max-width: 320px;
		padding: 4px;
		background: var(--bg-elevated);
		border: 1px solid var(--border-default);
		border-radius: 6px;
		box-shadow: var(--shadow-popover);
		z-index: 50;
	}

	.detail {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 6px 8px;
	}

	.detail-head {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}

	.detail-icon {
		flex-shrink: 0;
	}

	.detail-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-default);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.detail-kind {
		font-size: 11px;
		color: var(--text-muted);
	}

	.detail-status {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-default);
	}

	.detail-url {
		font-size: 11px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.menu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 6px 8px;
		border: none;
		border-radius: 4px;
		background: none;
		color: var(--text-muted);
		font-size: 12px;
		text-align: left;
		cursor: pointer;
	}

	.menu-item:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.menu-divider {
		height: 1px;
		margin: 4px 0;
		background: var(--border-default);
	}
</style>
