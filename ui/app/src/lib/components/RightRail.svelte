<script lang="ts">
	import { onMount } from 'svelte';
	import { executeSql, getNote, type SqlQueryResult } from '$lib/api';
	import TocPanel from '$lib/components/TocPanel.svelte';
	import AgentPanel from '$lib/components/AgentPanel.svelte';
	import { isAgentRunnerAvailable } from '$lib/agent-bridge';
	import { buildBacklinksQuery, buildOutgoingLinksQuery, buildRailMetadata } from '$lib/right-rail';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	type RailLink = { path: string; label: string };
	type RailTab = 'metadata' | 'links' | 'toc' | 'agent';

	let { collapsed = false }: { collapsed?: boolean } = $props();
	let activeTab = $state<RailTab>('metadata');
	let agentAvailable = $state(false);
	let backlinks = $state<SqlQueryResult>(emptySqlResult());
	let outgoingLinks = $state<SqlQueryResult>(emptySqlResult());
	let metadata = $state<Record<string, unknown> | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadToken = 0;

	const backlinkItems = $derived.by(() => toRailLinks(backlinks.rows, 'source_path', 'source_title'));
	const outgoingItems = $derived.by(() => toRailLinks(outgoingLinks.rows, 'target_path', 'target'));
	const metadataEntries = $derived.by(() => Object.entries(metadata ?? {}));

	onMount(() => {
		agentAvailable = isAgentRunnerAvailable();
		activeTab = loadTab();
	});

	$effect(() => {
		const path = tabStore.selectedPath;
		if (!path) {
			clearRail();
			return;
		}

		void loadRailData(path);
	});

	async function loadRailData(path: string) {
		const token = ++loadToken;
		const vault = vaultStore.currentVault;
		const summary = vaultStore.notes.find((note) => note.path === path);

		loading = true;
		error = null;

		const [backlinksResult, outgoingResult, noteResult] = await Promise.allSettled([
			executeSql(vault, buildBacklinksQuery(path)),
			executeSql(vault, buildOutgoingLinksQuery(path)),
			getNote(vault, path)
		]);

		if (token !== loadToken || tabStore.selectedPath !== path || vaultStore.currentVault !== vault) {
			return;
		}

		backlinks = backlinksResult.status === 'fulfilled' ? backlinksResult.value : emptySqlResult();
		outgoingLinks = outgoingResult.status === 'fulfilled' ? outgoingResult.value : emptySqlResult();
		metadata = buildRailMetadata(
			summary,
			noteResult.status === 'fulfilled' ? noteResult.value.frontmatter : null
		);

		if (
			backlinksResult.status === 'rejected' ||
			outgoingResult.status === 'rejected' ||
			noteResult.status === 'rejected'
		) {
			error = 'Some context is unavailable.';
			console.error('Failed to load right rail data', {
				backlinks: backlinksResult.status === 'rejected' ? backlinksResult.reason : null,
				outgoingLinks: outgoingResult.status === 'rejected' ? outgoingResult.reason : null,
				note: noteResult.status === 'rejected' ? noteResult.reason : null
			});
		}

		loading = false;
	}

	function clearRail() {
		loadToken += 1;
		backlinks = emptySqlResult();
		outgoingLinks = emptySqlResult();
		metadata = null;
		loading = false;
		error = null;
	}

	function emptySqlResult(): SqlQueryResult {
		return { columns: [], rows: [] };
	}

	function toRailLinks(
		rows: Record<string, unknown>[],
		pathKey: string,
		labelKey: string
	): RailLink[] {
		return rows
			.map((row) => {
				const path = readString(row[pathKey]);
				if (!path) {
					return null;
				}

				return {
					path,
					label: readString(row[labelKey]) ?? path
				};
			})
			.filter((item): item is RailLink => item !== null);
	}

	function readString(value: unknown): string | null {
		return typeof value === 'string' && value.length > 0 ? value : null;
	}

	function navigateTo(path: string) {
		tabStore.selectNote(path);
	}

	function railTabKey(): string | null {
		const vault = vaultStore.currentVault;
		if (!vault) return null;
		return `notesmith:rail-tab:${vault}`;
	}

	function loadTab(): RailTab {
		try {
			const key = railTabKey();
			if (!key) return 'metadata';
			const saved = localStorage.getItem(key);
			if (saved === 'metadata' || saved === 'links' || saved === 'toc') {
				return saved;
			}
			if (saved === 'agent' && agentAvailable) {
				return saved;
			}
		} catch {}

		return 'metadata';
	}

	function setTab(tab: RailTab) {
		activeTab = tab;
		try {
			const key = railTabKey();
			if (!key) return;
			localStorage.setItem(key, tab);
		} catch {}
	}

	function handleScrollTo(from: number) {
		window.dispatchEvent(new CustomEvent('notesmith:scroll-to', { detail: { from } }));
	}

	export function refresh() {
		if (tabStore.selectedPath) {
			void loadRailData(tabStore.selectedPath);
		}
	}

</script>

