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
		border-bottom: 1px solid var(--border-color, #333);
	}

	select {
		width: 100%;
		padding: 6px 8px;
		background: var(--input-bg, #3c3c3c);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border-color, #555);
		border-radius: 4px;
		font-size: 13px;
	}
</style>
