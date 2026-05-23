<script lang="ts">
	import type { SqlQueryResult } from '$lib/api';
	import { tabStore } from '$lib/tab-store.svelte';

	type Row = Record<string, unknown>;
	type SidebarViewListConfig = {
		id: string;
		name: string;
		icon: string;
		group_by?: string;
	};
	type RowGroup = {
		key: string;
		label: string;
		rows: Row[];
	};

	let {
		view,
		result,
		loading = false,
		error = null
	}: {
		view: SidebarViewListConfig;
		result?: SqlQueryResult;
		loading?: boolean;
		error?: string | null;
	} = $props();

	const EMPTY_GROUPS: RowGroup[] = [];

	let groups = $derived.by(() => {
		if (!result || result.rows.length === 0) return EMPTY_GROUPS;
		if (!view.group_by) {
			return [{ key: '__all__', label: '', rows: result.rows }];
		}

		const grouped: RowGroup[] = [];
		const indexByKey = new Map<string, number>();
		for (const row of result.rows) {
			const label = formatGroupValue(row[view.group_by]);
			const key = `${view.group_by}:${label}`;
			const existingIndex = indexByKey.get(key);
			if (existingIndex !== undefined) {
				grouped[existingIndex].rows.push(row);
				continue;
			}

			indexByKey.set(key, grouped.length);
			grouped.push({ key, label, rows: [row] });
		}

		return grouped;
	});

	function openRow(row: Row) {
		const path = row.path;
		if (typeof path !== 'string' || !path) return;
		tabStore.selectNote(path);
	}

	function isOpenable(row: Row): boolean {
		return typeof row.path === 'string' && row.path.length > 0;
	}

	function rowKey(row: Row, index: number): string {
		const path = typeof row.path === 'string' ? row.path : '';
		const title = typeof row.title === 'string' ? row.title : '';
		return `${path}:${title}:${index}`;
	}

	function titleFor(row: Row): string {
		if (typeof row.title === 'string' && row.title.trim()) return row.title;
		if (typeof row.text === 'string' && row.text.trim()) return row.text;
		if (typeof row.path === 'string' && row.path.trim()) {
			const segments = row.path.split('/');
			return segments[segments.length - 1].replace(/\.md$/, '');
		}
		return 'Untitled';
	}

	function pathMeta(row: Row): string | null {
		if (typeof row.path !== 'string' || !row.path) return null;
		return row.path;
	}

	function metadataEntries(row: Row): Array<[string, string]> {
		if (!result) return [];
		return result.columns
			.filter((column) => column !== 'title' && column !== 'path' && column !== view.group_by)
			.map((column) => [column, formatValue(row[column])] as [string, string])
			.filter(([, value]) => value.length > 0);
	}

	function formatGroupValue(value: unknown): string {
		const formatted = formatValue(value);
		return formatted || 'Ungrouped';
	}

	function formatValue(value: unknown): string {
		if (value === null || value === undefined) return '';
		if (typeof value === 'string') return value;
		if (typeof value === 'number' || typeof value === 'boolean') return String(value);
		try {
			return JSON.stringify(value);
		} catch {
			return String(value);
		}
	}
</script>

{#if loading}
	<div class="sidebar-state">Loading…</div>
{:else if error}
	<div class="sidebar-state error">{error}</div>
{:else if !result || result.rows.length === 0}
	<div class="sidebar-state">No results</div>
{:else}
	<div class="view-list">
		{#each groups as group (group.key)}
			{#if view.group_by}
				<div class="group-header">{group.label}</div>
			{/if}

			{#each group.rows as row, index (rowKey(row, index))}
				{@const metadata = metadataEntries(row)}
				<button
					class="view-row"
					class:openable={isOpenable(row)}
					class:selected={typeof row.path === 'string' && tabStore.selectedPath === row.path}
					disabled={!isOpenable(row)}
					onclick={() => openRow(row)}
					type="button"
				>
					<span class="row-title">{titleFor(row)}</span>

					{#if pathMeta(row)}
						<span class="row-path">{pathMeta(row)}</span>
					{/if}

					{#if metadata.length > 0}
						<span class="row-meta">
							{#each metadata as [key, value], metaIndex (`${key}:${metaIndex}`)}
								<span class="meta-pill"><strong>{key}:</strong> {value}</span>
							{/each}
						</span>
					{/if}
				</button>
			{/each}
		{/each}
	</div>
{/if}

<style>
	.view-list {
		display: flex;
		flex-direction: column;
		padding: 4px 0 12px;
	}

	.group-header {
		padding: 10px 12px 6px;
		font-size: 11px;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.view-row {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		width: 100%;
		padding: 8px 12px;
		border: none;
		background: none;
		color: var(--text-secondary);
		text-align: left;
	}

	.view-row.openable {
		cursor: pointer;
	}

	.view-row.openable:hover {
		background: var(--bg-hover);
	}

	.view-row.selected {
		background: var(--bg-selected);
		color: var(--text-inverse);
	}

	.view-row:disabled {
		opacity: 0.85;
	}

	.row-title {
		font-size: 13px;
		font-weight: 500;
	}

	.row-path {
		font-size: 11px;
		color: var(--text-muted);
		word-break: break-word;
	}

	.view-row.selected .row-path {
		color: var(--text-inverse);
	}

	.row-meta {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-top: 2px;
		font-size: 11px;
		color: var(--text-muted);
	}

	.view-row.selected .row-meta {
		color: var(--text-inverse);
	}

	.meta-pill {
		display: inline-flex;
		gap: 4px;
	}

	.sidebar-state {
		padding: 16px;
		text-align: center;
		color: var(--text-muted);
	}

	.sidebar-state.error {
		color: var(--color-danger);
	}
</style>
