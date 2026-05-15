<script lang="ts">
	import { executeSql, getNote, type SqlQueryResult } from '$lib/api';
	import { buildBacklinksQuery, buildOutgoingLinksQuery, buildRailMetadata } from '$lib/right-rail';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	type RailLink = { path: string; label: string };

	let collapsed = $state(false);
	let metadataOpen = $state(true);
	let backlinksOpen = $state(true);
	let outgoingOpen = $state(true);
	let backlinks = $state<SqlQueryResult>(emptySqlResult());
	let outgoingLinks = $state<SqlQueryResult>(emptySqlResult());
	let metadata = $state<Record<string, unknown> | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadToken = 0;

	const backlinkItems = $derived.by(() => toRailLinks(backlinks.rows, 'backlink_path', 'source_title'));
	const outgoingItems = $derived.by(() => toRailLinks(outgoingLinks.rows, 'target_path', 'target'));
	const metadataEntries = $derived.by(() => Object.entries(metadata ?? {}));

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

	export function refresh() {
		if (tabStore.selectedPath) {
			void loadRailData(tabStore.selectedPath);
		}
	}

	export function toggle() {
		collapsed = !collapsed;
	}
</script>

<div class="rail-shell" class:collapsed>
	<button
		class="rail-toggle"
		type="button"
		aria-expanded={!collapsed}
		aria-label={collapsed ? 'Expand right rail' : 'Collapse right rail'}
		onclick={toggle}
	>
		{collapsed ? '◀' : '▶'}
	</button>

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

		{#if !tabStore.selectedPath}
			<div class="rail-empty">Select a note to see metadata and links.</div>
		{:else}
			{#if error}
				<div class="rail-error">{error}</div>
			{/if}

			<section class="rail-section">
				<button
					class="section-toggle"
					type="button"
					aria-expanded={metadataOpen}
					onclick={() => (metadataOpen = !metadataOpen)}
				>
					<span>Metadata</span>
					<span>{metadataOpen ? '▾' : '▸'}</span>
				</button>

				{#if metadataOpen}
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
				{/if}
			</section>

			<section class="rail-section">
				<button
					class="section-toggle"
					type="button"
					aria-expanded={backlinksOpen}
					onclick={() => (backlinksOpen = !backlinksOpen)}
				>
					<span>Backlinks</span>
					<span>{backlinksOpen ? '▾' : '▸'}</span>
				</button>

				{#if backlinksOpen}
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
				{/if}
			</section>

			<section class="rail-section">
				<button
					class="section-toggle"
					type="button"
					aria-expanded={outgoingOpen}
					onclick={() => (outgoingOpen = !outgoingOpen)}
				>
					<span>Outgoing Links</span>
					<span>{outgoingOpen ? '▾' : '▸'}</span>
				</button>

				{#if outgoingOpen}
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
				{/if}
			</section>
		{/if}
	</div>
</div>

<style>
	.rail-shell {
		position: relative;
		width: 260px;
		height: 100%;
		transition: width 180ms ease;
		flex: 0 0 auto;
		overflow: visible;
	}

	.rail-shell.collapsed {
		width: 0;
	}

	.rail-toggle {
		position: absolute;
		top: 12px;
		left: -28px;
		width: 28px;
		height: 32px;
		border-radius: 8px 0 0 8px;
		border: 1px solid var(--ns-border);
		border-right: none;
		background: var(--ns-sidebar-bg);
		color: inherit;
		z-index: 2;
	}

	.rail-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--ns-sidebar-bg);
		border-left: 1px solid var(--ns-border);
		overflow: hidden auto;
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
		border-bottom: 1px solid var(--ns-border);
	}

	.rail-header h2 {
		margin: 0;
		font-size: 14px;
	}

	.rail-header p {
		margin: 4px 0 0;
		color: var(--ns-text-muted);
		font-size: 12px;
		word-break: break-word;
	}

	.rail-status,
	.section-empty,
	.rail-empty {
		color: var(--ns-text-muted);
		font-size: 12px;
	}

	.rail-empty,
	.rail-error {
		padding: 12px 16px;
		border-bottom: 1px solid var(--ns-border);
	}

	.rail-error {
		color: var(--ns-danger-text-muted);
		background: var(--ns-danger-bg-muted);
	}

	.rail-section {
		border-bottom: 1px solid var(--ns-border);
	}

	.section-toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 12px 16px;
		border: none;
		border-radius: 0;
		background: transparent;
		color: var(--ns-text);
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	.section-toggle:hover,
	.link-item:hover {
		background: var(--ns-surface-hover);
	}

	.metadata-list,
	.link-list {
		display: flex;
		flex-direction: column;
		padding-bottom: 8px;
	}

	.metadata-row {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 0 16px 10px;
	}

	.metadata-key {
		color: var(--ns-text-muted);
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
		background: var(--ns-accent-surface);
		color: var(--ns-accent-surface-text);
		font-size: 12px;
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
		color: var(--ns-text);
		text-align: left;
	}

	.link-label {
		font-size: 13px;
		font-weight: 600;
	}

	.link-path {
		color: var(--ns-text-muted);
		font-size: 12px;
		word-break: break-word;
	}

	.section-empty {
		padding: 0 16px 12px;
	}
</style>
