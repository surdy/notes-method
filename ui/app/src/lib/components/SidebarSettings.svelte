<script lang="ts">
	import {
		getSidebarConfigWithHash,
		putSidebarConfig,
		getVaultFolders,
		ApiError,
		type SidebarConfig,
		type SidebarView,
		type SidebarSection,
		type CustomItem,
		type SidebarConfigConflictError
	} from '$lib/api';

	interface Props {
		vault: string;
	}

	let { vault }: Props = $props();

	// ── Curated icon list ────────────────────────────────────────
	const ICON_OPTIONS = [
		{ value: 'inbox', label: 'Inbox' },
		{ value: 'file-text', label: 'File Text' },
		{ value: 'folder', label: 'Folder' },
		{ value: 'folder-open', label: 'Folder Open' },
		{ value: 'star', label: 'Star' },
		{ value: 'heart', label: 'Heart' },
		{ value: 'bookmark', label: 'Bookmark' },
		{ value: 'tag', label: 'Tag' },
		{ value: 'hash', label: 'Hash' },
		{ value: 'search', label: 'Search' },
		{ value: 'calendar', label: 'Calendar' },
		{ value: 'clock', label: 'Clock' },
		{ value: 'check-square', label: 'Check Square' },
		{ value: 'list', label: 'List' },
		{ value: 'archive', label: 'Archive' },
		{ value: 'home', label: 'Home' },
		{ value: 'settings', label: 'Settings' },
		{ value: 'layers', label: 'Layers' },
		{ value: 'database', label: 'Database' },
		{ value: 'code', label: 'Code' },
		{ value: 'book-open', label: 'Book Open' },
		{ value: 'layout', label: 'Layout' },
		{ value: 'grid', label: 'Grid' },
		{ value: 'zap', label: 'Zap' },
		{ value: 'alert-circle', label: 'Alert Circle' }
	];

	// ── State ────────────────────────────────────────────────────
	let config = $state<SidebarConfig | null>(null);
	let serverConfig = $state<SidebarConfig | null>(null);
	let etag = $state('');
	let status = $state<'idle' | 'loading' | 'saving'>('idle');
	let error = $state<string | null>(null);
	let conflict = $state<SidebarConfigConflictError | null>(null);
	let validationErrors = $state<Record<string, string>>({});
	let expandedViews = $state<Set<string>>(new Set());
	let dirtyViews = $state<Set<string>>(new Set());

	// Folder autocomplete
	let allFolders = $state<string[]>([]);
	let activeFolderInput = $state<string | null>(null);
	let folderQuery = $state('');
	let filteredFolders = $derived(
		folderQuery
			? allFolders.filter((f) => f.toLowerCase().includes(folderQuery.toLowerCase())).slice(0, 10)
			: []
	);

	// ── Load ─────────────────────────────────────────────────────
	async function load() {
		status = 'loading';
		error = null;
		conflict = null;
		validationErrors = {};
		try {
			const [result, folders] = await Promise.all([
				getSidebarConfigWithHash(vault),
				getVaultFolders(vault)
			]);
			config = structuredClone(result.config);
			serverConfig = structuredClone(result.config);
			etag = result.etag;
			allFolders = folders;
			dirtyViews = new Set();
			status = 'idle';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load sidebar config';
			status = 'idle';
		}
	}

	$effect(() => {
		if (vault) void load();
	});

	// ── Save ─────────────────────────────────────────────────────
	async function save() {
		if (!config) return;
		status = 'saving';
		error = null;
		conflict = null;
		validationErrors = {};
		try {
			const result = await putSidebarConfig(vault, config, etag);
			config = structuredClone(result.config);
			serverConfig = structuredClone(result.config);
			etag = result.etag;
			dirtyViews = new Set();
			status = 'idle';
		} catch (e) {
			if (e instanceof ApiError && e.status === 409) {
				const data = (e as ApiError & { conflict: SidebarConfigConflictError }).conflict;
				conflict = data;
				status = 'idle';
				return;
			}
			if (e instanceof ApiError && e.status === 422) {
				const data = (e as ApiError & { validation: { errors: Record<string, string> } })
					.validation;
				validationErrors = data.errors ?? {};
				status = 'idle';
				return;
			}
			error = e instanceof Error ? e.message : 'Failed to save sidebar config';
			status = 'idle';
		}
	}

	function acceptServerVersion() {
		if (conflict) {
			config = structuredClone(conflict.config);
			serverConfig = structuredClone(conflict.config);
			etag = conflict.hash;
			conflict = null;
			dirtyViews = new Set();
		}
	}

	async function overwriteConflict() {
		if (conflict && config) {
			etag = conflict.hash;
			conflict = null;
			await save();
		}
	}

	function revertView(viewId: string) {
		if (!config || !serverConfig) return;
		const idx = config.views.findIndex((v) => v.id === viewId);
		const serverView = serverConfig.views.find((v) => v.id === viewId);
		if (idx >= 0 && serverView) {
			config.views[idx] = structuredClone(serverView);
			config = { views: config!.views };
			const d = new Set(dirtyViews);
			d.delete(viewId);
			dirtyViews = d;
		}
	}

	function markViewDirty(viewId: string) {
		if (!dirtyViews.has(viewId)) {
			dirtyViews = new Set([...dirtyViews, viewId]);
		}
	}

	// ── View CRUD ────────────────────────────────────────────────
	function toggleView(viewId: string) {
		const updated = new Set(expandedViews);
		if (updated.has(viewId)) updated.delete(viewId);
		else updated.add(viewId);
		expandedViews = updated;
	}

	function addView() {
		if (!config) return;
		const id = `view-${Date.now()}`;
		const newView: SidebarView = {
			id,
			name: 'New View',
			icon: 'file-text',
			sections: [],
			badge_query: undefined
		};
		config = { views: [...config.views, newView] };
		expandedViews = new Set([...expandedViews, id]);
		markViewDirty(id);
	}

	function removeView(idx: number) {
		if (!config) return;
		const view = config.views[idx];
		if (!window.confirm(`Remove view "${view.name}"?`)) return;
		const views = [...config.views];
		views.splice(idx, 1);
		config = { views };
		markViewDirty(view.id);
	}

	function moveView(idx: number, dir: -1 | 1) {
		if (!config) return;
		const target = idx + dir;
		if (target < 0 || target >= config.views.length) return;
		const views = [...config.views];
		[views[idx], views[target]] = [views[target], views[idx]];
		config = { views };
		markViewDirty(views[idx].id);
		markViewDirty(views[target].id);
	}

	// ── Section CRUD ─────────────────────────────────────────────
	function addSection(viewIdx: number, type: string) {
		if (!config) return;
		const view = config.views[viewIdx];
		let section: SidebarSection;
		if (type === 'recently-viewed') {
			section = { type: 'recently-viewed', label: 'Recently Viewed', mode: 'both', limit: 10 };
		} else if (type === 'custom-folders') {
			section = { type: 'custom-folders', label: 'Folders', folders: [] };
		} else {
			section = { type: 'custom-items', label: 'Items', items: [] };
		}
		view.sections = [...view.sections, section];
		config = { views: config!.views };
		markViewDirty(view.id);
	}

	function removeSection(viewIdx: number, secIdx: number) {
		if (!config) return;
		const view = config.views[viewIdx];
		view.sections = view.sections.filter((_, i) => i !== secIdx);
		config = { views: config!.views };
		markViewDirty(view.id);
	}

	function moveSection(viewIdx: number, secIdx: number, dir: -1 | 1) {
		if (!config) return;
		const view = config.views[viewIdx];
		const target = secIdx + dir;
		if (target < 0 || target >= view.sections.length) return;
		const sections = [...view.sections];
		[sections[secIdx], sections[target]] = [sections[target], sections[secIdx]];
		view.sections = sections;
		config = { views: config!.views };
		markViewDirty(view.id);
	}

	// ── Folder management within custom-folders section ──────────
	function addFolder(viewIdx: number, secIdx: number) {
		if (!config) return;
		const section = config.views[viewIdx].sections[secIdx];
		if (section.type === 'custom-folders') {
			section.folders = [...section.folders, ''];
			config = { views: config!.views };
			markViewDirty(config.views[viewIdx].id);
		}
	}

	function removeFolder(viewIdx: number, secIdx: number, folderIdx: number) {
		if (!config) return;
		const section = config.views[viewIdx].sections[secIdx];
		if (section.type === 'custom-folders') {
			section.folders = section.folders.filter((_, i) => i !== folderIdx);
			config = { views: config!.views };
			markViewDirty(config.views[viewIdx].id);
		}
	}

	function updateFolder(viewIdx: number, secIdx: number, folderIdx: number, value: string) {
		if (!config) return;
		const section = config.views[viewIdx].sections[secIdx];
		if (section.type === 'custom-folders') {
			section.folders[folderIdx] = value;
			config = { views: config!.views };
			markViewDirty(config.views[viewIdx].id);
		}
	}

	// ── Custom items management ─────────────────────────────────
	function addItem(viewIdx: number, secIdx: number) {
		if (!config) return;
		const section = config.views[viewIdx].sections[secIdx];
		if (section.type === 'custom-items') {
			const newItem: CustomItem = {
				name: 'New Item',
				icon: 'file-text',
				source: { folder: '', recursive: false, sort: 'modified', sort_dir: 'desc' }
			};
			section.items = [...section.items, newItem];
			config = { views: config!.views };
			markViewDirty(config.views[viewIdx].id);
		}
	}

	function removeItem(viewIdx: number, secIdx: number, itemIdx: number) {
		if (!config) return;
		const section = config.views[viewIdx].sections[secIdx];
		if (section.type === 'custom-items') {
			section.items = section.items.filter((_, i) => i !== itemIdx);
			config = { views: config!.views };
			markViewDirty(config.views[viewIdx].id);
		}
	}

	// ── Folder autocomplete helpers ──────────────────────────────
	function startFolderAutocomplete(inputId: string) {
		activeFolderInput = inputId;
		folderQuery = '';
	}

	function closeFolderAutocomplete() {
		// Small delay so click on dropdown item registers
		setTimeout(() => {
			activeFolderInput = null;
			folderQuery = '';
		}, 150);
	}

	function selectFolderSuggestion(value: string, viewIdx: number, secIdx: number, folderIdx: number) {
		updateFolder(viewIdx, secIdx, folderIdx, value);
		activeFolderInput = null;
		folderQuery = '';
	}
