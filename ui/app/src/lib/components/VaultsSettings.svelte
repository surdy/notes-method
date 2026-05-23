<script lang="ts">
	import {
		listVaults,
		addVault,
		updateVault,
		removeVault,
		setDefaultVault,
		reindexVault,
		ApiError,
		type VaultInfo,
		type Capabilities
	} from '$lib/api';
	import { toastStore } from '$lib/toast-store.svelte';
	import { onMount, onDestroy } from 'svelte';
	import { resolveTauri } from '$lib/open-folder-as-vault';

	interface Props {
		capabilities: Capabilities | null;
	}

	let { capabilities }: Props = $props();

	let vaults = $state<VaultInfo[]>([]);
	let status = $state<'idle' | 'loading'>('idle');
	let error = $state<string | null>(null);

	// Add vault form
	let showAddForm = $state(false);
	let addName = $state('');
	let addPath = $state('');
	let addError = $state<string | null>(null);
	let addSaving = $state(false);

	// Rename state
	let renamingVault = $state<string | null>(null);
	let renameValue = $state('');
	let renameError = $state<string | null>(null);

	// Reindex state
	let reindexingVault = $state<string | null>(null);

	// Open-vault tracking for issue 103 — disable Remove for vaults with a live window.
	let openVaults = $state<string[]>([]);
	const tauriBridge = resolveTauri();
	let openVaultsPollHandle: ReturnType<typeof setInterval> | null = null;

	async function refreshOpenVaults() {
		if (!tauriBridge) {
			openVaults = [];
			return;
		}
		try {
			const result = (await tauriBridge.invoke('list_open_vaults')) as string[];
			openVaults = Array.isArray(result) ? result : [];
		} catch {
			openVaults = [];
		}
	}

	onMount(() => {
		void refreshOpenVaults();
		if (tauriBridge) {
			// Light polling so the Remove buttons re-enable when the user closes a
			// vault window without leaving Settings.
			openVaultsPollHandle = setInterval(() => void refreshOpenVaults(), 2000);
		}
	});

	onDestroy(() => {
		if (openVaultsPollHandle) {
			clearInterval(openVaultsPollHandle);
			openVaultsPollHandle = null;
		}
	});

	// ── Load ─────────────────────────────────────────────────────
	async function load() {
		status = 'loading';
		error = null;
		try {
			vaults = await listVaults();
			status = 'idle';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load vaults';
			status = 'idle';
		}
	}

	$effect(() => {
		void load();
	});

	// ── Add vault ────────────────────────────────────────────────
	function openAddForm() {
		showAddForm = true;
		addName = '';
		addPath = capabilities?.vaults_root ?? '';
		addError = null;
	}

	function cancelAdd() {
		showAddForm = false;
		addError = null;
	}

	async function submitAdd() {
		if (!addName.trim() || !addPath.trim()) {
			addError = 'Both name and path are required.';
			return;
		}
		addSaving = true;
		addError = null;
		try {
			await addVault(addName.trim(), addPath.trim());
			showAddForm = false;
			addName = '';
			addPath = '';
			await load();
		} catch (e) {
			addError = e instanceof Error ? e.message : 'Failed to add vault';
		} finally {
			addSaving = false;
		}
	}

	// ── Set default ──────────────────────────────────────────────
	async function handleSetDefault(name: string) {
		error = null;
		try {
			await setDefaultVault(name);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to set default vault';
		}
	}

	// ── Rename ───────────────────────────────────────────────────
	function startRename(name: string) {
		renamingVault = name;
		renameValue = name;
		renameError = null;
	}

	function cancelRename() {
		renamingVault = null;
		renameError = null;
	}

	async function submitRename(oldName: string) {
		if (!renameValue.trim() || renameValue.trim() === oldName) {
			cancelRename();
			return;
		}
		renameError = null;
		try {
			await updateVault(oldName, renameValue.trim());
			renamingVault = null;
			await load();
		} catch (e) {
			renameError = e instanceof Error ? e.message : 'Failed to rename vault';
		}
	}

	// ── Remove ───────────────────────────────────────────────────
	async function handleRemove(name: string) {
		error = null;
		if (openVaults.includes(name)) {
			error = `Close the "${name}" window first before removing this vault.`;
			return;
		}
		if (!window.confirm(`Remove vault "${name}"? This only unregisters the vault — your files will not be deleted.`)) {
			return;
		}
		try {
			await removeVault(name);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to remove vault';
		}
	}

	// ── Reindex ──────────────────────────────────────────────────
	async function handleReindex(name: string) {
		error = null;
		reindexingVault = name;
		try {
			const result = await reindexVault(name);
			reindexingVault = null;
			toastStore.add(`Reindexed ${result.notes} notes in "${name}".`, 'success');
		} catch (e) {
			reindexingVault = null;
			error = e instanceof Error ? e.message : 'Failed to reindex vault';
		}
	}
