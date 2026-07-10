<script lang="ts">
	import { executeSql, getNote, getRelatedNotes, type SqlQueryResult } from '$lib/api';
	import type { RelatedNote } from '$lib/api';
	import TocPanel from '$lib/components/TocPanel.svelte';
	import { buildBacklinksQuery, buildOutgoingLinksQuery, buildRailMetadata } from '$lib/right-rail';
	import type { RailTab } from '$lib/right-dock';
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	type RailLink = { path: string; label: string };

	let { collapsed = false, activeTab = 'metadata' }: { collapsed?: boolean; activeTab?: RailTab } =
		$props();
	let backlinks = $state<SqlQueryResult>(emptySqlResult());
	let outgoingLinks = $state<SqlQueryResult>(emptySqlResult());
	let relatedNotes = $state<RelatedNote[]>([]);
	let metadata = $state<Record<string, unknown> | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadToken = 0;

	const backlinkItems = $derived.by(() => toRailLinks(backlinks.rows, 'source_path', 'source_title'));
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

		const [backlinksResult, outgoingResult, noteResult, relatedResult] = await Promise.allSettled([
			executeSql(vault, buildBacklinksQuery(path)),
			executeSql(vault, buildOutgoingLinksQuery(path)),
			getNote(vault, path),
			getRelatedNotes(vault, path)
		]);

		if (token !== loadToken || tabStore.selectedPath !== path || vaultStore.currentVault !== vault) {
			return;
		}

		backlinks = backlinksResult.status === 'fulfilled' ? backlinksResult.value : emptySqlResult();
		outgoingLinks = outgoingResult.status === 'fulfilled' ? outgoingResult.value : emptySqlResult();
		relatedNotes = relatedResult.status === 'fulfilled' ? relatedResult.value.related : [];
		metadata = buildRailMetadata(
			summary,
			noteResult.status === 'fulfilled' ? noteResult.value.frontmatter : null
		);

		if (
			backlinksResult.status === 'rejected' ||
			outgoingResult.status === 'rejected' ||
			noteResult.status === 'rejected' ||
			relatedResult.status === 'rejected'
		) {
			error = 'Some context is unavailable.';
			console.error('Failed to load right rail data', {
				backlinks: backlinksResult.status === 'rejected' ? backlinksResult.reason : null,
				outgoingLinks: outgoingResult.status === 'rejected' ? outgoingResult.reason : null,
				note: noteResult.status === 'rejected' ? noteResult.reason : null,
				related: relatedResult.status === 'rejected' ? relatedResult.reason : null
			});
		}

		loading = false;
	}

	function clearRail() {
		loadToken += 1;
		backlinks = emptySqlResult();
		outgoingLinks = emptySqlResult();
		relatedNotes = [];
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
		<div class="rail-content">
			{#if !tabStore.selectedPath}
				<div class="rail-empty">Select a note to see metadata, links, and a table of contents.</div>
			{:else}
				{#if loading}
					<div class="rail-status">Refreshing…</div>
				{/if}
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

						<div class="links-group">
							<h3 class="links-heading">Relevant ({relatedNotes.length})</h3>
							{#if relatedNotes.length === 0}
								<div class="section-empty">No related notes</div>
							{:else}
								<div class="link-list">
									{#each relatedNotes as item (item.path)}
										<button class="link-item" type="button" onclick={() => navigateTo(item.path)}>
											<span class="link-label">{item.title}</span>
											<span class="link-path">{item.path}</span>
											<span class="related-signals">
												{#if item.directly_linked}<span class="related-tag">linked</span>{/if}
												{#if item.shared_neighbors > 0}<span class="related-tag"
														>{item.shared_neighbors} shared</span
													>{/if}
											</span>
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

	.rail-status,
	.section-empty,
	.rail-empty {
		color: var(--text-muted);
		font-size: 12px;
	}

	.rail-status {
		padding: 8px 16px 0;
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

	.links-section {
		padding: 8px 0;
	}

	.metadata-list {
		margin: var(--space-3);
		padding: var(--space-3) 0 var(--space-1);
		border: 1px solid var(--border-subtle);
		border-radius: var(--radius-lg);
		background: var(--bg-surface);
		box-shadow: var(--shadow-card);
	}

	.metadata-row {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 0 var(--space-3) 10px;
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
		border-radius: var(--radius-pill);
		background: var(--accent-bg);
		color: var(--accent-text);
		font-size: 12px;
	}

	.links-group + .links-group {
		border-top: 1px solid var(--border-subtle);
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

	.related-signals {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 2px;
	}

	.related-tag {
		padding: 1px 6px;
		border-radius: 999px;
		background: var(--accent-bg);
		color: var(--accent-text);
		font-size: 11px;
	}

	.section-empty {
		padding: 0 16px 12px;
	}
 </style>
