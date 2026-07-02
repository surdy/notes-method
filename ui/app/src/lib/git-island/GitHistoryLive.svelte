<!--
	GitHistoryLive — feeds the React git-history island with real vault data.

	Fetches the commit log from the daemon (`git/log`) and lazily loads each
	commit's diff (`git/diff/{sha}`) as it is selected, caching results. This is
	the production counterpart to the mock harness route: same island, live data.
-->
<script lang="ts">
	import { getCommitDiff, getGitLog } from '$lib/api';
	import GitHistoryIsland from '$lib/git-island/GitHistoryIsland.svelte';
	import type { CommitDiff, GitHistoryPanelProps, GitLogEntry } from '$lib/git-island/types';

	let { vault, limit = 50 }: { vault: string; limit?: number } = $props();

	let commits = $state<GitLogEntry[]>([]);
	let diffCache = $state<Record<string, CommitDiff>>({});
	let loading = $state(true);
	let error = $state<string | null>(null);
	const inFlight = new Set<string>();

	async function loadLog(target: string): Promise<void> {
		loading = true;
		error = null;
		try {
			const entries = await getGitLog(target, limit);
			if (target === vault) commits = entries;
		} catch (e) {
			if (target === vault) {
				error = e instanceof Error ? e.message : String(e);
				commits = [];
			}
		} finally {
			if (target === vault) loading = false;
		}
	}

	async function ensureDiff(sha: string): Promise<void> {
		if (diffCache[sha] || inFlight.has(sha)) return;
		inFlight.add(sha);
		const target = vault;
		try {
			const diff = await getCommitDiff(target, sha);
			if (target === vault) diffCache = { ...diffCache, [sha]: diff };
		} catch {
			// Leave uncached; the panel keeps showing the loading state.
		} finally {
			inFlight.delete(sha);
		}
	}

	// (Re)load whenever the vault changes; reset the per-vault caches.
	$effect(() => {
		const target = vault;
		diffCache = {};
		commits = [];
		void loadLog(target);
	});

	const panelProps: GitHistoryPanelProps = $derived({
		commits,
		diffForCommit: (sha: string) => diffCache[sha],
		onSelectCommit: (sha: string) => void ensureDiff(sha)
	});
</script>

{#if loading && commits.length === 0}
	<p class="ghl-state">Loading history…</p>
{:else if error}
	<p class="ghl-state ghl-state--error">Failed to load history: {error}</p>
{:else if commits.length === 0}
	<p class="ghl-state">No commits yet.</p>
{:else}
	<GitHistoryIsland props={panelProps} />
{/if}

<style>
	.ghl-state {
		margin: 0;
		padding: 24px;
		text-align: center;
		font-size: 13px;
		color: var(--text-muted);
	}

	.ghl-state--error {
		color: var(--color-danger);
	}
</style>
