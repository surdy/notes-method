<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { settingsStore } from '$lib/settings.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { registerHotkeys } from '$lib/hotkeys';
	import SidebarSettings from '$lib/components/SidebarSettings.svelte';
	import VaultsSettings from '$lib/components/VaultsSettings.svelte';
	import DailySettings from '$lib/components/settings/DailySettings.svelte';
	import EditorSettings from '$lib/components/settings/EditorSettings.svelte';
	import GeneralSettings from '$lib/components/settings/GeneralSettings.svelte';
	import GitSettings from '$lib/components/settings/GitSettings.svelte';
	import HooksSettings from '$lib/components/settings/HooksSettings.svelte';

	type Section = 'general' | 'daily' | 'editor' | 'sidebar' | 'git' | 'hooks' | 'vaults';

	let selectedSection = $state<Section>('general');
	let vault = $derived(vaultStore.currentVault);
	let cfg = $derived(settingsStore.draftConfig);
	let status = $derived(settingsStore.status);
	let fieldErrors = $derived(settingsStore.fieldErrors);
	let warnings = $derived(settingsStore.warnings);
	let conflict = $derived(settingsStore.conflict);
	let dirty = $derived(settingsStore.dirtySections);
	let caps = $derived(settingsStore.capabilities);

	const vaultSections: { id: Section; label: string }[] = [
		{ id: 'general', label: 'General' },
		{ id: 'daily', label: 'Daily Notes' },
		{ id: 'editor', label: 'Editor' },
		{ id: 'sidebar', label: 'Sidebar' },
		{ id: 'git', label: 'Git Sync' },
		{ id: 'hooks', label: 'Hooks' }
	];

	const appSections: { id: Section; label: string }[] = [{ id: 'vaults', label: 'Vaults' }];

	function navigateBack() {
		if (settingsStore.isDirty) {
			const discard = window.confirm('You have unsaved settings changes. Discard them?');
			if (!discard) return;
		}
		settingsStore.resetState();
		void goto(`${base}/?vault=${encodeURIComponent(vault)}`);
	}

	function markDirty(section: string) {
		settingsStore.markDirty(section);
	}

	async function saveImmediate(section: string) {
		settingsStore.markDirty(section);
		await settingsStore.saveConfig(vault);
	}

	async function saveSection(section: string) {
		const ok = await settingsStore.saveConfig(vault);
		if (ok) settingsStore.markClean(section);
	}

	function revert(section: string) {
		settingsStore.revertSection(section);
	}

	function sectionIsDirty(section: string): boolean {
		return dirty.has(section);
	}

	function fieldError(key: string): string | null {
		return fieldErrors[key] ?? null;
	}

	function fieldWarning(key: string): string | null {
		return warnings[key] ?? null;
	}

	onMount(() => {
		const url = new URL(window.location.href);
		const v = url.searchParams.get('vault');
		if (v) vaultStore.currentVault = v;

		void settingsStore.loadConfig(vault);
		if (!settingsStore.capabilities) {
			void settingsStore.loadCapabilities();
		}

		const unregister = registerHotkeys([
			{
				key: 'Escape',
				action: navigateBack
			}
		]);

		return () => {
			unregister();
		};
	});
</script>

