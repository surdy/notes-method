<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { settingsStore } from '$lib/settings.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { themeStore, type ThemeEntry, type ThemeMode, type VisualMode } from '$lib/theme.svelte';
	import { registerHotkeys } from '$lib/hotkeys';
	import SidebarSettings from '$lib/components/SidebarSettings.svelte';
	import VaultsSettings from '$lib/components/VaultsSettings.svelte';
	import DailySettings from '$lib/components/settings/DailySettings.svelte';
	import EditorSettings from '$lib/components/settings/EditorSettings.svelte';
	import GeneralSettings from '$lib/components/settings/GeneralSettings.svelte';
	import GitSettings from '$lib/components/settings/GitSettings.svelte';
	import HooksSettings from '$lib/components/settings/HooksSettings.svelte';
	import ToastStack from '$lib/components/ToastStack.svelte';

	type Section =
		| 'general'
		| 'daily'
		| 'editor'
		| 'sidebar'
		| 'git'
		| 'hooks'
		| 'appearance'
		| 'vaults';

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
		{ id: 'appearance', label: 'Appearance' },
		{ id: 'daily', label: 'Daily Notes' },
		{ id: 'editor', label: 'Editor' },
		{ id: 'sidebar', label: 'Sidebar' },
		{ id: 'git', label: 'Git Sync' },
		{ id: 'hooks', label: 'Hooks' }
	];

	const appSections: { id: Section; label: string }[] = [
		{ id: 'vaults', label: 'Vaults' }
	];

	const themeOptions: ThemeEntry[] = themeStore.getCatalog();
	const modeOptions: Array<{ value: ThemeMode; label: string }> = [
		{ value: 'system', label: 'System' },
		{ value: 'dark', label: 'Dark' },
		{ value: 'light', label: 'Light' }
	];
	const visualModeOptions: Array<{ value: VisualMode; label: string }> = [
		{ value: 'default', label: 'Default' },
		{ value: 'high-contrast', label: 'High Contrast' }
	];

	function navigateBack() {
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

	function currentThemeName(): string {
		const theme = cfg?.appearance?.theme;
		return theme === 'dark' || theme === 'light' || theme === 'system' || theme === 'manuscript' || theme === 'hc-dark'
			? themeStore.theme
			: theme ?? themeStore.theme;
	}

	function currentModeValue(): ThemeMode {
		const mode = cfg?.appearance?.mode;
		return mode === 'dark' || mode === 'light' || mode === 'system' ? mode : themeStore.mode;
	}

	function currentVisualModeValue(): VisualMode {
		const visualMode = cfg?.appearance?.visualMode;
		return visualMode === 'default' || visualMode === 'high-contrast'
			? visualMode
			: themeStore.visualMode;
	}

	function updateAppearance(partial: Partial<{ theme: string; mode: ThemeMode; visualMode: VisualMode }>) {
		if (!cfg) return;

		cfg.appearance = {
			theme: currentThemeName(),
			mode: currentModeValue(),
			visualMode: currentVisualModeValue(),
			...partial
		};
	}

	function selectedThemeEntry(): ThemeEntry | undefined {
		return themeOptions.find((option) => option.name === currentThemeName());
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

		{#if selectedSection === 'vaults'}
			<div class="settings-body">
				<VaultsSettings capabilities={caps} />
			</div>
		{:else if cfg}
			<div class="settings-body">
				{#if selectedSection === 'appearance'}
					<section class="section-content">
						<h2>Appearance</h2>
						<p class="section-description">
							Choose a catalog theme, tone preference, and optional high-contrast overlay for this vault.
						</p>
						<div class="appearance-grid">
							<label class="appearance-field">
								<span>Theme</span>
								<select
									value={currentThemeName()}
									onchange={(event) => {
										const theme = (event.currentTarget as HTMLSelectElement).value;
										updateAppearance({ theme });
										themeStore.setTheme(theme);
										void saveImmediate('appearance');
									}}
								>
									{#each themeOptions as option}
										<option value={option.name}>{option.display_name}</option>
									{/each}
								</select>
								{#if selectedThemeEntry()}
									<span class="appearance-meta">
										{selectedThemeEntry()?.author} · {selectedThemeEntry()?.tone}
									</span>
								{/if}
							</label>

							<label class="appearance-field">
								<span>Tone</span>
								<select
									value={currentModeValue()}
									onchange={(event) => {
										const mode = (event.currentTarget as HTMLSelectElement).value as ThemeMode;
										updateAppearance({ mode });
										themeStore.setMode(mode);
										void saveImmediate('appearance');
									}}
								>
									{#each modeOptions as option}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
								<span class="appearance-meta">System follows the OS color-scheme setting.</span>
							</label>

							<label class="appearance-field">
								<span>Visual mode</span>
								<select
									value={currentVisualModeValue()}
									onchange={(event) => {
										const visualMode = (event.currentTarget as HTMLSelectElement).value as VisualMode;
										updateAppearance({ visualMode });
										themeStore.setVisualMode(visualMode);
										void saveImmediate('appearance');
									}}
								>
									{#each visualModeOptions as option}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
								<span class="appearance-meta">High Contrast boosts semantic token contrast on top of the active theme.</span>
							</label>
						</div>
					</section>
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

	.section-content {
		padding: 16px 24px;
		max-width: 760px;
	}

	.section-content h2 {
		margin: 0 0 8px;
		font-size: 20px;
	}

	.section-description {
		margin: 0 0 18px;
		color: var(--ns-text-muted);
		font-size: 13px;
	}

	.appearance-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 16px;
	}

	.appearance-field {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border: 1px solid var(--ns-border);
		border-radius: 10px;
		background: var(--ns-surface-elevated);
		color: var(--ns-text);
		font-size: 13px;
		font-weight: 600;
	}

	.appearance-field select {
		padding: 8px 10px;
		border: 1px solid var(--ns-border-input);
		border-radius: 8px;
		background: var(--ns-input-bg);
		color: var(--ns-text);
		font-size: 13px;
	}

	.appearance-meta {
		color: var(--ns-text-muted);
		font-size: 12px;
		font-weight: 400;
		line-height: 1.4;
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

		.section-content {
			padding: 12px 16px;
		}
	}
</style>
