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
	let addParentPath = $state('');
	let addCreateSubfolder = $state(true);
	let addError = $state<string | null>(null);
	let addSaving = $state(false);

	// Rename state
	let renamingVault = $state<string | null>(null);
	let renameValue = $state('');
	let renameError = $state<string | null>(null);

	// Reindex state
	let reindexingVault = $state<string | null>(null);

	// Remove-confirmation modal state.
	let confirmRemove = $state<{ name: string; isOpen: boolean } | null>(null);
	let removing = $state(false);

	// Open-vault tracking. We still display whether a window is open in the UI,
	// but Remove is no longer gated by it — we close the window automatically
	// before unregistering the vault.
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
		addParentPath = capabilities?.vaults_root ?? '';
		addCreateSubfolder = true;
		addError = null;
	}

	function cancelAdd() {
		showAddForm = false;
		addError = null;
	}

	// Strip a trailing slash for clean preview concatenation.
	function trimTrailingSlash(p: string): string {
		return p.replace(/[\\/]+$/, '');
	}

	let computedPath = $derived.by(() => {
		const parent = trimTrailingSlash(addParentPath.trim());
		const name = addName.trim();
		if (!parent) return '';
		if (addCreateSubfolder && name) {
			return `${parent}/${name}`;
		}
		return parent;
	});

	async function browseFolder() {
		if (!tauriBridge) {
			addError = 'Folder picker requires the desktop app.';
			return;
		}
		try {
			const result = (await tauriBridge.invoke('pick_vault_folder')) as string | null;
			if (result) {
				addParentPath = result;
			}
		} catch (e) {
			addError = e instanceof Error ? e.message : 'Folder picker failed.';
		}
	}

	async function submitAdd() {
		const name = addName.trim();
		const parent = trimTrailingSlash(addParentPath.trim());
		if (!name) {
			addError = 'Vault name is required.';
			return;
		}
		if (!parent) {
			addError = addCreateSubfolder
				? 'Pick a parent folder for the new vault.'
				: 'Pick the vault folder.';
			return;
		}
		addSaving = true;
		addError = null;
		try {
			await addVault(name, computedPath, { create: addCreateSubfolder });
			showAddForm = false;
			addName = '';
			addParentPath = '';
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
	function requestRemove(name: string) {
		confirmRemove = { name, isOpen: openVaults.includes(name) };
	}

	function cancelConfirmRemove() {
		if (removing) return;
		confirmRemove = null;
	}

	async function confirmRemoveNow() {
		if (!confirmRemove) return;
		const { name, isOpen } = confirmRemove;
		removing = true;
		error = null;
		try {
			if (isOpen && tauriBridge) {
				try {
					await tauriBridge.invoke('close_vault_window', { vault: name });
				} catch (e) {
					// Non-fatal — proceed with removal even if window close failed.
					console.warn('close_vault_window failed', e);
				}
			}
			await removeVault(name);
			confirmRemove = null;
			await load();
			await refreshOpenVaults();
			toastStore.add(`Vault "${name}" removed.`, 'success');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to remove vault';
		} finally {
			removing = false;
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
	{#if vaults.length === 0 && !showAddForm}
		<div class="empty-state">
			<h3 class="empty-title">No vaults yet</h3>
			<p class="empty-message">
				A vault is a folder where Notesmith stores your notes. Add an existing
				folder or create a new one to get started.
			</p>
			<button type="button" class="btn-add-vault primary" onclick={openAddForm}>
				+ Add Vault
			</button>
		</div>
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
								<span class="vault-name">
									{vault.name}
									{#if openVaults.includes(vault.name)}
										<span class="open-badge" title="A window is open for this vault"
											>open</span
										>
									{/if}
								</span>
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
								disabled={vault.is_default}
								title={vault.is_default
									? 'Cannot remove the default vault. Set a different default first.'
									: 'Remove vault'}
								onclick={() => requestRemove(vault.name)}
							>Remove</button>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/if}

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
				<span class="field-label">
					{addCreateSubfolder ? 'Parent folder' : 'Vault folder'}
				</span>
				<div class="path-row">
					<input
						type="text"
						bind:value={addParentPath}
						placeholder={addCreateSubfolder
							? '/path/to/parent'
							: '/path/to/existing/vault'}
					/>
					{#if tauriBridge}
						<button type="button" class="btn-small" onclick={() => void browseFolder()}>
							Browse…
						</button>
					{/if}
				</div>
			</label>
			<label class="checkbox-row">
				<input type="checkbox" bind:checked={addCreateSubfolder} />
				<span>
					Create new subfolder for vault
					<small class="hint">
						{addCreateSubfolder
							? 'A new folder named after the vault will be created inside the parent folder.'
							: 'Use an existing folder as the vault root.'}
					</small>
				</span>
			</label>
			{#if computedPath}
				<div class="path-preview">
					Vault path: <code>{computedPath}</code>
				</div>
			{/if}
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
	{:else if vaults.length > 0}
		<button type="button" class="btn-add-vault" onclick={openAddForm}>+ Add Vault</button>
	{/if}
{/if}

{#if confirmRemove}
	<div class="modal-backdrop">
		<button
			type="button"
			class="modal-backdrop-button"
			aria-label="Close dialog"
			onclick={cancelConfirmRemove}
		></button>
		<div
			class="modal"
			role="dialog"
			aria-modal="true"
			aria-labelledby="confirm-remove-title"
			tabindex="-1"
		>
			<h3 id="confirm-remove-title" class="modal-title">Remove vault?</h3>
			<p class="modal-body">
				Remove vault <strong>"{confirmRemove.name}"</strong>?
				This only unregisters the vault — your files will not be deleted.
				{#if confirmRemove.isOpen}
					<br /><br />
					The vault window is currently open and will be closed.
				{/if}
			</p>
			<div class="modal-actions">
				<button
					type="button"
					class="btn-revert"
					disabled={removing}
					onclick={cancelConfirmRemove}
				>Cancel</button>
				<button
					type="button"
					class="btn-danger"
					disabled={removing}
					onclick={() => void confirmRemoveNow()}
				>{removing ? 'Removing…' : 'Remove'}</button>
			</div>
		</div>
	</div>
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

	.empty-state {
		padding: 32px 24px;
		text-align: center;
		border: 1px dashed var(--border-strong);
		border-radius: 8px;
		margin: 8px 0 12px;
	}

	.empty-title {
		margin: 0 0 6px;
		font-size: 15px;
		font-weight: 600;
		color: var(--text-default);
	}

	.empty-message {
		margin: 0 auto 16px;
		max-width: 420px;
		color: var(--text-muted);
		font-size: 13px;
		line-height: 1.5;
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
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	.open-badge {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 1px 6px;
		border-radius: 3px;
		background: var(--surface-translucent-subtle);
		color: var(--text-muted);
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

	.path-row {
		display: flex;
		align-items: stretch;
		gap: 6px;
		max-width: 480px;
	}

	.path-row input {
		flex: 1;
		max-width: none;
	}

	.checkbox-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		margin-bottom: 10px;
		font-size: 13px;
		color: var(--text-default);
		cursor: pointer;
	}

	.checkbox-row input[type='checkbox'] {
		margin-top: 3px;
		cursor: pointer;
	}

	.checkbox-row .hint {
		display: block;
		font-size: 11px;
		color: var(--text-muted);
		margin-top: 2px;
	}

	.path-preview {
		margin-bottom: 10px;
		font-size: 11px;
		color: var(--text-muted);
	}

	.path-preview code {
		font-family: var(--font-mono, monospace);
		background: var(--surface-translucent-subtle);
		padding: 2px 6px;
		border-radius: 3px;
		color: var(--text-default);
	}

	.add-actions {
		display: flex;
		gap: 6px;
	}

	.btn-save,
	.btn-revert,
	.btn-danger {
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

	.btn-save:disabled,
	.btn-danger:disabled,
	.btn-revert:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-revert {
		background: transparent;
		color: var(--text-muted);
	}

	.btn-revert:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.btn-danger {
		background: var(--color-danger);
		border-color: var(--color-danger);
		color: var(--text-inverse);
	}

	.btn-danger:hover:not(:disabled) {
		opacity: 0.9;
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

	.btn-add-vault.primary {
		border-style: solid;
		border-color: var(--accent-bg);
		background: var(--accent-bg);
		color: var(--text-inverse);
		font-weight: 500;
	}

	.btn-add-vault.primary:hover {
		background: var(--accent-hover);
		border-color: var(--accent-hover);
		color: var(--text-inverse);
	}

	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.modal-backdrop-button {
		position: absolute;
		inset: 0;
		background: transparent;
		border: none;
		padding: 0;
		margin: 0;
		cursor: default;
	}

	.modal {
		position: relative;
		z-index: 1;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
		border-radius: 8px;
		padding: 20px 24px;
		max-width: 420px;
		width: calc(100% - 32px);
		box-shadow: 0 10px 32px rgba(0, 0, 0, 0.4);
	}

	.modal-title {
		margin: 0 0 12px;
		font-size: 15px;
		font-weight: 600;
		color: var(--text-default);
	}

	.modal-body {
		margin: 0 0 20px;
		font-size: 13px;
		line-height: 1.5;
		color: var(--text-default);
	}

	.modal-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}
</style>
