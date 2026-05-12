<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { settingsStore } from '$lib/settings.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { registerHotkeys } from '$lib/hotkeys';
	import SidebarSettings from '$lib/components/SidebarSettings.svelte';
	import VaultsSettings from '$lib/components/VaultsSettings.svelte';

	type Section = 'general' | 'inbox' | 'daily' | 'editor' | 'sidebar' | 'git' | 'hooks' | 'vaults';

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
		{ id: 'inbox', label: 'Inbox' },
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
		void goto(`/?vault=${encodeURIComponent(vault)}`);
	}

	function textField(
		section: string,
		value: string | null | undefined,
		setter: (v: string) => void
	) {
		return {
			value: value ?? '',
			oninput(e: Event) {
				setter((e.target as HTMLInputElement).value);
				settingsStore.markDirty(section);
			}
		};
	}

	function toggleField(section: string, value: boolean, setter: (v: boolean) => void) {
		return {
			checked: value,
			onchange(e: Event) {
				setter((e.target as HTMLInputElement).checked);
				void saveImmediate(section);
			}
		};
	}

	function selectField(section: string, value: string, setter: (v: string) => void) {
		return {
			value,
			onchange(e: Event) {
				setter((e.target as HTMLSelectElement).value);
				void saveImmediate(section);
			}
		};
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
					<section class="config-section">
						{#if sectionIsDirty('name') || sectionIsDirty('homepage')}
							<div class="section-actions">
								<button
									type="button"
									class="btn-save"
									onclick={() => void saveSection('name')}>Save</button
								>
								<button
									type="button"
									class="btn-revert"
									onclick={() => {
										revert('name');
										revert('homepage');
									}}>Revert</button
								>
							</div>
						{/if}
						<label class="field">
							<span class="field-label">Vault Name</span>
							<input
								type="text"
								{...textField('name', cfg.name, (v) => {
									if (cfg) cfg.name = v;
								})}
							/>
							{#if fieldError('name')}<span class="field-error">{fieldError('name')}</span
							>{/if}
						</label>
						<label class="field">
							<span class="field-label">Homepage</span>
							<input
								type="text"
								placeholder="e.g. Dashboard.md"
								{...textField('homepage', cfg.homepage, (v) => {
									if (cfg) cfg.homepage = v || null;
								})}
							/>
						</label>
					</section>
				{:else if selectedSection === 'inbox'}
					<section class="config-section">
						{#if sectionIsDirty('inbox')}
							<div class="section-actions">
								<button
									type="button"
									class="btn-save"
									onclick={() => void saveSection('inbox')}>Save</button
								>
								<button
									type="button"
									class="btn-revert"
									onclick={() => revert('inbox')}>Revert</button
								>
							</div>
						{/if}
						<label class="field">
							<span class="field-label">Folder</span>
							<input
								type="text"
								{...textField('inbox', cfg.inbox.folder, (v) => {
									if (cfg) cfg.inbox.folder = v;
								})}
							/>
							{#if fieldError('inbox.folder')}<span class="field-error"
									>{fieldError('inbox.folder')}</span
								>{/if}
							{#if fieldWarning('inbox.folder')}<span class="field-warning"
									>{fieldWarning('inbox.folder')}</span
								>{/if}
						</label>
						<label class="field">
							<span class="field-label">Template</span>
							<input
								type="text"
								{...textField('inbox', cfg.inbox.template, (v) => {
									if (cfg) cfg.inbox.template = v;
								})}
							/>
						</label>
					</section>
				{:else if selectedSection === 'daily'}
					<section class="config-section">
						{#if sectionIsDirty('daily')}
							<div class="section-actions">
								<button
									type="button"
									class="btn-save"
									onclick={() => void saveSection('daily')}>Save</button
								>
								<button
									type="button"
									class="btn-revert"
									onclick={() => revert('daily')}>Revert</button
								>
							</div>
						{/if}
						<label class="field">
							<span class="field-label">Folder</span>
							<input
								type="text"
								{...textField('daily', cfg.daily.folder, (v) => {
									if (cfg) cfg.daily.folder = v;
								})}
							/>
							{#if fieldError('daily.folder')}<span class="field-error"
									>{fieldError('daily.folder')}</span
								>{/if}
							{#if fieldWarning('daily.folder')}<span class="field-warning"
									>{fieldWarning('daily.folder')}</span
								>{/if}
						</label>
						<label class="field">
							<span class="field-label">Template</span>
							<input
								type="text"
								{...textField('daily', cfg.daily.template, (v) => {
									if (cfg) cfg.daily.template = v;
								})}
							/>
						</label>
						<label class="field">
							<span class="field-label">Generate At (HH:MM)</span>
							<input
								type="text"
								placeholder="e.g. 06:00"
								{...textField('daily', cfg.daily.generate_at, (v) => {
									if (cfg) cfg.daily.generate_at = v || null;
								})}
							/>
							{#if fieldError('daily.generate_at')}<span class="field-error"
									>{fieldError('daily.generate_at')}</span
								>{/if}
						</label>
						<label class="field">
							<span class="field-label">Timezone</span>
							<input
								type="text"
								placeholder="e.g. America/New_York"
								{...textField('daily', cfg.daily.timezone, (v) => {
									if (cfg) cfg.daily.timezone = v || null;
								})}
							/>
							{#if fieldError('daily.timezone')}<span class="field-error"
									>{fieldError('daily.timezone')}</span
								>{/if}
						</label>
						<label class="field field-toggle">
							<span class="field-label">Catch Up Missed Days</span>
							<input
								type="checkbox"
								{...toggleField('daily', cfg.daily.catch_up, (v) => {
									if (cfg) cfg.daily.catch_up = v;
								})}
							/>
						</label>
					</section>
				{:else if selectedSection === 'editor'}
					<section class="config-section">
						<label class="field field-toggle">
							<span class="field-label">Live Preview</span>
							<input
								type="checkbox"
								{...toggleField('editor', cfg.editor.live_preview, (v) => {
									if (cfg) cfg.editor.live_preview = v;
								})}
							/>
						</label>
						<label class="field">
							<span class="field-label">Default Mode</span>
							<select
								{...selectField('editor', cfg.editor.default_mode, (v) => {
									if (cfg) cfg.editor.default_mode = v;
								})}
							>
								<option value="source">Source</option>
								<option value="reading">Reading</option>
								<option value="live-preview">Live Preview</option>
							</select>
						</label>
					</section>
				{:else if selectedSection === 'sidebar'}
					<section class="config-section">
						<SidebarSettings {vault} />
					</section>
				{:else if selectedSection === 'git'}
					<section class="config-section">
						{#if sectionIsDirty('git')}
							<div class="section-actions">
								<button
									type="button"
									class="btn-save"
									onclick={() => void saveSection('git')}>Save</button
								>
								<button
									type="button"
									class="btn-revert"
									onclick={() => revert('git')}>Revert</button
								>
							</div>
						{/if}
						<label class="field field-toggle">
							<span class="field-label">Enabled</span>
							<input
								type="checkbox"
								{...toggleField('git', cfg.git.enabled, (v) => {
									if (cfg) cfg.git.enabled = v;
								})}
							/>
						</label>
						<label class="field">
							<span class="field-label">Auto-commit Interval</span>
							<input
								type="text"
								placeholder="e.g. 5m"
								{...textField('git', cfg.git.auto_commit_every, (v) => {
									if (cfg) cfg.git.auto_commit_every = v || null;
								})}
							/>
							{#if fieldError('git.auto_commit_every')}<span class="field-error"
									>{fieldError('git.auto_commit_every')}</span
								>{/if}
						</label>
						<label class="field">
							<span class="field-label">Auto-pull Interval</span>
							<input
								type="text"
								placeholder="e.g. 5m"
								{...textField('git', cfg.git.auto_pull_every, (v) => {
									if (cfg) cfg.git.auto_pull_every = v || null;
								})}
							/>
							{#if fieldError('git.auto_pull_every')}<span class="field-error"
									>{fieldError('git.auto_pull_every')}</span
								>{/if}
						</label>
						<label class="field">
							<span class="field-label">Auto-push Interval</span>
							<input
								type="text"
								placeholder="e.g. 5m"
								{...textField('git', cfg.git.auto_push_every, (v) => {
									if (cfg) cfg.git.auto_push_every = v || null;
								})}
							/>
							{#if fieldError('git.auto_push_every')}<span class="field-error"
									>{fieldError('git.auto_push_every')}</span
								>{/if}
						</label>
						<label class="field">
							<span class="field-label">Commit Message</span>
							<input
								type="text"
								placeholder="e.g. auto: sync changes"
								{...textField('git', cfg.git.commit_message, (v) => {
									if (cfg) cfg.git.commit_message = v || null;
								})}
							/>
						</label>
					</section>
				{:else if selectedSection === 'hooks'}
					<section class="config-section">
						{#if sectionIsDirty('hooks')}
							<div class="section-actions">
								<button
									type="button"
									class="btn-save"
									onclick={() => void saveSection('hooks')}>Save</button
								>
								<button
									type="button"
									class="btn-revert"
									onclick={() => revert('hooks')}>Revert</button
								>
							</div>
						{/if}
						<label class="field">
							<span class="field-label">On Note Create</span>
							<input
								type="text"
								placeholder="shell command"
								{...textField('hooks', cfg.hooks.on_note_create, (v) => {
									if (cfg) cfg.hooks.on_note_create = v || null;
								})}
							/>
						</label>
						<label class="field">
							<span class="field-label">On Daily Create</span>
							<input
								type="text"
								placeholder="shell command"
								{...textField('hooks', cfg.hooks.on_daily_create, (v) => {
									if (cfg) cfg.hooks.on_daily_create = v || null;
								})}
							/>
						</label>
					</section>
				{:else if selectedSection === 'vaults'}
					<section class="config-section">
						<VaultsSettings capabilities={caps} />
					</section>
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
		background: var(--sidebar-bg, #252526);
		border-right: 1px solid var(--border-color, #333);
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
		border-bottom: 1px solid var(--border-color, #333);
		color: var(--text-muted, #888);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.back-btn:hover {
		background: var(--hover-bg, #2a2d2e);
		color: var(--text-primary, #e0e0e0);
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
		color: var(--text-muted, #888);
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 7px 16px;
		background: none;
		border: none;
		color: var(--text-primary, #e0e0e0);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.nav-item:hover {
		background: var(--hover-bg, #2a2d2e);
	}

	.nav-item.active {
		background: var(--active-bg, #37373d);
		color: #fff;
		font-weight: 500;
	}

	.dirty-dot {
		color: #f5c842;
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
		border-bottom: 1px solid var(--border-color, #333);
		flex-shrink: 0;
	}

	.content-header h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 600;
	}

	.status-badge {
		font-size: 12px;
		color: var(--text-muted, #888);
	}

	.error-banner {
		padding: 10px 24px;
		background: #3a1a1a;
		color: #ff6b6b;
		border-bottom: 1px solid #5a2a2a;
		font-size: 13px;
	}

	.conflict-banner {
		padding: 10px 24px;
		background: #3a3018;
		color: #f5c842;
		border-bottom: 1px solid #5a4a20;
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
		border: 1px solid #5a4a20;
		border-radius: 4px;
		background: transparent;
		color: #f5c842;
		font-size: 12px;
		cursor: pointer;
	}

	.conflict-actions button:hover {
		background: #4a3a10;
	}

	.settings-body {
		flex: 1;
		overflow-y: auto;
		padding: 0;
	}

	.config-section {
		padding: 16px 24px;
		max-width: 560px;
	}

	.section-actions {
		display: flex;
		gap: 6px;
		margin-bottom: 12px;
	}

	.btn-save,
	.btn-revert {
		padding: 5px 14px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #444);
		font-size: 12px;
		cursor: pointer;
	}

	.btn-save {
		background: #264f78;
		color: #fff;
		border-color: #264f78;
	}

	.btn-save:hover {
		background: #2d5f8e;
	}

	.btn-revert {
		background: transparent;
		color: var(--text-muted, #888);
	}

	.btn-revert:hover {
		background: var(--hover-bg, #2a2d2e);
		color: var(--text-primary, #e0e0e0);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 14px;
	}

	.field-toggle {
		flex-direction: row;
		align-items: center;
		gap: 10px;
	}

	.field-toggle input[type='checkbox'] {
		order: -1;
		width: 16px;
		height: 16px;
		accent-color: #264f78;
	}

	.field-toggle .field-label {
		order: 1;
	}

	.field-label {
		font-size: 12px;
		color: var(--text-muted, #888);
	}

	.field input[type='text'],
	.field select {
		padding: 6px 10px;
		border: 1px solid var(--border-color, #444);
		border-radius: 4px;
		background: var(--bg-secondary, #2a2a2a);
		color: var(--text-primary, #e0e0e0);
		font-size: 13px;
		max-width: 400px;
	}

	.field input[type='text']:focus,
	.field select:focus {
		outline: none;
		border-color: #264f78;
	}

	.field-error {
		color: #ff6b6b;
		font-size: 11px;
	}

	.field-warning {
		color: #f5c842;
		font-size: 11px;
	}

	.section-hint {
		color: var(--text-muted, #888);
		font-size: 13px;
		margin: 0;
		line-height: 1.6;
	}

	.section-hint.muted {
		margin-top: 8px;
		font-size: 12px;
		color: var(--text-muted, #666);
	}

	.section-hint code {
		background: var(--bg-secondary, #2a2a2a);
		padding: 1px 4px;
		border-radius: 3px;
		font-size: 12px;
	}

	.config-footer {
		padding: 8px 24px;
		border-top: 1px solid var(--border-color, #333);
		flex-shrink: 0;
	}

	.config-path {
		font-size: 11px;
		color: var(--text-muted, #666);
	}

	.settings-empty {
		padding: 32px 24px;
		text-align: center;
		color: var(--text-muted, #888);
		font-size: 13px;
	}

	@media (max-width: 600px) {
		.settings-nav {
			width: 160px;
			min-width: 140px;
		}
		.config-section {
			padding: 12px 16px;
		}
	}
</style>
