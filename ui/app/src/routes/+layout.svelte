<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Notesmith</title>
</svelte:head>

{@render children()}

<style>
	:global(body) {
		margin: 0;
		font-family:
			-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
		background: var(--bg-primary, #1e1e1e);
		color: var(--text-primary, #e0e0e0);
	}

	:global(*) {
		box-sizing: border-box;
	}
</style>
