<!--
	Test harness route for the AI Agent settings surface (ADR 0013, Phase 5).
	Mounts AgentSettings directly so the Playwright flow in
	e2e/agent-settings.spec.ts can drive the real component. The Tauri IPC bridge
	(window.__TAURI__) is stubbed by the test, not here, so this page ships no
	test logic.
-->
<script lang="ts">
	import AgentSettings from '$lib/components/settings/AgentSettings.svelte';
</script>

<div class="harness">
	<AgentSettings />
</div>

<style>
	.harness {
		position: fixed;
		inset: 0;
		overflow-y: auto;
		background: var(--bg-default);
	}
</style>
