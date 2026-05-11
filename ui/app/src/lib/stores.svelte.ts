import { listNotes, type NoteSummary } from './api';
import {
	closeTab as closeTabState,
	markTabDirty as markTabDirtyState,
	moveTab as moveTabState,
	openTab,
	reopenLastClosedTab,
	restoreTabState,
	serializeTabState,
	switchToTab as switchToTabState,
	toggleViewMode as toggleViewModeState,
	type Tab,
	type TabState,
	type ViewMode
} from './tab-state';

export interface FolderNode {
	name: string;
	path: string;
	children: FolderNode[];
	notes: NoteSummary[];
}

export type { Tab, ViewMode } from './tab-state';

class VaultStore {
	currentVault = $state('');
	notes = $state<NoteSummary[]>([]);
	selectedPath = $state<string | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);
	tabs = $state<Tab[]>([]);
	activeTabIndex = $state(-1);

	private _recentlyClosed: string[] = [];

	get tree(): FolderNode {
		return buildTree(this.notes);
	}

	get activeTab(): Tab | null {
		return this.tabs[this.activeTabIndex] ?? null;
	}

	get activeViewMode(): ViewMode {
		return this.activeTab?.viewMode ?? 'source';
	}

	async loadNotes() {
		if (!this.currentVault) return;

		this.loading = true;
		this.error = null;
		try {
			this.notes = await listNotes(this.currentVault);
		} catch (error) {
			this.error = error instanceof Error ? error.message : 'Failed to load notes';
		} finally {
			this.loading = false;
		}
	}

	selectNote(path: string) {
		this._applyTabState(openTab(this._tabState(), path, this.notes));
		this._persistTabs();
	}

	closeTab(index: number) {
		this._applyTabState(closeTabState(this._tabState(), index));
		this._persistTabs();
	}

	closeActiveTab() {
		if (this.activeTabIndex >= 0) {
			this.closeTab(this.activeTabIndex);
		}
	}

	reopenLastTab() {
		this._applyTabState(reopenLastClosedTab(this._tabState(), this.notes));
		this._persistTabs();
	}

	switchToTab(index: number) {
		this._applyTabState(switchToTabState(this._tabState(), index));
		this._persistTabs();
	}

	markDirty(path: string, dirty: boolean) {
		this._applyTabState(markTabDirtyState(this._tabState(), path, dirty));
	}

	moveTab(fromIndex: number, toIndex: number) {
		this._applyTabState(moveTabState(this._tabState(), fromIndex, toIndex));
		this._persistTabs();
	}

	toggleViewMode() {
		this._applyTabState(toggleViewModeState(this._tabState()));
		this._persistTabs();
	}

	restoreTabs() {
		try {
			const restored = restoreTabState(localStorage.getItem('notesmith:tabs'));
			if (!restored) {
				return;
			}

			this.tabs = restored.tabs;
			this.activeTabIndex = restored.activeTabIndex;
			this.selectedPath = restored.selectedPath;
			this._recentlyClosed = [];
		} catch {
			// Ignore unavailable or invalid browser storage.
		}
	}

	private _tabState(): TabState {
		return {
			tabs: this.tabs,
			activeTabIndex: this.activeTabIndex,
			selectedPath: this.selectedPath,
			recentlyClosed: this._recentlyClosed
		};
	}

	private _applyTabState(state: TabState) {
		this.tabs = state.tabs;
		this.activeTabIndex = state.activeTabIndex;
		this.selectedPath = state.selectedPath;
		this._recentlyClosed = state.recentlyClosed;
	}

	private _persistTabs() {
		try {
			localStorage.setItem(
				'notesmith:tabs',
				serializeTabState({
					tabs: this.tabs,
					activeTabIndex: this.activeTabIndex
				})
			);
		} catch {
			// Ignore unavailable browser storage.
		}
	}
}

function buildTree(notes: NoteSummary[]): FolderNode {
	const root: FolderNode = { name: '', path: '', children: [], notes: [] };

	for (const note of notes) {
		const parts = note.path.split('/');
		let current = root;

		for (let index = 0; index < parts.length - 1; index += 1) {
			const folderName = parts[index];
			const folderPath = parts.slice(0, index + 1).join('/');
			let child = current.children.find((candidate) => candidate.name === folderName);
			if (!child) {
				child = { name: folderName, path: folderPath, children: [], notes: [] };
				current.children.push(child);
			}
			current = child;
		}

		current.notes.push(note);
	}

	sortTree(root);
	return root;
}

function sortTree(node: FolderNode) {
	node.children.sort((left, right) => left.name.localeCompare(right.name));
	node.notes.sort((left, right) => left.path.localeCompare(right.path));
	node.children.forEach(sortTree);
}

export const vaultStore = new VaultStore();
