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
	import { connectionBadge, otherServerTargets } from '$lib/connection/badge-view';

	let { currentVault = '' }: { currentVault?: string } = $props();

	const client = createConnectionClient();

	// The badge reflects *this window's* own connection (ADR 0017 C.2) — it never
	// retargets the app. Switching is non-destructive: opening a vault on another
	// server spawns a new window bound to that server.
	let identity = $state<ConnectionIdentity>(LOCAL_IDENTITY);
	let list = $state<ConnectionList>({ active_id: LOCAL_IDENTITY.id, servers: [] });
	let status = $state<ConnectionTestResult | null>(null);
	let checking = $state(false);
	let open = $state(false);

	let badge = $derived(connectionBadge(identity, status, checking));
	let targets = $derived(otherServerTargets(list, identity.id));

	onMount(() => {
		if (!client.available()) return;
		void load();
		// Reflect server-list edits (add/remove) in the "open on another server"
		// menu without a reload.
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

	async function openOn(serverId: string) {
		close();
		if (!currentVault) return;
		try {
			await client.openVaultOnServer(serverId, currentVault);
		} catch {
			// Best-effort: a failed open leaves the current window untouched.
		}
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
		aria-haspopup="menu"
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
		<div class="menu" role="menu">
			<div class="menu-heading">
				{#if currentVault}
					Open “{currentVault}” on…
				{:else}
					No vault selected
				{/if}
			</div>
			{#each targets as target (target.id)}
				<button
					type="button"
					class="menu-item"
					role="menuitem"
					disabled={!currentVault}
					onclick={() => void openOn(target.id)}
				>
					<span class="menu-icon" aria-hidden="true">{target.kind === 'local' ? '💻' : '☁'}</span>
					<span class="menu-text"><span class="menu-name">{target.name}</span></span>
				</button>
			{/each}
			{#if targets.length === 0}
				<div class="menu-empty">No other servers configured</div>
			{/if}

			<div class="menu-divider" role="separator"></div>

			<button type="button" class="menu-item manage" role="menuitem" onclick={manage}>
				<span class="menu-text"><span class="menu-name">Manage servers…</span></span>
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

	.menu-heading {
		padding: 6px 8px 4px;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
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
		color: var(--text-default);
		font-size: 12px;
		text-align: left;
		cursor: pointer;
	}

	.menu-item:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.menu-item:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.menu-icon {
		flex-shrink: 0;
	}

	.menu-text {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.menu-name {
		font-weight: 500;
	}

	.menu-empty {
		padding: 6px 8px;
		font-size: 11px;
		color: var(--text-muted);
	}

	.menu-divider {
		height: 1px;
		margin: 4px 0;
		background: var(--border-default);
	}

	.manage .menu-name {
		color: var(--text-muted);
		font-weight: 400;
	}
</style>
