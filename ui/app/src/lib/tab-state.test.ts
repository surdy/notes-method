import { describe, expect, it } from 'vitest';
import type { NoteSummary } from './api';
import { openTab, setViewMode, switchToTab, toggleViewMode, closeOtherTabs, type TabState } from './tab-state';

function note(path: string): NoteSummary {
	return { path, title: path, tags: [] } as NoteSummary;
}

function emptyState(): TabState {
	return { tabs: [], activeTabIndex: -1, selectedPath: null, recentlyClosed: [] };
}

function stateWith(...paths: string[]): TabState {
	const notes = paths.map(note);
	return paths.reduce((state, path) => openTab(state, path, notes), emptyState());
}

describe('setViewMode', () => {
	it('sets the active tab view mode', () => {
		const state = switchToTab(stateWith('a.md', 'b.md'), 1);
		const next = setViewMode(state, 'reading');
		expect(next.tabs[1].viewMode).toBe('reading');
	});

	it('leaves other tabs unchanged', () => {
		const state = switchToTab(stateWith('a.md', 'b.md'), 0);
		const next = setViewMode(state, 'live-preview');
		expect(next.tabs[0].viewMode).toBe('live-preview');
		expect(next.tabs[1].viewMode).toBe('source');
	});

	it('is a no-op when there is no active tab', () => {
		const state = emptyState();
		expect(setViewMode(state, 'reading')).toBe(state);
	});
});

describe('toggleViewMode', () => {
	it('cycles source -> live-preview -> reading -> source', () => {
		let state = stateWith('a.md');
		expect(state.tabs[0].viewMode).toBe('source');
		state = toggleViewMode(state);
		expect(state.tabs[0].viewMode).toBe('live-preview');
		state = toggleViewMode(state);
		expect(state.tabs[0].viewMode).toBe('reading');
		state = toggleViewMode(state);
		expect(state.tabs[0].viewMode).toBe('source');
	});

	it('is a no-op when there is no active tab', () => {
		const state = emptyState();
		expect(toggleViewMode(state)).toBe(state);
	});
});

describe('closeOtherTabs', () => {
	it('keeps only the kept tab and makes it active', () => {
		const state = stateWith('a.md', 'b.md', 'c.md');
		const next = closeOtherTabs(state, 1);
		expect(next.tabs.map((tab) => tab.path)).toEqual(['b.md']);
		expect(next.activeTabIndex).toBe(0);
		expect(next.selectedPath).toBe('b.md');
	});

	it('records the closed tabs in recentlyClosed', () => {
		const state = stateWith('a.md', 'b.md', 'c.md');
		const next = closeOtherTabs(state, 1);
		expect(next.recentlyClosed).toEqual(['a.md', 'c.md']);
	});

	it('keeps a single tab unchanged', () => {
		const state = stateWith('a.md');
		const next = closeOtherTabs(state, 0);
		expect(next.tabs.map((tab) => tab.path)).toEqual(['a.md']);
		expect(next.activeTabIndex).toBe(0);
	});

	it('is a safe no-op when keepIndex is out of range', () => {
		const state = stateWith('a.md', 'b.md');
		expect(closeOtherTabs(state, 5)).toBe(state);
		expect(closeOtherTabs(state, -1)).toBe(state);
	});
});
