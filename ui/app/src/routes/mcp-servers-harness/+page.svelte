<!--
	Test harness route for the MCP server management surface (ADR 0016, #211).
	Mounts McpServers directly so the Playwright flow in
	e2e/mcp-servers.spec.ts can drive the real component. The Tauri IPC bridge
	(window.__TAURI__) is stubbed by the test, not here, so this page ships no
	test logic.
-->
<script lang="ts">
	import McpServers from '$lib/components/settings/McpServers.svelte';
</script>

<div class="harness">
	<McpServers />
</div>

<style>
	.harness {
		position: fixed;
		inset: 0;
		overflow-y: auto;
		background: var(--bg-default);
	}
</style>
