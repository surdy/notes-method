<script lang="ts">
	import { onDestroy, tick } from 'svelte';

	import type { NoteSummary } from '$lib/api';
	import type { Command } from '$lib/commands';
	import { fuzzyFilter } from '$lib/fuzzy';
	import { noteIcon } from '$lib/note-icons';
	import { getRecentlyViewed } from '$lib/recently-viewed';
	import { settingsStore } from '$lib/settings.svelte';
	import { themeStore, type ThemeEntry, type VisualMode } from '$lib/theme.svelte';
	import { filterThemes, findThemeByName } from '$lib/theme-picker';
	import { toastStore } from '$lib/toast-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';

	type Mode = 'files' | 'commands' | 'themes';

	type FileItem = {
		kind: 'file';
		id: string;
		note: NoteSummary;
		title: string;
		folderPath: string;
	};

	type CreateItem = {
		kind: 'create';
		id: string;
		title: string;
	};

	type CommandItem = {
		kind: 'command';
		id: string;
		command: Command;
	};

	type ThemeItem = {
		kind: 'theme';
		id: string;
		theme: ThemeEntry;
	};

	type ThemeSnapshot = {
		theme: string;
		followSystem: boolean;
		darkTheme: string;
		lightTheme: string;
		visualMode: VisualMode;
	};

	type PaletteItem = FileItem | CreateItem | CommandItem | ThemeItem;

	const RECENT_COMMAND_LIMIT = 10;
	const themeCatalog = themeStore.getCatalog();

	let { commands, initialMode, onClose, onSelectNote, onCreateNote }:
		{
			commands: Command[];
			initialMode: 'files' | 'commands';
			onClose: () => void;
			onSelectNote: (path: string) => void;
			onCreateNote: (title: string) => void;
		} = $props();

	let rawInput = $state('');
	let selectedIndex = $state(0);
	let inputRef: HTMLInputElement | undefined;
	let resultsRef: HTMLDivElement | undefined;
	let recentCommandIds = $state<string[]>(loadRecentCommandIds(vaultStore.currentVault));
	let themePickerMode = $state(false);
	let themeQueryDirty = $state(false);
	let previewedThemeName = $state<string | null>(null);
	let themeRestore: (() => void) | null = null;
	let initialized = false;

	function recentCommandsKey(vault: string): string | null {
		return vault ? `notesmith:recent-commands:${vault}` : null;
	}

	function loadRecentCommandIds(vault: string): string[] {
		if (!vault || typeof localStorage === 'undefined') {
			return [];
		}

		try {
			const stored = localStorage.getItem(recentCommandsKey(vault) ?? '');
			if (!stored) return [];
			const parsed = JSON.parse(stored);
			return Array.isArray(parsed)
				? parsed
						.filter((value): value is string => typeof value === 'string')
						.slice(0, RECENT_COMMAND_LIMIT)
				: [];
		} catch {
			return [];
		}
	}

	function saveRecentCommandIds(vault: string, ids: string[]) {
		if (typeof localStorage === 'undefined') {
			return;
		}

		const key = recentCommandsKey(vault);
		if (!key) {
			return;
		}

		try {
			localStorage.setItem(key, JSON.stringify(ids.slice(0, RECENT_COMMAND_LIMIT)));
		} catch {
			// ignore storage errors
		}
	}

	function recordRecentCommand(commandId: string) {
		const vault = vaultStore.currentVault;
		const nextIds = [commandId, ...recentCommandIds.filter((id) => id !== commandId)].slice(
			0,
			RECENT_COMMAND_LIMIT
		);
		recentCommandIds = nextIds;
		saveRecentCommandIds(vault, nextIds);
	}

	function displayTitle(note: NoteSummary): string {
		return note.title || pathBasename(note.path);
	}

	function pathBasename(path: string): string {
		return path.split('/').at(-1)?.replace(/\.md$/, '') || path;
	}

	function folderPath(path: string): string {
		const parts = path.split('/');
		return parts.slice(0, -1).join('/') || 'Vault root';
	}

	function fallbackNote(path: string, title: string): NoteSummary {
		return {
			path,
			title: title || pathBasename(path),
			tags: [],
			frontmatter: null
		};
	}

	function normalize(value: string): string {
		return value.trim().toLowerCase();
	}

	function currentThemeEntry(): ThemeEntry | undefined {
		return findThemeByName(themeCatalog, themeStore.activeTheme);
	}

	function clearThemePreview(revert = true) {
		if (themeRestore && revert) {
			themeRestore();
		}
		themeRestore = null;
		previewedThemeName = null;
	}

	async function enterThemePicker() {
		themePickerMode = true;
		themeQueryDirty = false;
		rawInput = currentThemeEntry()?.display_name ?? themeStore.activeTheme;
		selectedIndex = Math.max(
			themeCatalog.findIndex((theme) => theme.name === themeStore.activeTheme),
			0
		);
		await tick();
		inputRef?.focus();
		inputRef?.select();
	}

	function exitThemePicker() {
		clearThemePreview();
		themePickerMode = false;
		themeQueryDirty = false;
		rawInput = '> ';
		selectedIndex = 0;
	}

	function closePalette() {
		clearThemePreview();
		themePickerMode = false;
		themeQueryDirty = false;
		onClose();
	}

	async function persistThemeSelection(theme: ThemeEntry, previousState: ThemeSnapshot) {
		themeStore.setTheme(theme.name);
		themeStore.setFollowSystem(false);

		const vault = vaultStore.currentVault;
		if (!vault || !settingsStore.draftConfig) {
			toastStore.add(`Theme changed to ${theme.display_name}.`, 'success');
			return;
		}

		const previousAppearance = settingsStore.draftConfig.appearance
			? structuredClone(settingsStore.draftConfig.appearance)
			: undefined;
		const wasAppearanceDirty = settingsStore.dirtySections.has('appearance');
		settingsStore.draftConfig.appearance = {
			...settingsStore.draftConfig.appearance,
			theme: theme.name,
			followSystem: false,
			darkTheme: themeStore.darkTheme,
			lightTheme: themeStore.lightTheme,
			visualMode: themeStore.visualMode
		};
		settingsStore.markDirty('appearance');

		const saved = await settingsStore.saveConfig(vault);
		if (!saved) {
			themeStore.applyFromConfig(previousState);
			settingsStore.draftConfig.appearance = previousAppearance
				? structuredClone(previousAppearance)
				: undefined;
			if (!wasAppearanceDirty) {
				settingsStore.markClean('appearance');
			}
			toastStore.add(settingsStore.error ?? 'Failed to save theme selection.', 'error');
			return;
		}

		toastStore.add(`Theme changed to ${theme.display_name}.`, 'success');
	}

	function handleInput() {
		if (themePickerMode) {
			themeQueryDirty = true;
		}
	}

	function selectItem(item: PaletteItem) {
		if (item.kind === 'file') {
			onSelectNote(item.note.path);
			onClose();
			return;
		}

		if (item.kind === 'create') {
			onCreateNote(item.title);
			onClose();
			return;
		}

		if (item.kind === 'theme') {
			const previousState: ThemeSnapshot = {
				theme: themeStore.theme,
				followSystem: themeStore.followSystem,
				darkTheme: themeStore.darkTheme,
				lightTheme: themeStore.lightTheme,
				visualMode: themeStore.visualMode
			};
			clearThemePreview(false);
			themePickerMode = false;
			themeQueryDirty = false;
			onClose();
			void persistThemeSelection(item.theme, previousState);
			return;
		}

		recordRecentCommand(item.command.id);
		if (item.command.id === 'change-theme') {
			void enterThemePicker();
			return;
		}

		onClose();
		void Promise.resolve(item.command.execute()).catch((error) => {
			console.error('Command failed', error);
		});
	}

	function handleKeydown(event: KeyboardEvent) {
		switch (event.key) {
			case 'ArrowDown':
				event.preventDefault();
				selectedIndex = results.length === 0 ? 0 : Math.min(selectedIndex + 1, results.length - 1);
				break;
			case 'ArrowUp':
				event.preventDefault();
				selectedIndex = results.length === 0 ? 0 : Math.max(selectedIndex - 1, 0);
				break;
			case 'Enter':
				event.preventDefault();
				if (results[selectedIndex]) {
					selectItem(results[selectedIndex]);
				}
				break;
			case 'Escape':
				event.preventDefault();
				if (themePickerMode) {
					exitThemePicker();
					break;
				}
				closePalette();
				break;
		}
	}

	let activeMode = $derived<Mode>(
		themePickerMode ? 'themes' : rawInput.startsWith('> ') ? 'commands' : 'files'
	);
	let query = $derived.by(() => {
		if (activeMode === 'commands') {
			return rawInput.slice(2);
		}
		if (activeMode === 'themes') {
			return themeQueryDirty ? rawInput : '';
		}
		return rawInput;
	});
	let trimmedQuery = $derived(query.trim());

	let fileResults = $derived.by((): (FileItem | CreateItem)[] => {
		const currentQuery = trimmedQuery;
		const notes = currentQuery
			? fuzzyFilter(currentQuery, vaultStore.notes, (note) => `${displayTitle(note)} ${note.path}`)
					.slice(0, 20)
					.map((match) => match.item)
			: (typeof localStorage === 'undefined'
					? []
					: getRecentlyViewed(vaultStore.currentVault, 10).map((entry) => {
							return (
								vaultStore.notes.find((note) => note.path === entry.path) ??
								fallbackNote(entry.path, entry.title)
							);
						}));

		const items: (FileItem | CreateItem)[] = notes.map((note) => ({
			kind: 'file',
			id: note.path,
			note,
			title: displayTitle(note),
			folderPath: folderPath(note.path)
		}));

		if (
			currentQuery &&
			!vaultStore.notes.some((note) => normalize(displayTitle(note)) === normalize(currentQuery))
		) {
			items.push({
				kind: 'create',
				id: `create:${normalize(currentQuery)}`,
				title: currentQuery
			});
		}

		return items;
	});

	let commandResults = $derived.by((): CommandItem[] => {
		const currentQuery = trimmedQuery;
		if (!currentQuery) {
			return recentCommandIds
				.map((id) => commands.find((command) => command.id === id))
				.filter((command): command is Command => !!command)
				.slice(0, RECENT_COMMAND_LIMIT)
				.map((command): CommandItem => ({
					kind: 'command',
					id: command.id,
					command
				}));
		}

		const recentBoost = new Map(
			recentCommandIds.map((id, index) => [id, RECENT_COMMAND_LIMIT - index])
		);
		return fuzzyFilter(currentQuery, commands, (command) => `${command.label} ${command.category}`)
			.map((match) => ({
				match,
				score: match.score + (recentBoost.get(match.item.id) ?? 0)
			}))
			.sort((left, right) => {
				if (right.score !== left.score) {
					return right.score - left.score;
				}

				return left.match.item.label.localeCompare(right.match.item.label);
			})
			.map(({ match }): CommandItem => ({
				kind: 'command',
				id: match.item.id,
				command: match.item
			}));
	});

	let themeResults = $derived.by((): ThemeItem[] =>
		filterThemes(themeCatalog, trimmedQuery).map((theme) => ({
			kind: 'theme',
			id: `theme:${theme.name}`,
			theme
		}))
	);

	let results = $derived.by((): PaletteItem[] => {
		if (activeMode === 'commands') return [...commandResults];
		if (activeMode === 'themes') return [...themeResults];
		return [...fileResults];
	});

	$effect(() => {
		initialMode;
		if (!initialized) {
			rawInput = initialMode === 'commands' ? '> ' : '';
			initialized = true;
		}
	});

	$effect(() => {
		vaultStore.currentVault;
		recentCommandIds = loadRecentCommandIds(vaultStore.currentVault);
	});

	$effect(() => {
		if (themePickerMode && !themeQueryDirty) {
			return;
		}
		trimmedQuery;
		selectedIndex = 0;
	});

	$effect(() => {
		if (selectedIndex >= results.length) {
			selectedIndex = Math.max(results.length - 1, 0);
		}
	});

	$effect(() => {
		inputRef?.focus();
	});

	$effect(() => {
		selectedIndex;
		results;
		const selected = resultsRef?.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`);
		selected?.scrollIntoView({ block: 'nearest' });
	});

	$effect(() => {
		if (!themePickerMode) {
			return;
		}

		const selectedTheme = themeResults[selectedIndex]?.theme;
		if (!selectedTheme) {
			clearThemePreview();
			return;
		}

		if (previewedThemeName === selectedTheme.name) {
			return;
		}

		clearThemePreview();
		themeRestore = themeStore.preview(selectedTheme.name);
		previewedThemeName = selectedTheme.name;
	});

	onDestroy(() => {
		clearThemePreview();
	});
</script>

<div
	class="palette-backdrop"
	onclick={(event) => event.target === event.currentTarget && closePalette()}
	onkeydown={handleKeydown}
	role="dialog"
	aria-modal="true"
	tabindex="-1"
>
	<div class="palette">
		<div class="palette-header">
			<span class="mode-pill">
				{activeMode === 'files' ? 'Files' : activeMode === 'commands' ? 'Commands' : 'Themes'}
			</span>
			<input
				bind:this={inputRef}
				bind:value={rawInput}
				class="palette-input"
				oninput={handleInput}
				placeholder={
					activeMode === 'files'
						? 'Open a note...'
						: activeMode === 'commands'
							? 'Type a command...'
							: 'Filter themes by name, tone, or tags...'
				}
				type="text"
			/>
		</div>

		<div bind:this={resultsRef} class="palette-results">
			{#if results.length === 0}
				<div class="no-results">No matches</div>
			{:else}
				{#each results as item, index (item.id)}
					<button
						class="palette-item"
						class:selected={index === selectedIndex}
						class:theme-item={item.kind === 'theme'}
						data-index={index}
						onclick={() => selectItem(item)}
						onmouseenter={() => (selectedIndex = index)}
						type="button"
					>
						{#if item.kind === 'command'}
							<span class="item-label">{item.command.label}</span>
							<span class="item-meta">
								<span class="cmd-category">{item.command.category}</span>
								{#if item.command.shortcut}
									<kbd class="item-shortcut">{item.command.shortcut}</kbd>
								{/if}
							</span>
						{:else if item.kind === 'file'}
							<span class="file-row">
								<span class="file-icon">{noteIcon(item.note)}</span>
								<span class="file-body">
									<span class="file-title">{item.title}</span>
									<span class="file-path">{item.folderPath}</span>
								</span>
							</span>
						{:else if item.kind === 'theme'}
							<span class="theme-row">
								<span class="theme-swatch-strip" aria-hidden="true">
									<span class="theme-swatch" style={`background: ${item.theme.palette.bg}`}></span>
									<span class="theme-swatch" style={`background: ${item.theme.palette.fg}`}></span>
									<span class="theme-swatch" style={`background: ${item.theme.palette.blue}`}></span>
									<span class="theme-swatch" style={`background: ${item.theme.palette.red}`}></span>
									<span class="theme-swatch" style={`background: ${item.theme.palette.green}`}></span>
								</span>
								<span class="theme-body">
									<span class="theme-line">
										<span class="item-label">{item.theme.display_name}</span>
										{#if themeStore.activeTheme === item.theme.name}
											<span class="theme-current-badge">Current</span>
										{/if}
									</span>
									<span class="theme-description">
										{item.theme.author} · {item.theme.tags.slice(0, 2).join(' · ')}
									</span>
								</span>
							</span>
						{:else}
							<span class="file-row create-row">
								<span class="file-icon">✨</span>
								<span class="file-body">
									<span class="file-title">{`Create '${item.title}'`}</span>
								</span>
							</span>
						{/if}
					</button>
				{/each}
			{/if}
		</div>

		<div class="palette-footer">
			<span class="hint"><kbd>↑↓</kbd> navigate</span>
			<span class="hint"><kbd>Enter</kbd> select</span>
			<span class="hint"><kbd>Esc</kbd> {activeMode === 'themes' ? 'back' : 'close'}</span>
			{#if activeMode !== 'themes'}
				<span class="hint"><kbd>&gt;</kbd> commands</span>
			{:else}
				<span class="hint current-theme-hint">
					Current:
					<strong>{currentThemeEntry()?.display_name ?? themeStore.activeTheme}</strong>
				</span>
			{/if}
		</div>
	</div>
</div>

<style>
	.palette-backdrop {
		position: fixed;
		inset: 0;
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding: min(18vh, 140px) 16px 16px;
		background: color-mix(in srgb, var(--bg-default) 74%, transparent);
		z-index: 50;
	}

	.palette {
		width: min(640px, 100%);
		max-height: min(62vh, 720px);
		display: flex;
		flex-direction: column;
		background: var(--bg-panel);
		border: 1px solid var(--border-default);
		border-radius: 16px;
		box-shadow: 0 24px 60px color-mix(in srgb, var(--bg-default) 86%, transparent);
		overflow: hidden;
	}

	.palette-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 16px;
		border-bottom: 1px solid var(--border-default);
		background: var(--bg-panel);
	}

	.mode-pill {
		flex: 0 0 auto;
		padding: 3px 10px;
		border-radius: 999px;
		background: var(--bg-secondary);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.palette-input {
		width: 100%;
		padding: 18px 4px 18px 0;
		border: none;
		outline: none;
		background: var(--bg-panel);
		color: var(--text-default);
		font-size: 17px;
	}

	.palette-results {
		overflow-y: auto;
		padding: 8px;
	}

	.palette-item {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 12px 14px;
		border: none;
		border-radius: 10px;
		background: transparent;
		color: var(--text-default);
		cursor: pointer;
		text-align: left;
	}

	.palette-item:hover,
	.palette-item.selected {
		background: var(--bg-hover);
	}

	.theme-item {
		align-items: stretch;
	}

	.file-row,
	.theme-row {
		display: flex;
		align-items: center;
		gap: 14px;
		min-width: 0;
		flex: 1;
	}

	.file-icon {
		width: 20px;
		flex: 0 0 20px;
		text-align: center;
		line-height: 1;
		color: var(--text-muted);
	}

	.file-body,
	.theme-body {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.file-title,
	.item-label {
		min-width: 0;
		font-size: 14px;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-default);
	}

	.file-path,
	.theme-description,
	.cmd-category {
		font-size: 12px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.create-row .file-title {
		font-style: italic;
		color: var(--text-secondary);
	}

	.item-meta,
	.theme-line {
		display: flex;
		align-items: center;
		gap: 10px;
		white-space: nowrap;
	}

	.item-meta {
		margin-left: auto;
	}

	.theme-line {
		flex-wrap: wrap;
		gap: 8px;
	}

	.theme-swatch-strip {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: 4px;
		flex: 0 0 88px;
		padding: 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 10px;
		background: var(--bg-default);
	}

	.theme-swatch {
		display: block;
		height: 34px;
		border-radius: 6px;
	}

	.theme-current-badge,
	.item-shortcut,
	.hint kbd {
		padding: 3px 8px;
		border-radius: 999px;
		border: 1px solid var(--border-default);
		background: var(--bg-secondary);
		font-size: 12px;
		color: var(--text-default);
	}

	.theme-current-badge {
		border-color: var(--accent);
		background: var(--accent-bg);
		color: var(--accent-text);
	}


	.no-results {
		padding: 24px;
		text-align: center;
		color: var(--text-muted);
	}

	.palette-footer {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		padding: 12px 16px;
		border-top: 1px solid var(--border-default);
		background: var(--bg-surface);
	}

	.hint {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}

	.current-theme-hint strong {
		color: var(--text-default);
	}
</style>