</script>

{#if error}
	<div class="error-banner">{error}</div>
{/if}

{#if status === 'loading'}
	<p class="loading">Loading vaults…</p>
{:else}
	<div class="vaults-list">
		{#each vaults as vault (vault.name)}
			<div class="vault-card" class:is-default={vault.is_default}>
				<div class="vault-main">
					<div class="vault-info">
						{#if renamingVault === vault.name}
							<div class="rename-row">
								<input
									type="text"
									class="rename-input"
									bind:value={renameValue}
									onkeydown={(e) => {
										if (e.key === 'Enter') void submitRename(vault.name);
										if (e.key === 'Escape') cancelRename();
									}}
								/>
								<button
									type="button"
									class="btn-small"
									onclick={() => void submitRename(vault.name)}>Save</button
								>
								<button type="button" class="btn-small muted" onclick={cancelRename}
									>Cancel</button
								>
							</div>
							{#if renameError}
								<span class="inline-error">{renameError}</span>
							{/if}
						{:else}
							<span class="vault-name">{vault.name}</span>
						{/if}
						<span class="vault-path">{vault.path}</span>
					</div>
					<div class="vault-actions">
						<button
							type="button"
							class="default-btn"
							class:active={vault.is_default}
							title={vault.is_default ? 'Default vault' : 'Set as default'}
							onclick={() => {
								if (!vault.is_default) void handleSetDefault(vault.name);
							}}
						>
							{vault.is_default ? '★' : '☆'}
						</button>
						<button
							type="button"
							class="btn-small"
							onclick={() => startRename(vault.name)}
						>Rename</button>
						<button
							type="button"
							class="btn-small"
							disabled={reindexingVault === vault.name}
							onclick={() => void handleReindex(vault.name)}
						>
							{reindexingVault === vault.name ? 'Reindexing…' : 'Reindex'}
						</button>
						<button
							type="button"
							class="btn-small danger"
							disabled={vault.is_default || openVaults.includes(vault.name)}
							title={vault.is_default
								? 'Cannot remove the default vault'
								: openVaults.includes(vault.name)
									? `Close the "${vault.name}" window first`
									: 'Remove vault'}
							onclick={() => void handleRemove(vault.name)}
						>Remove</button>
					</div>
				</div>
			</div>
		{/each}
	</div>

	{#if showAddForm}
		<div class="add-form">
			<h4 class="add-title">Add Vault</h4>
			{#if addError}
				<div class="inline-error">{addError}</div>
			{/if}
			<label class="field">
				<span class="field-label">Vault Name</span>
				<input
					type="text"
					bind:value={addName}
					placeholder="e.g. Personal"
				/>
			</label>
			<label class="field">
				<span class="field-label">Path</span>
				{#if capabilities?.vaults_root}
					<div class="path-prefix">
						<span class="prefix">{capabilities.vaults_root}/</span>
						<input
							type="text"
							bind:value={addPath}
							placeholder="subfolder"
						/>
					</div>
				{:else}
					<input
						type="text"
						bind:value={addPath}
						placeholder="/path/to/vault"
					/>
				{/if}
			</label>
			<div class="add-actions">
				<button
					type="button"
					class="btn-save"
					disabled={addSaving}
					onclick={() => void submitAdd()}
				>
					{addSaving ? 'Adding…' : 'Add Vault'}
				</button>
				<button type="button" class="btn-revert" onclick={cancelAdd}>Cancel</button>
			</div>
		</div>
	{:else}
		<button type="button" class="btn-add-vault" onclick={openAddForm}>+ Add Vault</button>
	{/if}
{/if}

<style>
	.error-banner {
		padding: 10px 0;
		color: var(--color-danger);
		font-size: 13px;
	}

	.loading {
		color: var(--text-muted);
		font-size: 13px;
	}

	.vaults-list {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 12px;
	}

	.vault-card {
		border: 1px solid var(--border-default);
		border-radius: 6px;
		padding: 10px 12px;
		background: var(--bg-secondary);
	}

	.vault-card.is-default {
		border-color: var(--accent-bg);
	}

	.vault-main {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.vault-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.vault-name {
		font-size: 14px;
		font-weight: 500;
	}

	.vault-path {
		font-size: 11px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.vault-actions {
		display: flex;
		gap: 4px;
		align-items: center;
		flex-shrink: 0;
	}

	.default-btn {
		background: none;
		border: none;
		font-size: 16px;
		cursor: pointer;
		padding: 2px 4px;
		color: var(--text-muted);
	}

	.default-btn.active {
		color: var(--color-warning);
		cursor: default;
	}

	.default-btn:hover:not(.active) {
		color: var(--color-warning);
	}

	.btn-small {
		padding: 3px 8px;
		border: 1px solid var(--border-strong);
		border-radius: 4px;
		background: transparent;
		color: var(--text-muted);
		font-size: 11px;
		cursor: pointer;
	}

	.btn-small:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.btn-small:disabled {
		opacity: 0.3;
		cursor: default;
	}

	.btn-small.danger:hover:not(:disabled) {
		color: var(--color-danger);
		border-color: var(--color-danger);
	}

	.btn-small.muted {
		color: var(--text-faint);
	}

	.rename-row {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.rename-input {
		padding: 3px 8px;
		border: 1px solid var(--accent-bg);
		border-radius: 4px;
		background: var(--bg-default);
		color: var(--text-default);
		font-size: 13px;
		width: 160px;
	}

	.inline-error {
		color: var(--color-danger);
		font-size: 11px;
		display: block;
		margin-top: 2px;
	}

	.add-form {
		border: 1px solid var(--border-default);
		border-radius: 6px;
		padding: 12px;
		margin-top: 8px;
	}

	.add-title {
		margin: 0 0 10px;
		font-size: 13px;
		font-weight: 600;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 3px;
		margin-bottom: 10px;
	}

	.field-label {
		font-size: 11px;
		color: var(--text-muted);
	}

	.field input[type='text'] {
		padding: 5px 8px;
		border: 1px solid var(--border-strong);
		border-radius: 4px;
		background: var(--bg-secondary);
		color: var(--text-default);
		font-size: 13px;
		max-width: 360px;
	}

	.field input:focus {
		outline: none;
		border-color: var(--accent-bg);
	}

	.path-prefix {
		display: flex;
		align-items: center;
		gap: 0;
		max-width: 360px;
	}

	.prefix {
		padding: 5px 6px;
		background: var(--surface-translucent-subtle);
		border: 1px solid var(--border-strong);
		border-right: none;
		border-radius: 4px 0 0 4px;
		color: var(--text-muted);
		font-size: 12px;
		white-space: nowrap;
	}

	.path-prefix input {
		border-radius: 0 4px 4px 0 !important;
		flex: 1;
	}

	.add-actions {
		display: flex;
		gap: 6px;
	}

	.btn-save,
	.btn-revert {
		padding: 5px 14px;
		border-radius: 4px;
		border: 1px solid var(--border-strong);
		font-size: 12px;
		cursor: pointer;
	}

	.btn-save {
		background: var(--accent-bg);
		color: var(--text-inverse);
		border-color: var(--accent-bg);
	}

	.btn-save:hover:not(:disabled) {
		background: var(--accent-hover);
	}

	.btn-save:disabled {
		opacity: 0.5;
	}

	.btn-revert {
		background: transparent;
		color: var(--text-muted);
	}

	.btn-revert:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.btn-add-vault {
		padding: 8px 16px;
		border: 1px dashed var(--border-strong);
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
		cursor: pointer;
		margin-top: 8px;
	}

	.btn-add-vault:hover {
		border-color: var(--text-muted);
		color: var(--text-default);
	}
</style>