<div class="rail-shell" class:collapsed>
	<div class="rail-panel">
		<div class="rail-header">
			<div>
				<h2>Context</h2>
				{#if tabStore.selectedPath}
					<p>{tabStore.selectedPath}</p>
				{/if}
			</div>
			{#if loading}
				<span class="rail-status">Refreshing…</span>
			{/if}
		</div>

		<div class="rail-tab-bar">
			<button
				class="rail-tab"
				class:active={activeTab === 'metadata'}
				type="button"
				onclick={() => setTab('metadata')}
			>
				Metadata
			</button>
			<button
				class="rail-tab"
				class:active={activeTab === 'links'}
				type="button"
				onclick={() => setTab('links')}
			>
				Links
			</button>
			<button
				class="rail-tab"
				class:active={activeTab === 'toc'}
				type="button"
				onclick={() => setTab('toc')}
			>
				TOC
			</button>
			{#if agentAvailable}
				<button
					class="rail-tab"
					class:active={activeTab === 'agent'}
					type="button"
					onclick={() => setTab('agent')}
				>
					Agent
				</button>
			{/if}
		</div>

		<div class="rail-content">
			{#if activeTab === 'agent'}
				<AgentPanel />
			{:else if !tabStore.selectedPath}
				<div class="rail-empty">Select a note to see metadata, links, and a table of contents.</div>
			{:else}
				{#if error}
					<div class="rail-error">{error}</div>
				{/if}

				{#if activeTab === 'metadata'}
					{#if metadataEntries.length === 0}
						<div class="section-empty">No metadata</div>
					{:else}
						<div class="metadata-list">
							{#each metadataEntries as [key, value] (key)}
								<div class="metadata-row">
									<span class="metadata-key">{key}</span>
									{#if Array.isArray(value)}
										<div class="tag-list">
											{#each value as tag (tag)}
												<span class="tag-chip">{tag}</span>
											{/each}
										</div>
									{:else}
										<span class="metadata-value">{String(value)}</span>
									{/if}
								</div>
							{/each}
						</div>
					{/if}
				{:else if activeTab === 'links'}
					<div class="links-section">
						<div class="links-group">
							<h3 class="links-heading">Backlinks ({backlinkItems.length})</h3>
							{#if backlinkItems.length === 0}
								<div class="section-empty">No backlinks</div>
							{:else}
								<div class="link-list">
									{#each backlinkItems as item (item.path)}
										<button class="link-item" type="button" onclick={() => navigateTo(item.path)}>
											<span class="link-label">{item.label}</span>
											<span class="link-path">{item.path}</span>
										</button>
									{/each}
								</div>
							{/if}
						</div>

						<div class="links-group">
							<h3 class="links-heading">Outgoing ({outgoingItems.length})</h3>
							{#if outgoingItems.length === 0}
								<div class="section-empty">No outgoing links</div>
							{:else}
								<div class="link-list">
									{#each outgoingItems as item (item.path)}
										<button class="link-item" type="button" onclick={() => navigateTo(item.path)}>
											<span class="link-label">{item.label}</span>
											<span class="link-path">{item.path}</span>
										</button>
									{/each}
								</div>
							{/if}
						</div>
					</div>
				{:else}
					<TocPanel onScrollTo={handleScrollTo} />
				{/if}
			{/if}
		</div>
	</div>
</div>

<style>
	.rail-shell {
		position: relative;
		width: 100%;
		height: 100%;
		overflow: hidden;
	}

	.rail-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-default);
		overflow: hidden;
		opacity: 1;
		transition:
			opacity 120ms ease,
			visibility 120ms ease;
	}

	.collapsed .rail-panel {
		opacity: 0;
		visibility: hidden;
		pointer-events: none;
	}

	.rail-header {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 14px 16px 12px;
		border-bottom: 1px solid var(--border-default);
	}

	.rail-header h2 {
		margin: 0;
		font-size: 14px;
	}

	.rail-header p {
		margin: 4px 0 0;
		color: var(--text-muted);
		font-size: 12px;
		word-break: break-word;
	}

	.rail-status,
	.section-empty,
	.rail-empty {
		color: var(--text-muted);
		font-size: 12px;
	}

	.rail-tab-bar {
		display: flex;
		border-bottom: 1px solid var(--border-default);
		flex-shrink: 0;
	}

	.rail-tab {
		flex: 1;
		padding: 8px 4px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		border-bottom: 2px solid transparent;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.rail-tab:hover {
		color: var(--text-default);
		background: var(--bg-hover);
	}

	.rail-tab.active {
		color: var(--accent);
		border-bottom-color: var(--accent);
	}

	.rail-content {
		flex: 1;
		overflow-y: auto;
	}

	.rail-empty,
	.rail-error {
		padding: 12px 16px;
	}

	.rail-error {
		color: var(--danger-text-muted);
		background: var(--danger-bg-muted);
	}

	.metadata-list,
	.link-list,
	.links-section {
		display: flex;
		flex-direction: column;
	}

	.metadata-list,
	.links-section {
		padding: 8px 0;
	}

	.metadata-row {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 0 16px 10px;
	}

	.metadata-key {
		color: var(--text-muted);
		font-size: 11px;
		text-transform: uppercase;
	}

	.metadata-value {
		font-size: 13px;
	}

	.tag-list {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.tag-chip {
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--accent-bg);
		color: var(--accent-text);
		font-size: 12px;
	}

	.links-group + .links-group {
		border-top: 1px solid var(--border-default);
	}

	.links-heading {
		margin: 0;
		padding: 12px 16px 8px;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	.link-item {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		width: 100%;
		padding: 10px 16px;
		border: none;
		border-radius: 0;
		background: transparent;
		color: var(--text-default);
		text-align: left;
	}

	.link-item:hover {
		background: var(--bg-hover);
	}

	.link-label {
		font-size: 13px;
		font-weight: 600;
	}

	.link-path {
		color: var(--text-muted);
		font-size: 12px;
		word-break: break-word;
	}

	.section-empty {
		padding: 0 16px 12px;
	}
 </style>
