<!--
	GitHistoryIsland — mounts the React `GitHistoryPanel` inside a Svelte host.

	This is the "Option B" boundary: Notesmith stays Svelte, but the git
	history/diff UI is the shared React component (eventually `@surdy/git-history-react`,
	keeping @pierre/diffs). React + react-dom are lazy-imported so they only load
	when this island is actually rendered. The root is cleanly unmounted on destroy,
	and props changes re-render the React tree via $effect.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { Root } from 'react-dom/client';
	import type { GitHistoryPanelProps } from './types';

	let { props }: { props: GitHistoryPanelProps } = $props();

	let container: HTMLDivElement;
	let root: Root | undefined;
	let renderFn: ((p: GitHistoryPanelProps) => void) | undefined = $state(undefined);

	onMount(() => {
		let active = true;

		void (async () => {
			const [{ createRoot }, React, { GitHistoryPanel }] = await Promise.all([
				import('react-dom/client'),
				import('react'),
				import('./GitHistoryPanel')
			]);
			if (!active) return;
			root = createRoot(container);
			renderFn = (p: GitHistoryPanelProps) =>
				root?.render(React.createElement(GitHistoryPanel, p));
			renderFn(props);
		})();

		return () => {
			active = false;
			root?.unmount();
			root = undefined;
			renderFn = undefined;
		};
	});

	// Re-render the React tree whenever props change (after the root is ready).
	$effect(() => {
		renderFn?.(props);
	});
</script>

<div bind:this={container} class="react-island"></div>

<style>
	.react-island {
		width: 100%;
		height: 100%;
		min-height: 0;
	}
</style>
