<script lang="ts">
	import { vaultStore } from '$lib/stores.svelte';

	let { vaults }: { vaults: string[] } = $props();

	const OPEN_FOLDER_EVENT = 'notesmith://open-folder-as-vault';

	function resolveTauri():
		| { invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown> }
		| null {
		if (typeof window === 'undefined') return null;
		const t = (window as unknown as {
			__TAURI__?: { core?: { invoke?: (cmd: string, args: Record<string, unknown>) => Promise<unknown> } };
		}).__TAURI__;
		if (!t?.core?.invoke) return null;
		return { invoke: t.core.invoke };
	}

	async function openVault(vault: string) {
		if (vault === vaultStore.currentVault) return;
		const tauri = resolveTauri();
		if (!tauri) {
			// Dev / browser mode — navigate by setting ?vault=.
			if (typeof window !== 'undefined') {
				const url = new URL(window.location.href);
				url.searchParams.set('vault', vault);
				window.location.href = url.toString();
			}
			return;
		}
		try {
			await tauri.invoke('open_vault_window', { vault });
		} catch (error) {
			console.warn('open_vault_window failed', error);
		}
	}

	function openFolderAsVault() {
		if (typeof window === 'undefined') return;
		// #103 will land a modal that listens for this event. For now the
		// menu route also emits the same event from the Tauri side.
		window.dispatchEvent(new CustomEvent(OPEN_FOLDER_EVENT));
	}
</script>

<div class="vault-switcher" role="group" aria-label="Vaults">
	<ul class="vault-list">
		{#each vaults as vault (vault)}
			{@const isCurrent = vault === vaultStore.currentVault}
			<li>
				<button
					type="button"
					class="vault-row"
					class:current={isCurrent}
					disabled={isCurrent}
					aria-current={isCurrent ? 'true' : undefined}
					onclick={() => void openVault(vault)}
					title={isCurrent ? `${vault} (this window)` : `Open ${vault} in a new window`}
				>
					<span class="vault-name">{vault}</span>
					{#if isCurrent}
						<span class="vault-tag">(this window)</span>
					{/if}
				</button>
			</li>
		{/each}
		<li>
			<button
				type="button"
				class="vault-row open-folder"
				onclick={openFolderAsVault}
				title="Open another folder as a vault"
			>
				<span class="vault-name">Open Folder…</span>
			</button>
		</li>
	</ul>
</div>

<style>
	.vault-switcher {
		padding: 6px 4px;
		border-bottom: 1px solid var(--ns-border);
	}

	.vault-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.vault-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		width: 100%;
		padding: 6px 10px;
		background: transparent;
		color: var(--ns-text);
		border: 1px solid transparent;
		border-radius: 4px;
		font-size: 13px;
		text-align: left;
		cursor: pointer;
	}

	.vault-row:hover:not(:disabled) {
		background: var(--ns-surface-hover);
	}

	.vault-row.current {
		background: var(--ns-surface-active);
		cursor: default;
	}

	.vault-row:disabled {
		opacity: 1;
	}

	.vault-tag {
		font-size: 11px;
		opacity: 0.7;
	}

	.vault-row.open-folder {
		border-top: 1px dashed var(--ns-border);
		margin-top: 4px;
		padding-top: 8px;
		opacity: 0.85;
	}
</style>
