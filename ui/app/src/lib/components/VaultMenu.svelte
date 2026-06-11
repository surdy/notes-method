<script lang="ts">
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { resolveTauri } from '$lib/open-folder-as-vault';
	import {
		buildVaultMenuModel,
		isBrowserVaultMenu,
		settingsRoute,
		vaultSwitchUrl
	} from '$lib/vault-menu';

	let { vaults, currentVault }: { vaults: string[]; currentVault: string } = $props();

	const OPEN_FOLDER_EVENT = 'notesmith://open-folder-as-vault';

	// Browser-only: the desktop shell exposes these actions through its native OS
	// menu and a window-per-vault model, so the dropdown would only duplicate them.
	const isBrowser = isBrowserVaultMenu(resolveTauri());

	let open = $state(false);
	let menuRef = $state<HTMLDivElement | null>(null);
	let triggerRef = $state<HTMLButtonElement | null>(null);

	const model = $derived(buildVaultMenuModel({ vaults, currentVault }));
	const label = $derived(currentVault || 'No vault selected');

	function toggle() {
		open = !open;
	}

	function close() {
		open = false;
	}

	function switchVault(vault: string) {
		close();
		if (vault === currentVault || typeof window === 'undefined') return;
		window.location.href = vaultSwitchUrl(window.location.href, vault);
	}

	function addVault() {
		close();
		if (typeof window === 'undefined') return;
		window.dispatchEvent(new CustomEvent(OPEN_FOLDER_EVENT));
	}

	function openSettings() {
		close();
		void goto(settingsRoute(base, currentVault));
	}

	function onWindowPointerDown(event: MouseEvent) {
		const target = event.target as Node;
		if (menuRef?.contains(target) || triggerRef?.contains(target)) return;
		close();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && open) {
			event.stopPropagation();
			close();
			triggerRef?.focus();
		}
	}

	$effect(() => {
		if (!open || typeof window === 'undefined') return;
		window.addEventListener('pointerdown', onWindowPointerDown, true);
		window.addEventListener('keydown', onKeydown, true);
		return () => {
			window.removeEventListener('pointerdown', onWindowPointerDown, true);
			window.removeEventListener('keydown', onKeydown, true);
		};
	});
</script>

{#if isBrowser}
	<div class="vault-menu">
		<button
			bind:this={triggerRef}
			type="button"
			class="vault-trigger"
			class:open
			aria-haspopup="menu"
			aria-expanded={open}
			title={currentVault}
			onclick={toggle}
		>
			<span class="vault-icon" aria-hidden="true">🗄️</span>
			<span class="vault-name">{label}</span>
			<span class="vault-caret" aria-hidden="true" class:open>
				<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
					<polyline points="6 9 12 15 18 9" />
				</svg>
			</span>
		</button>

		{#if open}
			<div bind:this={menuRef} class="vault-dropdown" role="menu" aria-label="Vault menu">
				{#if model.hasOtherVaults}
					<div class="dropdown-section" role="group" aria-label="Switch vault">
						{#each model.vaults as entry (entry.name)}
							<button
								type="button"
								class="dropdown-item"
								class:current={entry.isCurrent}
								role="menuitem"
								disabled={entry.isCurrent}
								onclick={() => switchVault(entry.name)}
							>
								<span class="item-icon" aria-hidden="true">🗄️</span>
								<span class="item-label">{entry.name}</span>
								{#if entry.isCurrent}
									<span class="item-tag">current</span>
								{/if}
							</button>
						{/each}
					</div>
					<div class="dropdown-sep" role="separator"></div>
				{/if}

				<button type="button" class="dropdown-item" role="menuitem" onclick={addVault}>
					<span class="item-icon" aria-hidden="true">＋</span>
					<span class="item-label">Add Vault…</span>
				</button>
				<button type="button" class="dropdown-item" role="menuitem" onclick={openSettings}>
					<span class="item-icon" aria-hidden="true">
						<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="12" cy="12" r="3" />
							<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
						</svg>
					</span>
					<span class="item-label">Settings</span>
				</button>
			</div>
		{/if}
	</div>
{:else}
	<span class="vault-identity" title={currentVault}>
		<span class="vault-icon" aria-hidden="true">🗄️</span>
		<span class="vault-name">{currentVault}</span>
	</span>
{/if}

<style>
	.vault-menu {
		position: relative;
		display: flex;
		min-width: 0;
	}

	.vault-trigger {
		display: flex;
		align-items: center;
		gap: 5px;
		min-width: 0;
		max-width: 220px;
		padding: 3px 6px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 6px;
		color: var(--text-default);
		cursor: pointer;
	}

	.vault-trigger:hover,
	.vault-trigger.open {
		background: var(--bg-hover);
		border-color: var(--border-default);
	}

	.vault-trigger:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	.vault-icon {
		font-size: 14px;
		flex-shrink: 0;
	}

	.vault-name {
		font-size: 14px;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.vault-caret {
		display: inline-flex;
		flex-shrink: 0;
		color: var(--text-muted);
		transition: transform 0.12s ease;
	}

	.vault-caret.open {
		transform: rotate(180deg);
	}

	.vault-dropdown {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		min-width: 220px;
		max-width: 280px;
		padding: 5px;
		background: var(--bg-elevated);
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		box-shadow: 0 10px 26px rgba(0, 0, 0, 0.4);
		z-index: 50;
	}

	.dropdown-section {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.dropdown-item {
		display: flex;
		align-items: center;
		gap: 9px;
		width: 100%;
		padding: 7px 9px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 5px;
		color: var(--text-secondary);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
	}

	.dropdown-item:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.dropdown-item:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -2px;
		color: var(--text-default);
	}

	.dropdown-item.current {
		color: var(--text-default);
		cursor: default;
	}

	.item-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		flex-shrink: 0;
		color: var(--text-muted);
	}

	.item-label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.item-tag {
		font-size: 11px;
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.dropdown-sep {
		height: 1px;
		margin: 4px 2px;
		background: var(--border-default);
	}

	.vault-identity {
		display: flex;
		align-items: center;
		gap: 5px;
		overflow: hidden;
	}

	.vault-identity .vault-icon {
		font-size: 14px;
		flex-shrink: 0;
	}

	.vault-identity .vault-name {
		font-size: 14px;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
