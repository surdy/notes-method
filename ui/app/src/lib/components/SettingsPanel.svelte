<script lang="ts">
	import { settingsStore } from '$lib/settings.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	let vault = $derived(vaultStore.currentVault);
	let cfg = $derived(settingsStore.draftConfig);
	let status = $derived(settingsStore.status);
	let fieldErrors = $derived(settingsStore.fieldErrors);
	let warnings = $derived(settingsStore.warnings);
	let conflict = $derived(settingsStore.conflict);
	let dirty = $derived(settingsStore.dirtySections);
	let caps = $derived(settingsStore.capabilities);

	$effect(() => {
		if (settingsStore.open && vault) {
			void settingsStore.loadConfig(vault);
			if (!settingsStore.capabilities) {
				void settingsStore.loadCapabilities();
			}
		}
	});

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			settingsStore.close();
		}
	}

	function textField(section: string, value: string | null | undefined, setter: (v: string) => void) {
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
</script>

{#if settingsStore.open}
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="settings-backdrop" role="dialog" aria-label="Settings" onkeydown={handleKeydown}>
	<div class="settings-panel">
		<header class="settings-header">
			<h2>Settings</h2>
			<div class="header-right">
				{#if status === 'loading'}
					<span class="status-badge">Loading…</span>
				{:else if status === 'saving'}
					<span class="status-badge">Saving…</span>
				{/if}
				<button class="close-btn" type="button" onclick={() => settingsStore.close()} aria-label="Close settings">✕</button>
			</div>
		</header>

		{#if settingsStore.error}
			<div class="error-banner">{settingsStore.error}</div>
		{/if}

		{#if conflict}
			<div class="conflict-banner">
				<p>⚠️ Config was changed externally.</p>
				<div class="conflict-actions">
					<button type="button" onclick={() => settingsStore.acceptServerVersion()}>Reload</button>
					<button type="button" onclick={() => void settingsStore.overwriteConflict(vault)}>Overwrite</button>
				</div>
			</div>
		{/if}

		{#if cfg}
			<div class="settings-body">
				<!-- General -->
				<section class="config-section">
					<div class="section-header">
						<h3>General</h3>
						{#if sectionIsDirty('name') || sectionIsDirty('homepage')}
							<div class="section-actions">
								<button type="button" class="btn-save" onclick={() => void saveSection('name')}>Save</button>
								<button type="button" class="btn-revert" onclick={() => { revert('name'); revert('homepage'); }}>Revert</button>
							</div>
						{/if}
					</div>
					<label class="field">
						<span class="field-label">Vault Name</span>
						<input type="text" {...textField('name', cfg.name, (v) => { if (cfg) cfg.name = v; })} />
						{#if fieldError('name')}<span class="field-error">{fieldError('name')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Homepage</span>
						<input type="text" placeholder="e.g. Dashboard.md" {...textField('homepage', cfg.homepage, (v) => { if (cfg) cfg.homepage = v || null; })} />
					</label>
				</section>

				<!-- Inbox -->
				<section class="config-section">
					<div class="section-header">
						<h3>Inbox</h3>
						{#if sectionIsDirty('inbox')}
							<div class="section-actions">
								<button type="button" class="btn-save" onclick={() => void saveSection('inbox')}>Save</button>
								<button type="button" class="btn-revert" onclick={() => revert('inbox')}>Revert</button>
							</div>
						{/if}
					</div>
					<label class="field">
						<span class="field-label">Folder</span>
						<input type="text" {...textField('inbox', cfg.inbox.folder, (v) => { if (cfg) cfg.inbox.folder = v; })} />
						{#if fieldError('inbox.folder')}<span class="field-error">{fieldError('inbox.folder')}</span>{/if}
						{#if fieldWarning('inbox.folder')}<span class="field-warning">{fieldWarning('inbox.folder')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Template</span>
						<input type="text" {...textField('inbox', cfg.inbox.template, (v) => { if (cfg) cfg.inbox.template = v; })} />
					</label>
				</section>

				<!-- Daily Notes -->
				<section class="config-section">
					<div class="section-header">
						<h3>Daily Notes</h3>
						{#if sectionIsDirty('daily')}
							<div class="section-actions">
								<button type="button" class="btn-save" onclick={() => void saveSection('daily')}>Save</button>
								<button type="button" class="btn-revert" onclick={() => revert('daily')}>Revert</button>
							</div>
						{/if}
					</div>
					<label class="field">
						<span class="field-label">Folder</span>
						<input type="text" {...textField('daily', cfg.daily.folder, (v) => { if (cfg) cfg.daily.folder = v; })} />
						{#if fieldError('daily.folder')}<span class="field-error">{fieldError('daily.folder')}</span>{/if}
						{#if fieldWarning('daily.folder')}<span class="field-warning">{fieldWarning('daily.folder')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Template</span>
						<input type="text" {...textField('daily', cfg.daily.template, (v) => { if (cfg) cfg.daily.template = v; })} />
					</label>
					<label class="field">
						<span class="field-label">Generate At (HH:MM)</span>
						<input type="text" placeholder="e.g. 06:00" {...textField('daily', cfg.daily.generate_at, (v) => { if (cfg) cfg.daily.generate_at = v || null; })} />
						{#if fieldError('daily.generate_at')}<span class="field-error">{fieldError('daily.generate_at')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Timezone</span>
						<input type="text" placeholder="e.g. America/New_York" {...textField('daily', cfg.daily.timezone, (v) => { if (cfg) cfg.daily.timezone = v || null; })} />
						{#if fieldError('daily.timezone')}<span class="field-error">{fieldError('daily.timezone')}</span>{/if}
					</label>
					<label class="field field-toggle">
						<span class="field-label">Catch Up Missed Days</span>
						<input type="checkbox" {...toggleField('daily', cfg.daily.catch_up, (v) => { if (cfg) cfg.daily.catch_up = v; })} />
					</label>
				</section>

				<!-- Editor -->
				<section class="config-section">
					<div class="section-header">
						<h3>Editor</h3>
					</div>
					<label class="field field-toggle">
						<span class="field-label">Live Preview</span>
						<input type="checkbox" {...toggleField('editor', cfg.editor.live_preview, (v) => { if (cfg) cfg.editor.live_preview = v; })} />
					</label>
					<label class="field">
						<span class="field-label">Default Mode</span>
						<select {...selectField('editor', cfg.editor.default_mode, (v) => { if (cfg) cfg.editor.default_mode = v; })}>
							<option value="source">Source</option>
							<option value="reading">Reading</option>
							<option value="live-preview">Live Preview</option>
						</select>
					</label>
				</section>

				<!-- Git Sync -->
				<section class="config-section">
					<div class="section-header">
						<h3>Git Sync</h3>
						{#if sectionIsDirty('git')}
							<div class="section-actions">
								<button type="button" class="btn-save" onclick={() => void saveSection('git')}>Save</button>
								<button type="button" class="btn-revert" onclick={() => revert('git')}>Revert</button>
							</div>
						{/if}
					</div>
					<label class="field field-toggle">
						<span class="field-label">Enabled</span>
						<input type="checkbox" {...toggleField('git', cfg.git.enabled, (v) => { if (cfg) cfg.git.enabled = v; })} />
					</label>
					<label class="field">
						<span class="field-label">Auto-commit Interval</span>
						<input type="text" placeholder="e.g. 5m" {...textField('git', cfg.git.auto_commit_every, (v) => { if (cfg) cfg.git.auto_commit_every = v || null; })} />
						{#if fieldError('git.auto_commit_every')}<span class="field-error">{fieldError('git.auto_commit_every')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Auto-pull Interval</span>
						<input type="text" placeholder="e.g. 5m" {...textField('git', cfg.git.auto_pull_every, (v) => { if (cfg) cfg.git.auto_pull_every = v || null; })} />
						{#if fieldError('git.auto_pull_every')}<span class="field-error">{fieldError('git.auto_pull_every')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Auto-push Interval</span>
						<input type="text" placeholder="e.g. 5m" {...textField('git', cfg.git.auto_push_every, (v) => { if (cfg) cfg.git.auto_push_every = v || null; })} />
						{#if fieldError('git.auto_push_every')}<span class="field-error">{fieldError('git.auto_push_every')}</span>{/if}
					</label>
					<label class="field">
						<span class="field-label">Commit Message</span>
						<input type="text" placeholder="e.g. auto: sync changes" {...textField('git', cfg.git.commit_message, (v) => { if (cfg) cfg.git.commit_message = v || null; })} />
					</label>
				</section>

				<!-- Hooks -->
				<section class="config-section">
					<div class="section-header">
						<h3>Hooks</h3>
						{#if sectionIsDirty('hooks')}
							<div class="section-actions">
								<button type="button" class="btn-save" onclick={() => void saveSection('hooks')}>Save</button>
								<button type="button" class="btn-revert" onclick={() => revert('hooks')}>Revert</button>
							</div>
						{/if}
					</div>
					<label class="field">
						<span class="field-label">On Note Create</span>
						<input type="text" placeholder="shell command" {...textField('hooks', cfg.hooks.on_note_create, (v) => { if (cfg) cfg.hooks.on_note_create = v || null; })} />
					</label>
					<label class="field">
						<span class="field-label">On Daily Create</span>
						<input type="text" placeholder="shell command" {...textField('hooks', cfg.hooks.on_daily_create, (v) => { if (cfg) cfg.hooks.on_daily_create = v || null; })} />
					</label>
				</section>

				<!-- Sidebar Config (read-only) -->
				<section class="config-section">
					<div class="section-header">
						<h3>Sidebar</h3>
					</div>
					<p class="section-hint">
						Sidebar views are configured in <code>.notesmith/sidebar.yaml</code>.
						{#if caps?.can_open_local_paths}
							Edit the file directly in your editor.
						{/if}
					</p>
				</section>

				<!-- Config file info -->
				<footer class="config-footer">
					<span class="config-path">Config: {settingsStore.configPath}</span>
				</footer>
			</div>
		{:else if status !== 'loading'}
			<div class="settings-empty">
				{#if !caps?.can_edit_vault_config}
					<p>Vault config editing is not available in this deployment.</p>
				{:else}
					<p>No configuration loaded.</p>
				{/if}
			</div>
		{/if}
	</div>
</div>
{/if}

<style>
	.settings-backdrop {
		position: fixed;
		top: 0;
		right: 0;
		bottom: 0;
		left: 0;
		z-index: 100;
		display: flex;
		justify-content: flex-end;
		background: rgba(0, 0, 0, 0.3);
	}

	.settings-panel {
		width: 400px;
		max-width: 100vw;
		height: 100%;
		background: var(--bg-primary, #1e1e1e);
		border-left: 1px solid var(--border-color, #333);
		display: flex;
		flex-direction: column;
		animation: slide-in 200ms ease-out;
		overflow: hidden;
	}

	@keyframes slide-in {
		from { transform: translateX(100%); }
		to { transform: translateX(0); }
	}

	.settings-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border-color, #333);
		flex-shrink: 0;
	}

	.settings-header h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 600;
	}

	.header-right {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.status-badge {
		font-size: 12px;
		color: var(--text-muted, #888);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 18px;
		cursor: pointer;
		padding: 2px 6px;
		border-radius: 4px;
	}

	.close-btn:hover {
		background: var(--hover-bg, #2a2d2e);
		color: var(--text-primary, #e0e0e0);
	}

	.error-banner {
		padding: 10px 16px;
		background: #3a1a1a;
		color: #ff6b6b;
		border-bottom: 1px solid #5a2a2a;
		font-size: 13px;
	}

	.conflict-banner {
		padding: 10px 16px;
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
		padding-bottom: 16px;
	}

	.config-section {
		padding: 12px 16px;
		border-bottom: 1px solid var(--border-color, #333);
	}

	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 10px;
	}

	.section-header h3 {
		margin: 0;
		font-size: 13px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--text-primary, #e0e0e0);
	}

	.section-actions {
		display: flex;
		gap: 6px;
	}

	.btn-save,
	.btn-revert {
		padding: 3px 10px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #444);
		font-size: 11px;
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
		margin-bottom: 10px;
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
		font-size: 12px;
		margin: 0;
		line-height: 1.5;
	}

	.section-hint code {
		background: var(--bg-secondary, #2a2a2a);
		padding: 1px 4px;
		border-radius: 3px;
		font-size: 11px;
	}

	.config-footer {
		padding: 8px 16px;
		border-top: 1px solid var(--border-color, #333);
	}

	.config-path {
		font-size: 11px;
		color: var(--text-muted, #666);
	}

	.settings-empty {
		padding: 24px 16px;
		text-align: center;
		color: var(--text-muted, #888);
		font-size: 13px;
	}

	@media (max-width: 480px) {
		.settings-panel {
			width: 100vw;
		}
	}
</style>