</script>

{#if error}
	<div class="error-banner">{error}</div>
{/if}

{#if conflict}
	<div class="conflict-banner">
		<p>⚠️ Sidebar config was changed externally.</p>
		<div class="conflict-actions">
			<button type="button" onclick={acceptServerVersion}>Reload</button>
			<button type="button" onclick={() => void overwriteConflict()}>Overwrite</button>
		</div>
	</div>
{/if}

{#if Object.keys(validationErrors).length > 0}
	<div class="error-banner">
		{#each Object.entries(validationErrors) as [key, msg]}
			<div>{key}: {msg}</div>
		{/each}
	</div>
{/if}

{#if config}
	<div class="sidebar-editor">
		{#each config.views as view, viewIdx (view.id)}
			<div class="view-card" class:expanded={expandedViews.has(view.id)}>
				<div class="view-header">
					<button
						class="view-toggle"
						type="button"
						onclick={() => toggleView(view.id)}
						aria-label={expandedViews.has(view.id) ? 'Collapse' : 'Expand'}
					>
						<span class="chevron">{expandedViews.has(view.id) ? '▾' : '▸'}</span>
						<span class="view-name">{view.name}</span>
						{#if dirtyViews.has(view.id)}
							<span class="dirty-dot" title="Unsaved changes">●</span>
						{/if}
					</button>
					<div class="view-controls">
						<button
							type="button"
							class="icon-btn"
							disabled={viewIdx === 0}
							onclick={() => moveView(viewIdx, -1)}
							title="Move up"
						>↑</button>
						<button
							type="button"
							class="icon-btn"
							disabled={viewIdx === config.views.length - 1}
							onclick={() => moveView(viewIdx, 1)}
							title="Move down"
						>↓</button>
						<button
							type="button"
							class="icon-btn danger"
							onclick={() => removeView(viewIdx)}
							title="Remove view"
						>✕</button>
					</div>
				</div>

				{#if expandedViews.has(view.id)}
					<div class="view-body">
						<div class="view-props">
							<label class="field">
								<span class="field-label">Name</span>
								<input
									type="text"
									autocapitalize="off"
									value={view.name}
									oninput={(e) => {
										view.name = (e.target as HTMLInputElement).value;
										config = { views: config!.views };
										markViewDirty(view.id);
									}}
								/>
							</label>
							<label class="field">
								<span class="field-label">Icon</span>
								<select
									value={view.icon}
									onchange={(e) => {
										view.icon = (e.target as HTMLSelectElement).value;
										config = { views: config!.views };
										markViewDirty(view.id);
									}}
								>
									{#each ICON_OPTIONS as opt}
										<option value={opt.value}>{opt.label}</option>
									{/each}
								</select>
							</label>
							<label class="field">
								<span class="field-label">Badge Query (optional)</span>
								<input
									type="text"
									autocapitalize="off"
									value={view.badge_query ?? ''}
									placeholder="e.g. SELECT COUNT(*) FROM tasks WHERE status='todo'"
									oninput={(e) => {
										const val = (e.target as HTMLInputElement).value;
										view.badge_query = val || undefined;
										config = { views: config!.views };
										markViewDirty(view.id);
									}}
								/>
							</label>
						</div>

						<h4 class="subsection-title">Sections</h4>

						{#each view.sections as section, secIdx}
							<div class="section-card">
								<div class="section-header">
									<span class="section-type">{section.type}</span>
									<div class="section-controls">
										<button
											type="button"
											class="icon-btn"
											disabled={secIdx === 0}
											onclick={() => moveSection(viewIdx, secIdx, -1)}
											title="Move up"
										>↑</button>
										<button
											type="button"
											class="icon-btn"
											disabled={secIdx === view.sections.length - 1}
											onclick={() => moveSection(viewIdx, secIdx, 1)}
											title="Move down"
										>↓</button>
										<button
											type="button"
											class="icon-btn danger"
											onclick={() => removeSection(viewIdx, secIdx)}
											title="Remove section"
										>✕</button>
									</div>
								</div>

								<div class="section-fields">
									<label class="field">
										<span class="field-label">Label</span>
										<input
											type="text"
											autocapitalize="off"
											value={section.label}
											oninput={(e) => {
												section.label = (e.target as HTMLInputElement).value;
												config = { views: config!.views };
												markViewDirty(view.id);
											}}
										/>
									</label>

									{#if section.type === 'recently-viewed'}
										<label class="field">
											<span class="field-label">Mode</span>
											<select
												value={section.mode}
												onchange={(e) => {
													if (section.type === 'recently-viewed')
														section.mode = (e.target as HTMLSelectElement)
															.value as 'viewed' | 'edited' | 'both';
													config = { views: config!.views };
													markViewDirty(view.id);
												}}
											>
												<option value="viewed">Viewed</option>
												<option value="edited">Edited</option>
												<option value="both">Both</option>
											</select>
										</label>
										<label class="field">
											<span class="field-label">Limit</span>
											<input
												type="number"
												min="1"
												max="50"
												value={section.limit}
												oninput={(e) => {
													if (section.type === 'recently-viewed')
														section.limit =
															parseInt((e.target as HTMLInputElement).value) || 10;
													config = { views: config!.views };
													markViewDirty(view.id);
												}}
											/>
										</label>
									{:else if section.type === 'custom-folders'}
										<div class="folder-list">
											{#each section.folders as folder, folderIdx}
												{@const inputId = `folder-${viewIdx}-${secIdx}-${folderIdx}`}
												<div class="folder-row">
													<div class="autocomplete-wrapper">
														<input
															type="text"
															autocapitalize="off"
															value={folder}
															placeholder="Folder path"
															oninput={(e) => {
																const val = (e.target as HTMLInputElement).value;
																updateFolder(viewIdx, secIdx, folderIdx, val);
																folderQuery = val;
															}}
															onfocus={() => startFolderAutocomplete(inputId)}
															onblur={closeFolderAutocomplete}
														/>
														{#if activeFolderInput === inputId && filteredFolders.length > 0}
															<ul class="autocomplete-dropdown">
																{#each filteredFolders as suggestion}
																	<li>
																		<button
																			type="button"
																			onmousedown={() =>
																				selectFolderSuggestion(
																					suggestion,
																					viewIdx,
																					secIdx,
																					folderIdx
																				)}
																		>
																			{suggestion}
																		</button>
																	</li>
																{/each}
															</ul>
														{/if}
													</div>
													<button
														type="button"
														class="icon-btn danger"
														onclick={() => removeFolder(viewIdx, secIdx, folderIdx)}
														title="Remove folder"
													>✕</button>
												</div>
											{/each}
											<button
												type="button"
												class="btn-add-small"
												onclick={() => addFolder(viewIdx, secIdx)}
											>+ Add Folder</button>
										</div>
									{:else if section.type === 'custom-items'}
										<div class="items-list">
											{#each section.items as item, itemIdx}
												<div class="item-card">
													<div class="item-header">
														<span class="item-name">{item.name}</span>
														<button
															type="button"
															class="icon-btn danger"
															onclick={() => removeItem(viewIdx, secIdx, itemIdx)}
															title="Remove item"
														>✕</button>
													</div>
													<label class="field">
														<span class="field-label">Name</span>
														<input
															type="text"
															autocapitalize="off"
															value={item.name}
															oninput={(e) => {
																item.name = (e.target as HTMLInputElement).value;
																config = { views: config!.views };
																markViewDirty(view.id);
															}}
														/>
													</label>
													<label class="field">
														<span class="field-label">Icon</span>
														<select
															value={item.icon}
															onchange={(e) => {
																item.icon = (e.target as HTMLSelectElement).value;
																config = { views: config!.views };
																markViewDirty(view.id);
															}}
														>
															{#each ICON_OPTIONS as opt}
																<option value={opt.value}>{opt.label}</option>
															{/each}
														</select>
													</label>
													{#if 'folder' in item.source}
														<label class="field">
															<span class="field-label">Folder</span>
															<input
																type="text"
																autocapitalize="off"
																value={item.source.folder}
																placeholder="Folder path"
																oninput={(e) => {
																	if ('folder' in item.source) {
																		item.source.folder = (
																			e.target as HTMLInputElement
																		).value;
																		config = { views: config!.views };
																		markViewDirty(view.id);
																	}
																}}
															/>
														</label>
													{:else if 'query' in item.source}
														<label class="field">
															<span class="field-label">Query</span>
															<input
																type="text"
																autocapitalize="off"
																value={item.source.query}
																placeholder="SQL query"
																oninput={(e) => {
																	if ('query' in item.source) {
																		item.source.query = (
																			e.target as HTMLInputElement
																		).value;
																		config = { views: config!.views };
																		markViewDirty(view.id);
																	}
																}}
															/>
														</label>
													{/if}
												</div>
											{/each}
											<button
												type="button"
												class="btn-add-small"
												onclick={() => addItem(viewIdx, secIdx)}
											>+ Add Item</button>
										</div>
									{/if}
								</div>
							</div>
						{/each}

						<div class="add-section-row">
							<span class="add-label">Add section:</span>
							<button
								type="button"
								class="btn-add-small"
								onclick={() => addSection(viewIdx, 'recently-viewed')}
							>Recently Viewed</button>
							<button
								type="button"
								class="btn-add-small"
								onclick={() => addSection(viewIdx, 'custom-folders')}
							>Custom Folders</button>
							<button
								type="button"
								class="btn-add-small"
								onclick={() => addSection(viewIdx, 'custom-items')}
							>Custom Items</button>
						</div>

						{#if dirtyViews.has(view.id)}
							<div class="view-save-row">
								<button type="button" class="btn-save" onclick={() => void save()}
									>Save All</button
								>
								<button
									type="button"
									class="btn-revert"
									onclick={() => revertView(view.id)}>Revert View</button
								>
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{/each}

		<button type="button" class="btn-add-view" onclick={addView}>+ Add View</button>

		{#if dirtyViews.size > 0}
			<div class="global-save-row">
				<button
					type="button"
					class="btn-save"
					disabled={status === 'saving'}
					onclick={() => void save()}
				>
					{status === 'saving' ? 'Saving…' : 'Save All Changes'}
				</button>
			</div>
		{/if}
	</div>
{:else if status === 'loading'}
	<p class="loading">Loading sidebar config…</p>
{/if}

<style>
	.error-banner {
		padding: 10px 0;
		color: var(--color-danger);
		font-size: 13px;
	}

	.conflict-banner {
		padding: 10px 0;
		color: var(--color-warning);
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

	.sidebar-editor {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.view-card {
		border: 1px solid var(--border-default);
		border-radius: 6px;
		overflow: hidden;
	}

	.view-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		background: var(--bg-secondary);
	}

	.view-toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		background: none;
		border: none;
		color: var(--text-default);
		font-size: 13px;
		cursor: pointer;
		padding: 0;
	}

	.chevron {
		font-size: 11px;
		width: 12px;
	}

	.view-name {
		font-weight: 500;
	}

	.dirty-dot {
		color: var(--color-warning);
		font-size: 10px;
	}

	.view-controls,
	.section-controls {
		display: flex;
		gap: 4px;
	}

	.icon-btn {
		background: none;
		border: 1px solid transparent;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
		padding: 2px 6px;
		border-radius: 3px;
	}

	.icon-btn:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.icon-btn:disabled {
		opacity: 0.3;
		cursor: default;
	}

	.icon-btn.danger:hover:not(:disabled) {
		color: var(--color-danger);
	}

	.view-body {
		padding: 12px;
		border-top: 1px solid var(--border-default);
	}

	.view-props {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-bottom: 16px;
	}

	.subsection-title {
		margin: 0 0 8px;
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
	}

	.section-card {
		border: 1px solid var(--border-default);
		border-radius: 4px;
		margin-bottom: 8px;
		overflow: hidden;
	}

	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 10px;
		background: var(--surface-translucent-alt);
	}

	.section-type {
		font-size: 12px;
		font-weight: 500;
		color: var(--text-muted);
	}

	.section-fields {
		padding: 8px 10px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 3px;
		margin-bottom: 8px;
	}

	.field-label {
		font-size: 11px;
		color: var(--text-muted);
	}

	.field input[type='text'],
	.field input[type='number'],
	.field select {
		padding: 5px 8px;
		border: 1px solid var(--border-strong);
		border-radius: 4px;
		background: var(--bg-secondary);
		color: var(--text-default);
		font-size: 12px;
		max-width: 360px;
	}

	.field input:focus,
	.field select:focus {
		outline: none;
		border-color: var(--accent-bg);
	}

	.folder-list,
	.items-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.folder-row {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.autocomplete-wrapper {
		position: relative;
		flex: 1;
		max-width: 320px;
	}

	.autocomplete-wrapper input {
		width: 100%;
		padding: 5px 8px;
		border: 1px solid var(--border-strong);
		border-radius: 4px;
		background: var(--bg-secondary);
		color: var(--text-default);
		font-size: 12px;
	}

	.autocomplete-dropdown {
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		background: var(--bg-default);
		border: 1px solid var(--border-strong);
		border-radius: 0 0 4px 4px;
		list-style: none;
		margin: 0;
		padding: 0;
		z-index: 10;
		max-height: 180px;
		overflow-y: auto;
	}

	.autocomplete-dropdown li button {
		display: block;
		width: 100%;
		padding: 5px 8px;
		background: none;
		border: none;
		color: var(--text-default);
		font-size: 12px;
		text-align: left;
		cursor: pointer;
	}

	.autocomplete-dropdown li button:hover {
		background: var(--bg-hover);
	}

	.item-card {
		border: 1px solid var(--border-default);
		border-radius: 4px;
		padding: 8px;
		margin-bottom: 4px;
	}

	.item-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 6px;
	}

	.item-name {
		font-size: 12px;
		font-weight: 500;
	}

	.btn-add-small {
		background: none;
		border: 1px dashed var(--border-strong);
		color: var(--text-muted);
		font-size: 11px;
		padding: 4px 10px;
		border-radius: 4px;
		cursor: pointer;
	}

	.btn-add-small:hover {
		color: var(--text-default);
		border-color: var(--text-muted);
	}

	.add-section-row {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: 8px;
	}

	.add-label {
		font-size: 11px;
		color: var(--text-muted);
	}

	.view-save-row,
	.global-save-row {
		display: flex;
		gap: 6px;
		margin-top: 10px;
		padding-top: 10px;
		border-top: 1px solid var(--border-default);
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

	.btn-save:hover {
		background: var(--accent-hover);
	}

	.btn-save:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.btn-revert {
		background: transparent;
		color: var(--text-muted);
	}

	.btn-revert:hover {
		background: var(--bg-hover);
		color: var(--text-default);
	}

	.btn-add-view {
		padding: 8px 16px;
		border: 1px dashed var(--border-strong);
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
		cursor: pointer;
	}

	.btn-add-view:hover {
		border-color: var(--text-muted);
		color: var(--text-default);
	}

	.loading {
		color: var(--text-muted);
		font-size: 13px;
	}
</style>
