<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';
	import '../app.css';
	import { themeStore } from '$lib/theme.svelte';

	let { children } = $props();

	themeStore;
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Notesmith</title>
</svelte:head>

{@render children()}
