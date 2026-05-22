<script lang="ts">
	import type { NoteSummary } from '$lib/api';
	import type { Command } from '$lib/commands';
	import { fuzzyFilter } from '$lib/fuzzy';
	import { noteIcon } from '$lib/note-icons';
	import { getRecentlyViewed } from '$lib/recently-viewed';
	import { vaultStore } from '$lib/stores.svelte';

	type Mode = 'files' | 'commands';

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

	type PaletteItem = FileItem | CreateItem | CommandItem;

	const RECENT_COMMAND_LIMIT = 10;

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
				? parsed.filter((value): value is string => typeof value === 'string').slice(0, RECENT_COMMAND_LIMIT)
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
			type: 'note',
			archived: false,
			frontmatter: null
		};
	}

	function normalize(value: string): string {
		return value.trim().toLowerCase();
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

		recordRecentCommand(item.command.id);
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
				onClose();
				break;
		}
	}

	let activeMode = $derived<Mode>(rawInput.startsWith('> ') ? 'commands' : 'files');
	let query = $derived(activeMode === 'commands' ? rawInput.slice(2) : rawInput);
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

		const recentBoost = new Map(recentCommandIds.map((id, index) => [id, RECENT_COMMAND_LIMIT - index]));
		return fuzzyFilter(currentQuery, commands, (command) => command.label)
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

	let results = $derived.by((): PaletteItem[] =>
		activeMode === 'commands' ? [...commandResults] : [...fileResults]
	);

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
		rawInput;
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
</script>

<div
	class="palette-backdrop"
	onclick={(event) => event.target === event.currentTarget && onClose()}
	onkeydown={handleKeydown}
	role="dialog"
	aria-modal="true"
	tabindex="-1"
>
	<div class="palette">
		<div class="palette-header">
			<span class="mode-pill">{activeMode === 'files' ? 'Files' : 'Commands'}</span>
			<input
				bind:this={inputRef}
				bind:value={rawInput}
				class="palette-input"
				placeholder={activeMode === 'files' ? 'Open a note...' : 'Type a command...'}
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
			<span class="hint"><kbd>Esc</kbd> close</span>
			<span class="hint"><kbd>&gt;</kbd> commands</span>
		</div>
	</div>
</div>

<style>
	.palette-backdrop {
		position: fixed;
		inset: 0;
		background: var(--ns-overlay);
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding: min(18vh, 140px) 16px 16px;
		z-index: 50;
	}

	.palette {
		width: min(600px, 100%);
		max-height: min(60vh, 720px);
		display: flex;
		flex-direction: column;
		background: var(--ns-panel-bg-strong);
		border: 1px solid var(--ns-border-overlay);
		border-radius: 16px;
		box-shadow: var(--ns-shadow);
		overflow: hidden;
	}

	.palette-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 16px;
		border-bottom: 1px solid var(--ns-border-overlay);
		background: var(--ns-panel-bg-strong);
	}

	.mode-pill {
		flex: 0 0 auto;
		font-size: 11px;
		padding: 2px 8px;
		border-radius: 4px;
		background: var(--ns-surface-hover);
		color: var(--ns-text-muted);
	}

	.palette-input {
		width: 100%;
		padding: 18px 4px 18px 0;
		border: none;
		outline: none;
		background: var(--ns-panel-bg-strong);
		color: var(--ns-text);
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
		color: var(--ns-text);
		cursor: pointer;
		text-align: left;
	}

	.palette-item:hover,
	.palette-item.selected {
		background: var(--ns-surface-hover-strong);
	}

	.file-row {
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
		color: var(--ns-text-muted-strong);
	}

	.file-body {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.file-title,
	.item-label {
		min-width: 0;
		font-size: 14px;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.file-path {
		font-size: 12px;
		color: var(--ns-text-muted-strong);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.create-row .file-title {
		font-style: italic;
		color: var(--ns-text-secondary);
	}

	.item-meta {
		display: flex;
		align-items: center;
		gap: 10px;
		white-space: nowrap;
		margin-left: auto;
	}

	.cmd-category {
		font-size: 12px;
		color: var(--ns-text-muted-strong);
		text-align: right;
	}

	.item-shortcut,
	.hint kbd {
		padding: 3px 8px;
		border-radius: 999px;
		background: var(--ns-kbd-bg);
		border: 1px solid var(--ns-kbd-border);
		font-size: 12px;
		color: var(--ns-text);
	}

	.no-results {
		padding: 24px;
		text-align: center;
		color: var(--ns-text-muted-soft);
	}

	.palette-footer {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		padding: 12px 16px;
		border-top: 1px solid var(--ns-border-overlay);
		background: var(--ns-surface-translucent-subtle);
	}

	.hint {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ns-text-muted-strong);
	}
</style>
