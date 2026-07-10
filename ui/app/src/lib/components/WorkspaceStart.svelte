<script lang="ts">
	import { executeSql } from '$lib/api';
	import { buildCommands } from '$lib/commands';
	import { getRecentlyViewed } from '$lib/recently-viewed';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import {
		buildRecentList,
		START_ACTIONS,
		type RecentEntry,
		type RecentSource,
		type StartAction
	} from '$lib/workspace-start';

	let { onOpenQuickSwitcher }: { onOpenQuickSwitcher?: () => void } = $props();

	const RECENT_LIMIT = 5;

	let recents = $state<RecentEntry[]>([]);

	const commands = $derived.by(() =>
		buildCommands(vaultStore.currentVault, (path) => tabStore.selectNote(path))
	);

	$effect(() => {
		const vault = vaultStore.currentVault;
		if (!vault) {
			recents = [];
			return;
		}
		void loadRecents(vault);
	});

	async function loadRecents(vault: string) {
		const viewed: RecentSource[] = getRecentlyViewed(vault, RECENT_LIMIT).map((entry) => ({
			path: entry.path,
			title: entry.title,
			timestamp: entry.timestamp
		}));

		let edited: RecentSource[] = [];
		try {
			const result = await executeSql(
				vault,
				`SELECT path, title, updated_at FROM v_notes ORDER BY updated_at DESC LIMIT ${RECENT_LIMIT}`
			);
			edited = result.rows.map((row) => ({
				path: String(row.path ?? ''),
				title: String(row.title ?? row.path ?? ''),
				updatedAt: String(row.updated_at ?? '')
			}));
		} catch (err) {
			console.error('Failed to load recent notes for workspace start', err);
		}

		recents = buildRecentList(viewed, edited, RECENT_LIMIT);
	}

	function runAction(action: StartAction) {
		if (action.command === 'quick-switcher') {
			onOpenQuickSwitcher?.();
			return;
		}
		void commands.find((command) => command.id === action.command)?.execute();
	}

	function openRecent(path: string) {
		tabStore.selectNote(path);
	}
</script>

<div class="workspace-start">
	<div class="ws-inner">
		<h1 class="ws-title">{vaultStore.currentVault || 'Notesmith'}</h1>
		<p class="ws-subtitle">Pick up where you left off, or start something new.</p>

		<div class="ws-actions">
			{#each START_ACTIONS as action (action.command)}
				<button
					class="ws-action"
					class:primary={action.primary}
					type="button"
					onclick={() => runAction(action)}
				>
					<span class="ws-action-icon" aria-hidden="true">{action.icon}</span>
					<span class="ws-action-label">{action.label}</span>
					<kbd class="ws-action-kbd">{action.shortcut}</kbd>
				</button>
			{/each}
		</div>

		{#if recents.length > 0}
			<div class="ws-recent">
				<div class="ws-recent-label">Recent</div>
				{#each recents as item (item.path)}
					<button class="ws-recent-item" type="button" onclick={() => openRecent(item.path)}>
						<span class="ws-recent-icon" aria-hidden="true">📄</span>
						<span class="ws-recent-title">{item.title}</span>
						{#if item.label}<span class="ws-recent-meta">{item.label}</span>{/if}
					</button>
				{/each}
			</div>
		{/if}
	</div>
</div>

<style>
	.workspace-start {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 24px 32px;
		overflow-y: auto;
	}

	.ws-inner {
		width: 100%;
		max-width: 560px;
	}

	.ws-title {
		margin: 0 0 6px;
		font-size: 30px;
		line-height: 1.1;
		font-weight: 650;
		color: var(--text-default);
	}

	.ws-subtitle {
		margin: 0 0 22px;
		font-size: 14px;
		color: var(--text-muted);
	}

	.ws-actions {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
		margin-bottom: 26px;
	}

	.ws-action {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 14px;
		border: 1px solid var(--border-default);
		border-radius: 10px;
		background: transparent;
		color: var(--text-default);
		font-size: 14px;
		text-align: left;
		cursor: pointer;
		transition:
			background-color 120ms ease,
			border-color 120ms ease;
	}

	.ws-action:hover {
		background: var(--bg-hover);
		border-color: var(--accent);
	}

	.ws-action:focus-visible {
		outline: none;
		border-color: var(--accent);
		box-shadow: inset 0 0 0 1px var(--accent);
	}

	.ws-action.primary {
		border-color: var(--accent);
		background: var(--accent-bg);
	}

	.ws-action-icon {
		width: 22px;
		font-size: 16px;
		text-align: center;
	}

	.ws-action-label {
		flex: 1;
		min-width: 0;
	}

	.ws-action-kbd {
		flex-shrink: 0;
		padding: 2px 6px;
		border: 1px solid var(--border-default);
		border-radius: 5px;
		background: var(--bg-hover);
		color: var(--text-muted);
		font-family: var(--font-mono);
		font-size: 11px;
	}

	.ws-recent {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.ws-recent-label {
		margin: 0 0 8px 2px;
		font-size: 11px;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.ws-recent-item {
		display: flex;
		align-items: center;
		gap: 9px;
		padding: 7px 10px;
		border: none;
		border-radius: 8px;
		background: transparent;
		color: var(--text-default);
		font-size: 13.5px;
		text-align: left;
		cursor: pointer;
	}

	.ws-recent-item:hover {
		background: var(--bg-hover);
	}

	.ws-recent-item:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 1px var(--accent);
	}

	.ws-recent-icon {
		flex-shrink: 0;
	}

	.ws-recent-title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ws-recent-meta {
		margin-left: auto;
		flex-shrink: 0;
		font-size: 11px;
		color: var(--text-muted);
	}
</style>