<div class="settings-layout">
	<nav class="settings-nav">
		<button class="back-btn" type="button" onclick={navigateBack}>← Back to vault</button>

		<div class="nav-group">
			<h3 class="nav-group-title">VAULT: {vault}</h3>
			{#each vaultSections as section}
				<button
					class="nav-item"
					class:active={selectedSection === section.id}
					type="button"
					onclick={() => (selectedSection = section.id)}
				>
					{section.label}
					{#if dirty.has(section.id)}
						<span class="dirty-dot" title="Unsaved changes">●</span>
					{/if}
				</button>
			{/each}
		</div>

		<div class="nav-group">
			<h3 class="nav-group-title">APP</h3>
			{#each appSections as section}
				<button
					class="nav-item"
					class:active={selectedSection === section.id}
					type="button"
					onclick={() => (selectedSection = section.id)}
				>
					{section.label}
				</button>
			{/each}
		</div>
	</nav>

	<main class="settings-content">
		<header class="content-header">
			<h2>{vaultSections.find((s) => s.id === selectedSection)?.label ?? appSections.find((s) => s.id === selectedSection)?.label ?? ''}</h2>
			{#if status === 'loading'}
				<span class="status-badge">Loading…</span>
			{:else if status === 'saving'}
				<span class="status-badge">Saving…</span>
			{/if}
		</header>

		{#if settingsStore.error}
			<div class="error-banner">{settingsStore.error}</div>
		{/if}

		{#if conflict}
			<div class="conflict-banner">
				<p>⚠️ Config was changed externally.</p>
				<div class="conflict-actions">
					<button type="button" onclick={() => settingsStore.acceptServerVersion()}
						>Reload</button
					>
					<button type="button" onclick={() => void settingsStore.overwriteConflict(vault)}
						>Overwrite</button
					>
				</div>
			</div>
		{/if}

		{#if cfg}
			<div class="settings-body">
				{#if selectedSection === 'general'}
					<GeneralSettings
						{cfg}
						{fieldError}
						{fieldWarning}
						{sectionIsDirty}
						{saveSection}
						{revert}
						{markDirty}
						{saveImmediate}
					/>
				{:else if selectedSection === 'daily'}
					<DailySettings
						{cfg}
						{fieldError}
						{fieldWarning}
						{sectionIsDirty}
						{saveSection}
						{revert}
						{markDirty}
						{saveImmediate}
					/>
				{:else if selectedSection === 'editor'}
					<EditorSettings {cfg} {saveImmediate} {markDirty} />
				{:else if selectedSection === 'sidebar'}
					<SidebarSettings {vault} />
				{:else if selectedSection === 'git'}
					<GitSettings
						{cfg}
						{fieldError}
						{sectionIsDirty}
						{saveSection}
						{revert}
						{markDirty}
						{saveImmediate}
					/>
				{:else if selectedSection === 'hooks'}
					<HooksSettings {cfg} {sectionIsDirty} {saveSection} {revert} {markDirty} />
				{:else if selectedSection === 'vaults'}
					<VaultsSettings capabilities={caps} />
				{/if}
			</div>

			<footer class="config-footer">
				<span class="config-path">Config: {settingsStore.configPath}</span>
			</footer>
		{:else if status !== 'loading'}
			<div class="settings-empty">
				{#if !caps?.can_edit_vault_config}
					<p>Vault config editing is not available in this deployment.</p>
				{:else}
					<p>No configuration loaded.</p>
				{/if}
			</div>
		{/if}
	</main>
</div>

<style>
	.settings-layout {
		display: flex;
		height: 100vh;
		overflow: hidden;
	}

	.settings-nav {
		width: 220px;
		min-width: 180px;
		background: var(--ns-sidebar-bg);
		border-right: 1px solid var(--ns-border);
		display: flex;
		flex-direction: column;
		padding: 0;
		overflow-y: auto;
	}

	.back-btn {
		display: block;
		width: 100%;
		padding: 14px 16px;
		background: none;
		border: none;
		border-bottom: 1px solid var(--ns-border);
		color: var(--ns-text-muted);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.back-btn:hover {
		background: var(--ns-surface-hover);
		color: var(--ns-text);
	}

	.nav-group {
		padding: 12px 0 4px;
	}

	.nav-group-title {
		margin: 0 0 4px;
		padding: 0 16px;
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--ns-text-muted);
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 7px 16px;
		background: none;
		border: none;
		color: var(--ns-text);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.nav-item:hover {
		background: var(--ns-surface-hover);
	}

	.nav-item.active {
		background: var(--ns-surface-active);
		color: var(--ns-text-inverse);
		font-weight: 500;
	}

	.dirty-dot {
		color: var(--ns-warning);
		font-size: 10px;
	}

	.settings-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.content-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 14px 24px;
		border-bottom: 1px solid var(--ns-border);
		flex-shrink: 0;
	}

	.content-header h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 600;
	}

	.status-badge {
		font-size: 12px;
		color: var(--ns-text-muted);
	}

	.error-banner {
		padding: 10px 24px;
		background: var(--ns-danger-bg);
		color: var(--ns-danger);
		border-bottom: 1px solid var(--ns-danger-border);
		font-size: 13px;
	}

	.conflict-banner {
		padding: 10px 24px;
		background: var(--ns-warning-bg);
		color: var(--ns-warning);
		border-bottom: 1px solid var(--ns-warning-border);
		font-size: 13px;
	}

	.conflict-banner p {
		margin: 0 0 8px;
	}

	.conflict-actions {
		display: flex;
		gap: 8px;
	}

	.conflict-actions button {
		padding: 4px 12px;
		border: 1px solid var(--ns-warning-border);
		border-radius: 4px;
		background: transparent;
		color: var(--ns-warning);
		font-size: 12px;
		cursor: pointer;
	}

	.conflict-actions button:hover {
		background: var(--ns-warning-hover);
	}

	.settings-body {
		flex: 1;
		overflow-y: auto;
		padding: 0;
	}

	.config-footer {
		padding: 8px 24px;
		border-top: 1px solid var(--ns-border);
		flex-shrink: 0;
	}

	.config-path {
		font-size: 11px;
		color: var(--ns-text-subtle);
	}

	.settings-empty {
		padding: 32px 24px;
		text-align: center;
		color: var(--ns-text-muted);
		font-size: 13px;
	}

	@media (max-width: 600px) {
		.settings-nav {
			width: 160px;
			min-width: 140px;
		}
	}
</style>
