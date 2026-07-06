<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { settingsStore } from '$lib/settings.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { listVaults } from '$lib/api';
	import { chooseRegisteredVault } from '$lib/app-shell-core';
	import { registerHotkeys } from '$lib/hotkeys';
	import SidebarSettings from '$lib/components/SidebarSettings.svelte';
	import VaultsSettings from '$lib/components/VaultsSettings.svelte';
	import AgentSettings from '$lib/components/settings/AgentSettings.svelte';
	import McpServers from '$lib/components/settings/McpServers.svelte';
	import AppearanceSettings from '$lib/components/settings/AppearanceSettings.svelte';
	import ConnectionSettings from '$lib/components/settings/ConnectionSettings.svelte';
	import DailySettings from '$lib/components/settings/DailySettings.svelte';
	import EditorSettings from '$lib/components/settings/EditorSettings.svelte';
	import GeneralSettings from '$lib/components/settings/GeneralSettings.svelte';
	import GitSettings from '$lib/components/settings/GitSettings.svelte';
	import HooksSettings from '$lib/components/settings/HooksSettings.svelte';
	import EmbeddingsSettings from '$lib/components/settings/EmbeddingsSettings.svelte';
	import ToastStack from '$lib/components/ToastStack.svelte';

	type Section =
		| 'general'
		| 'daily'
		| 'editor'
		| 'sidebar'
		| 'git'
		| 'hooks'
		| 'embed'
		| 'appearance'
		| 'vaults'
		| 'connection'
		| 'agent'
		| 'mcp';

	let selectedSection = $state<Section>('general');
	let vault = $derived(vaultStore.currentVault);
	let cfg = $derived(settingsStore.draftConfig);
	let status = $derived(settingsStore.status);
	let fieldErrors = $derived(settingsStore.fieldErrors);
	let warnings = $derived(settingsStore.warnings);
	let conflict = $derived(settingsStore.conflict);
	let dirty = $derived(settingsStore.dirtySections);
	let caps = $derived(settingsStore.capabilities);
	let hasVault = $derived(Boolean(vault));

	const vaultSections: { id: Section; label: string }[] = [
		{ id: 'general', label: 'General' },
		{ id: 'appearance', label: 'Appearance' },
		{ id: 'daily', label: 'Daily Notes' },
		{ id: 'editor', label: 'Editor' },
		{ id: 'sidebar', label: 'Sidebar' },
		{ id: 'git', label: 'Git Sync' },
		{ id: 'hooks', label: 'Hooks' },
		{ id: 'embed', label: 'Semantic Search' }
	];

	const appSections: { id: Section; label: string }[] = [
		{ id: 'agent', label: 'AI Agent' },
		{ id: 'mcp', label: 'MCP Servers' },
		{ id: 'connection', label: 'Connection' },
		{ id: 'vaults', label: 'Vaults' }
	];

	function navigateBack() {
		settingsStore.resetState();
		if (vault) {
			void goto(`${base}/?vault=${encodeURIComponent(vault)}`);
		} else {
			void goto(`${base}/`);
		}
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
		void initializeSettings();
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

	async function initializeSettings() {
		const url = new URL(window.location.href);
		const requestedVault = url.searchParams.get('vault');
		const requestedSection = url.searchParams.get('section');
		let selectedVault = vaultStore.currentVault;
		try {
			const registrations = await listVaults();
			selectedVault = chooseRegisteredVault(requestedVault, registrations).vault;
			vaultStore.currentVault = selectedVault;
		} catch {
			if (requestedVault) {
				selectedVault = requestedVault;
				vaultStore.currentVault = requestedVault;
			}
		}

		// Only load vault config when we actually have a vault. Without this
		// guard the daemon returns 404 for `/api/v//config` and surfaces a
		// confusing error to the user on a fresh install.
		if (selectedVault) {
			void settingsStore.loadConfig(selectedVault);
		} else {
			// No vault → drop the user straight into the Vaults section so the
			// empty-state CTA is what they see first.
			selectedSection = 'vaults';
		}
		// A `section` query param deep-links to an app-level section (e.g. the
		// status bar's "Manage servers…" → Connection). It wins over the
		// no-vault default above.
		if (requestedSection && isSection(requestedSection)) {
			selectedSection = requestedSection;
		}
		if (!settingsStore.capabilities) {
			void settingsStore.loadCapabilities();
		}
	}

	function isSection(value: string): value is Section {
		return (
			vaultSections.some((s) => s.id === value) || appSections.some((s) => s.id === value)
		);
	}
</script>

<div class="settings-layout">
	<nav class="settings-nav">
		<button class="back-btn" type="button" onclick={navigateBack}
			>{hasVault ? '← Back to vault' : '← Close'}</button
		>

		{#if hasVault}
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
		{/if}

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

		{#if selectedSection === 'vaults'}
			<div class="settings-body">
				<VaultsSettings capabilities={caps} />
			</div>
		{:else if selectedSection === 'agent'}
			<div class="settings-body">
				<AgentSettings />
			</div>
		{:else if selectedSection === 'mcp'}
			<div class="settings-body">
				<McpServers />
			</div>
		{:else if selectedSection === 'connection'}
			<div class="settings-body">
				<ConnectionSettings />
			</div>
		{:else if cfg}
			<div class="settings-body">
				{#if selectedSection === 'appearance'}
					<AppearanceSettings {cfg} {saveImmediate} />
				{:else if selectedSection === 'general'}
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
				{:else if selectedSection === 'embed'}
					<EmbeddingsSettings {cfg} capabilities={caps} {saveImmediate} />
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

<ToastStack />

<style>
	.settings-layout {
		display: flex;
		height: 100vh;
		overflow: hidden;
	}

	.settings-nav {
		width: 220px;
		min-width: 180px;
		background: var(--bg-secondary);
		border-right: 1px solid var(--border-default);
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
		border-bottom: 1px solid var(--border-default);
		color: var(--text-muted);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.back-btn:hover {
		background: var(--bg-hover);
		color: var(--text-default);
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
		color: var(--text-muted);
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 7px 16px;
		background: none;
		border: none;
		color: var(--text-default);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.nav-item:hover {
		background: var(--bg-hover);
	}

	.nav-item.active {
		background: var(--bg-active);
		color: var(--text-inverse);
		font-weight: 500;
	}

	.dirty-dot {
		color: var(--color-warning);
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
		border-bottom: 1px solid var(--border-default);
		flex-shrink: 0;
	}

	.content-header h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 600;
	}

	.status-badge {
		font-size: 12px;
		color: var(--text-muted);
	}

	.error-banner {
		padding: 10px 24px;
		background: var(--danger-bg);
		color: var(--color-danger);
		border-bottom: 1px solid var(--danger-border);
		font-size: 13px;
	}

	.conflict-banner {
		padding: 10px 24px;
		background: var(--warning-bg);
		color: var(--color-warning);
		border-bottom: 1px solid var(--warning-border);
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
		border: 1px solid var(--warning-border);
		border-radius: 4px;
		background: transparent;
		color: var(--color-warning);
		font-size: 12px;
		cursor: pointer;
	}

	.conflict-actions button:hover {
		background: var(--warning-hover);
	}

	.settings-body {
		flex: 1;
		overflow-y: auto;
		padding: 0;
	}

	.config-footer {
		padding: 8px 24px;
		border-top: 1px solid var(--border-default);
		flex-shrink: 0;
	}

	.config-path {
		font-size: 11px;
		color: var(--text-faint);
	}

	.settings-empty {
		padding: 32px 24px;
		text-align: center;
		color: var(--text-muted);
		font-size: 13px;
	}

	@media (max-width: 600px) {
		.settings-nav {
			width: 160px;
			min-width: 140px;
		}
	}
</style>
