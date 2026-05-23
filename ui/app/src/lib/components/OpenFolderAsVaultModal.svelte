<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { listVaults } from '$lib/api';
	import {
		defaultNameFromPath,
		resolveTauri,
		validateVaultName,
		type TauriBridge
	} from '$lib/open-folder-as-vault';

	let {
		onClose,
		bridge = resolveTauri()
	}: {
		onClose: () => void;
		bridge?: TauriBridge | null;
	} = $props();

	let phase = $state<'picking' | 'naming' | 'submitting'>('picking');
	let path = $state('');
	let name = $state('');
	let existing = $state<string[]>([]);
	let error = $state<string | null>(null);
	let nameInput = $state<HTMLInputElement | undefined>(undefined);

	onMount(() => {
		void start();
	});

	async function start() {
		if (!bridge) {
			error = 'The folder picker is only available inside the Notesmith desktop app.';
			phase = 'naming';
			return;
		}
		try {
			existing = (await listVaults()).map((v) => v.name);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load existing vaults';
		}
		try {
			const picked = (await bridge.invoke('pick_vault_folder')) as string | null;
			if (!picked) {
				onClose();
				return;
			}
			path = picked;
			name = defaultNameFromPath(picked);
			phase = 'naming';
			await tick();
			nameInput?.focus();
			nameInput?.select();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Folder picker failed';
			phase = 'naming';
		}
	}

	async function confirm() {
		const result = validateVaultName(name, existing);
		if (!result.ok) {
			error = result.error.message;
			return;
		}
		if (!bridge) {
			error = 'Cannot register vaults outside the Notesmith desktop app.';
			return;
		}
		if (!path) {
			error = 'No folder selected.';
			return;
		}
		phase = 'submitting';
		error = null;
		try {
			await bridge.invoke('open_folder_as_vault', {
				path,
				displayName: result.value
			});
			onClose();
		} catch (e) {
			error = typeof e === 'string' ? e : e instanceof Error ? e.message : 'Failed to register vault';
			phase = 'naming';
			await tick();
			nameInput?.focus();
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		} else if (event.key === 'Enter' && phase === 'naming') {
			event.preventDefault();
			void confirm();
		}
	}
</script>

<div
	class="modal-backdrop"
	role="dialog"
	aria-modal="true"
	aria-labelledby="open-folder-title"
	tabindex="-1"
	onclick={(event) => event.target === event.currentTarget && onClose()}
	onkeydown={handleKeydown}
>
	<div class="modal-sheet">
		<h2 id="open-folder-title" class="modal-title">Open Folder as Vault</h2>

		{#if phase === 'picking'}
			<p class="modal-body">Choose a folder to register as a new vault…</p>
		{:else}
			<p class="modal-body">
				Folder: <code class="folder-path" title={path}>{path || '(none)'}</code>
			</p>
			<label class="field">
				<span class="field-label">Vault name</span>
				<input
					bind:this={nameInput}
					bind:value={name}
					type="text"
					class="name-input"
					placeholder="e.g. Personal"
					disabled={phase === 'submitting'}
					aria-invalid={error !== null}
					aria-describedby={error ? 'open-folder-error' : undefined}
				/>
			</label>
			{#if error}
				<div id="open-folder-error" class="inline-error" role="alert">{error}</div>
			{/if}
			<div class="modal-actions">
				<button type="button" class="btn-secondary" onclick={onClose}>Cancel</button>
				<button
					type="button"
					class="btn-primary"
					disabled={phase === 'submitting' || !path}
					onclick={() => void confirm()}
				>
					{phase === 'submitting' ? 'Opening…' : 'Open Vault'}
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.modal-sheet {
		background: var(--bg-elevated);
		color: var(--text-default);
		border: 1px solid var(--border-default);
		border-radius: 8px;
		padding: 20px;
		min-width: 420px;
		max-width: 90vw;
		box-shadow: 0 10px 40px rgba(0, 0, 0, 0.4);
	}

	.modal-title {
		margin: 0 0 12px;
		font-size: 16px;
		font-weight: 600;
	}

	.modal-body {
		margin: 0 0 12px;
		font-size: 13px;
		color: var(--text-secondary);
	}

	.folder-path {
		font-family: var(--font-mono);
		background: var(--bg-default);
		padding: 2px 6px;
		border-radius: 3px;
		font-size: 12px;
		word-break: break-all;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 8px;
	}

	.field-label {
		font-size: 11px;
		color: var(--text-muted);
	}

	.name-input {
		padding: 6px 10px;
		border: 1px solid var(--border-input);
		border-radius: 4px;
		background: var(--bg-input);
		color: var(--text-default);
		font-size: 13px;
	}

	.name-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.inline-error {
		color: var(--color-danger);
		font-size: 12px;
		margin-bottom: 8px;
	}

	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 12px;
	}

	.btn-primary,
	.btn-secondary {
		padding: 6px 14px;
		border-radius: 4px;
		font-size: 13px;
		cursor: pointer;
		border: 1px solid var(--border-strong);
	}

	.btn-primary {
		background: var(--accent-bg);
		color: var(--text-inverse);
		border-color: var(--accent-bg);
	}

	.btn-primary:hover:not(:disabled) {
		background: var(--accent-hover);
	}

	.btn-primary:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-secondary {
		background: transparent;
		color: var(--text-muted);
	}

	.btn-secondary:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}
</style>
