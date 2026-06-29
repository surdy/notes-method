<!--
	Test/dev harness for the React git-history island (Option B prototype).
	Renders GitHistoryIsland full-screen with mock vault history so the
	React-in-Svelte integration and native theming can be verified visually
	(and later driven by Playwright). Ships no test logic.
-->
<script lang="ts">
	import GitHistoryIsland from '$lib/git-island/GitHistoryIsland.svelte';
	import { MOCK_COMMITS, mockDiffForCommit } from '$lib/git-island/mockData';
	import type { GitHistoryPanelProps } from '$lib/git-island/types';

	const props: GitHistoryPanelProps = {
		commits: MOCK_COMMITS,
		diffForCommit: mockDiffForCommit
	};
</script>

<div class="harness">
	<header class="harness-bar">
		<strong>git-history island</strong>
		<span>React component hosted in Svelte · mock data</span>
	</header>
	<div class="harness-body">
		<GitHistoryIsland {props} />
	</div>
</div>

<style>
	.harness {
		position: fixed;
		inset: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg-default);
		color: var(--text-default);
	}

	.harness-bar {
		display: flex;
		gap: 10px;
		align-items: baseline;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-default);
		background: var(--bg-panel);
		font-size: 12px;
	}

	.harness-bar span {
		color: var(--text-muted);
	}

	.harness-body {
		flex: 1;
		min-height: 0;
	}
</style>
