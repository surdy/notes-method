<!--
	Test harness route for the agent chat panel. Mounts AgentPanel with a fixed
	vault so the Playwright flow in e2e/agent-chat.spec.ts can drive the real
	component. The agent transport (window.__TAURI__) and the transcript HTTP
	endpoints are stubbed by the test, not here, so this page ships no test logic.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import AgentPanel from '$lib/components/agent/AgentPanel.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	onMount(() => {
		vaultStore.currentVault = 'harness';
	});
</script>

<div class="harness">
	<AgentPanel />
</div>

<style>
	.harness {
		position: fixed;
		inset: 0;
		width: 360px;
		height: 100vh;
		background: var(--bg-default);
	}
</style>
