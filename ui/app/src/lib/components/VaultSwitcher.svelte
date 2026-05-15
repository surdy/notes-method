<script lang="ts">
	import { tabStore } from '$lib/tab-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let { vaults }: { vaults: string[] } = $props();

	function switchVault(event: Event) {
		const target = event.target as HTMLSelectElement;
		vaultStore.currentVault = target.value;
		tabStore.selectedPath = null;
		void vaultStore.loadNotes();
	}
</script>

<div class="vault-switcher">
	<select onchange={switchVault} value={vaultStore.currentVault}>
		{#each vaults as vault (vault)}
			<option value={vault}>{vault}</option>
		{/each}
	</select>
</div>

<style>
	.vault-switcher {
		padding: 8px 12px;
		border-bottom: 1px solid var(--ns-border);
	}

	select {
		width: 100%;
		padding: 6px 8px;
		background: var(--ns-input-bg);
		color: var(--ns-text);
		border: 1px solid var(--ns-border-input);
		border-radius: 4px;
		font-size: 13px;
	}
</style>
