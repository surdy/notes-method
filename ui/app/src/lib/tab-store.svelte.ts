import type { NoteSummary } from './api';
import { recordView } from './recently-viewed';
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
import { vaultStore } from './stores.svelte';

const STORAGE_KEY = 'notesmith:tabs';

export type { Tab, ViewMode } from './tab-state';

class TabStore {
tabs = $state<Tab[]>([]);
activeTabIndex = $state(-1);
selectedPath = $state<string | null>(null);

private _recentlyClosed: string[] = [];

get activeTab(): Tab | null {
return this.tabs[this.activeTabIndex] ?? null;
}

get activeViewMode(): ViewMode {
return this.activeTab?.viewMode ?? 'source';
}

selectNote(path: string) {
this._applyTabState(openTab(this._tabState(), path, vaultStore.notes));
this._persistTabs();
const note = vaultStore.notes.find((candidate) => candidate.path === path);
if (note && vaultStore.currentVault) {
recordView(
vaultStore.currentVault,
path,
note.title || path.split('/').pop()?.replace(/\.md$/, '') || path
);
}
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
this._applyTabState(reopenLastClosedTab(this._tabState(), vaultStore.notes));
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
const restored = restoreTabState(localStorage.getItem(STORAGE_KEY));
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
STORAGE_KEY,
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

export const tabStore = new TabStore();
