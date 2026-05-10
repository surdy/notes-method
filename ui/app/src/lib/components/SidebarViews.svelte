<script lang="ts">
	import type { SidebarViewConfig, SqlQueryResult } from '$lib/api';
	import { executeSql, getSidebarViews } from '$lib/api';
	import FileTree from './FileTree.svelte';
	import SidebarViewList from './SidebarViewList.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let views = $state<SidebarViewConfig[]>([]);
	let activeViewId = $state('files');
	let badges = $state<Record<string, number>>({});
	let viewData = $state<Record<string, SqlQueryResult>>({});
	let loading = $state<Record<string, boolean>>({});
	let errors = $state<Record<string, string>>({});

	$effect(() => {
		const vault = vaultStore.currentVault;
		if (!vault) return;

		activeViewId = 'files';
		views = [];
		badges = {};
		viewData = {};
		loading = {};
		errors = {};
		void loadViews(vault);
	});

	async function loadViews(vault: string) {
		try {
			const loadedViews = await getSidebarViews(vault);
			if (vault !== vaultStore.currentVault) return;

			views = loadedViews;
			for (const view of loadedViews) {
				if (view.badge_query) {
					void loadBadge(vault, view);
				}
			}
		} catch (error) {
			console.error('Failed to load sidebar views', error);
		}
	}

	async function loadBadge(vault: string, view: SidebarViewConfig) {
		if (!view.badge_query) return;
		try {
			const result = await executeSql(vault, view.badge_query);
			if (vault !== vaultStore.currentVault) return;

			const firstColumn = result.columns[0];
			const firstValue = firstColumn ? result.rows[0]?.[firstColumn] : undefined;
			badges[view.id] = parseBadgeValue(firstValue);
		} catch {
			if (vault !== vaultStore.currentVault) return;
			badges[view.id] = 0;
		}
	}

	async function selectView(viewId: string) {
		activeViewId = viewId;
		if (viewId === 'files') return;

		const view = views.find((candidate) => candidate.id === viewId);
		if (!view || viewData[viewId]) return;
		await loadViewData(view);
	}

	async function loadViewData(view: SidebarViewConfig) {
		const vault = vaultStore.currentVault;
		if (!vault) return;

		loading[view.id] = true;
		delete errors[view.id];
		try {
			const result = await executeSql(vault, view.data_source);
			if (vault !== vaultStore.currentVault) return;
			viewData[view.id] = result;
		} catch (error) {
			if (vault !== vaultStore.currentVault) return;
			errors[view.id] =
				error instanceof Error ? error.message : `Failed to load view ${view.id}`;
		} finally {
			loading[view.id] = false;
		}
	}

	function parseBadgeValue(value: unknown): number {
		if (typeof value === 'number' && Number.isFinite(value)) return value;
		if (typeof value === 'string') {
			const parsed = Number(value);
			return Number.isFinite(parsed) ? parsed : 0;
		}
		return 0;
	}

	export function refresh() {
		const vault = vaultStore.currentVault;
		if (!vault) return;

		for (const view of views) {
			if (view.badge_query) {
				void loadBadge(vault, view);
			}
		}

		if (activeViewId === 'files') return;
		const activeView = views.find((candidate) => candidate.id === activeViewId);
		if (activeView) {
			void loadViewData(activeView);
		}
	}
</script>

<div class="sidebar-views">
	<div class="tab-bar">
		<button
			class="tab-button"
			class:active={activeViewId === 'files'}
			onclick={() => selectView('files')}
			type="button"
		>
			<span class="tab-icon">📁</span>
			<span class="tab-label">Files</span>
		</button>

		{#each views as view (view.id)}
			<button
				class="tab-button"
				class:active={activeViewId === view.id}
				onclick={() => selectView(view.id)}
				type="button"
			>
				<span class="tab-icon">{view.icon}</span>
				<span class="tab-label">{view.name}</span>
				{#if view.badge_query}
					<span class="tab-badge">{badges[view.id] ?? 0}</span>
				{/if}
			</button>
		{/each}
	</div>

	<div class="view-content">
		{#if activeViewId === 'files'}
			{#if vaultStore.loading && vaultStore.notes.length === 0}
				<div class="loading-indicator">Loading...</div>
			{:else if vaultStore.error}
				<div class="error-indicator">{vaultStore.error}</div>
			{:else}
				<FileTree node={vaultStore.tree} />
			{/if}
		{:else}
			{@const activeView = views.find((candidate) => candidate.id === activeViewId)}
			{#if activeView}
				<SidebarViewList
					view={activeView}
					result={viewData[activeView.id]}
					loading={loading[activeView.id] ?? false}
					error={errors[activeView.id] ?? null}
				/>
			{/if}
		{/if}
	</div>
</div>

<style>
	.sidebar-views {
		display: flex;
		flex: 1;
		flex-direction: column;
		min-height: 0;
	}

	.tab-bar {
		display: flex;
		gap: 6px;
		padding: 8px;
		overflow-x: auto;
		border-bottom: 1px solid var(--border-color, #333);
	}

	.tab-button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		border: 1px solid transparent;
		border-radius: 8px;
		background: transparent;
		color: var(--text-secondary, #cccccc);
		white-space: nowrap;
		cursor: pointer;
	}

	.tab-button:hover {
		background: var(--hover-bg, #2a2d2e);
	}

	.tab-button.active {
		background: var(--selected-bg, #094771);
		color: white;
		border-color: color-mix(in srgb, var(--selected-bg, #094771) 70%, white 30%);
	}

	.tab-icon {
		font-size: 13px;
	}

	.tab-label {
		font-size: 12px;
		font-weight: 500;
	}

	.tab-badge {
		min-width: 18px;
		padding: 1px 6px;
		border-radius: 999px;
		background: color-mix(in srgb, currentColor 18%, transparent);
		font-size: 11px;
		text-align: center;
	}

	.view-content {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.loading-indicator,
	.error-indicator {
		padding: 16px;
		text-align: center;
		color: var(--text-muted, #888);
	}

	.error-indicator {
		color: #ff6b6b;
	}
</style>
