<script lang="ts">
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { goto } from '$app/navigation';
	import { settingsRoute } from '$lib/vault-menu';
	import {
		createConnectionClient,
		LOCAL_ID,
		type ConnectionList,
		type ConnectionTestResult
	} from '$lib/connection/connection-client';
	import {
		activeOption,
		connectionOptions,
		pillIcon,
		pillLabel
	} from '$lib/connection/connection-view';

	let { currentVault = '' }: { currentVault?: string } = $props();

	const client = createConnectionClient();

	let list = $state<ConnectionList>({ active_id: LOCAL_ID, servers: [] });
	let activeStatus = $state<ConnectionTestResult | null>(null);
	let open = $state(false);
	let switching = $state(false);

	let options = $derived(connectionOptions(list));
	let label = $derived(pillLabel(list, activeStatus));
	let icon = $derived(pillIcon(list));

	onMount(() => {
		if (!client.available()) return;
		void load();
		const unsubscribe = client.onChanged((next) => {
			list = next;
			void probeActive();
		});
		return unsubscribe;
	});

	async function load() {
		try {
			list = await client.list();
			await probeActive();
		} catch {
			// Leave the default local pill on failure — the switcher must never
			// break the status bar.
		}
	}

	/** Best-effort reachability probe of the active remote, for the latency suffix. */
	async function probeActive() {
		const active = activeOption(list);
		if (active.kind === 'local' || !active.url) {
			activeStatus = null;
			return;
		}
		try {
			activeStatus = await client.test(active.url, null);
		} catch {
			activeStatus = null;
		}
	}

	function toggle() {
		open = !open;
	}

	function close() {
		open = false;
	}

	async function select(id: string) {
		close();
		if (list.active_id === id || switching) return;
		switching = true;
		try {
			list = await client.setActive(id === LOCAL_ID ? null : id);
			await probeActive();
		} catch {
			// Swallow — a failed switch leaves the previous connection in place.
		} finally {
			switching = false;
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
		title="Switch connection"
		onclick={toggle}
		disabled={switching}
	>
		<span class="pill-icon" aria-hidden="true">{icon}</span>
		<span class="pill-label">{label}</span>
		<span class="pill-caret" aria-hidden="true">▾</span>
	</button>

	{#if open}
		<div class="menu" role="menu">
			{#each options as option (option.id)}
				<button
					type="button"
					class="menu-item"
					class:active={option.active}
					role="menuitemradio"
					aria-checked={option.active}
					onclick={() => void select(option.id)}
				>
					<span class="check" aria-hidden="true">{option.active ? '✓' : ''}</span>
					<span class="menu-icon" aria-hidden="true">{option.kind === 'local' ? '💻' : '☁'}</span>
					<span class="menu-text">
						<span class="menu-name">{option.name}</span>
						{#if option.url}
							<span class="menu-url">{option.url}</span>
						{/if}
					</span>
				</button>
			{/each}

			<div class="menu-divider" role="separator"></div>

			<button type="button" class="menu-item manage" role="menuitem" onclick={manage}>
				<span class="check" aria-hidden="true"></span>
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

	.pill:hover:not(:disabled),
	.pill.open {
		background: var(--button-hover);
	}

	.pill:disabled {
		opacity: 0.6;
		cursor: default;
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

	.menu-item:hover {
		background: var(--bg-hover);
	}

	.menu-item.active {
		color: var(--text-default);
	}

	.check {
		width: 12px;
		flex-shrink: 0;
		color: var(--color-success);
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

	.menu-url {
		font-size: 10px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
