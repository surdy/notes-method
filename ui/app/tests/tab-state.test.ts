import test from 'node:test';
import assert from 'node:assert/strict';

import {
	closeTab,
	markTabDirty,
	moveTab,
	openTab,
	reopenLastClosedTab,
	restoreTabState,
	serializeTabState,
	switchToTab,
	toggleViewMode
} from '../src/lib/tab-state.ts';

test('openTab opens a note in a new active tab', () => {
	const state = openTab(
		{ tabs: [], activeTabIndex: -1, selectedPath: null, recentlyClosed: [] },
		'Inbox/Daily/2026-05-10.md',
		[
			{
				path: 'Inbox/Daily/2026-05-10.md',
				title: 'Daily Note',
				type: 'daily',
				archived: false
			}
		]
	);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/Daily/2026-05-10.md', title: 'Daily Note', dirty: false, viewMode: 'source' }
	]);
	assert.equal(state.activeTabIndex, 0);
	assert.equal(state.selectedPath, 'Inbox/Daily/2026-05-10.md');
});

test('openTab switches to an already open tab without duplicating it', () => {
	const state = openTab(
		{
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' }
			],
			activeTabIndex: 0,
			selectedPath: 'Inbox/alpha.md',
			recentlyClosed: []
		},
		'Inbox/beta.md',
		[]
	);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
		{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' }
	]);
	assert.equal(state.activeTabIndex, 1);
	assert.equal(state.selectedPath, 'Inbox/beta.md');
});

test('closeTab removes the active tab, activates the previous tab, and tracks it as recently closed', () => {
	const state = closeTab({
		tabs: [
			{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
			{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' },
			{ path: 'Inbox/gamma.md', title: 'Gamma', dirty: false, viewMode: 'source' }
		],
		activeTabIndex: 1,
		selectedPath: 'Inbox/beta.md',
		recentlyClosed: []
	}, 1);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
		{ path: 'Inbox/gamma.md', title: 'Gamma', dirty: false, viewMode: 'source' }
	]);
	assert.equal(state.activeTabIndex, 0);
	assert.equal(state.selectedPath, 'Inbox/alpha.md');
	assert.deepEqual(state.recentlyClosed, ['Inbox/beta.md']);
});

test('moveTab reorders tabs and keeps the moved tab active', () => {
	const state = moveTab(
		{
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' },
				{ path: 'Inbox/gamma.md', title: 'Gamma', dirty: false, viewMode: 'source' }
			],
			activeTabIndex: 2,
			selectedPath: 'Inbox/gamma.md',
			recentlyClosed: []
		},
		2,
		0
	);

	assert.deepEqual(state.tabs.map((tab) => tab.path), [
		'Inbox/gamma.md',
		'Inbox/alpha.md',
		'Inbox/beta.md'
	]);
	assert.equal(state.activeTabIndex, 0);
	assert.equal(state.selectedPath, 'Inbox/gamma.md');
});

test('reopenLastClosedTab restores the most recently closed note with a fallback title', () => {
	const state = reopenLastClosedTab(
		{
			tabs: [{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' }],
			activeTabIndex: 0,
			selectedPath: 'Inbox/alpha.md',
			recentlyClosed: ['Projects/roadmap.md']
		},
		[]
	);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
		{ path: 'Projects/roadmap.md', title: 'roadmap', dirty: false, viewMode: 'source' }
	]);
	assert.equal(state.activeTabIndex, 1);
	assert.equal(state.selectedPath, 'Projects/roadmap.md');
	assert.deepEqual(state.recentlyClosed, []);
});

test('restoreTabState rebuilds clean tabs and selects the persisted active tab', () => {
	const restored = restoreTabState(
		serializeTabState({
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: true, viewMode: 'reading' },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' }
			],
			activeTabIndex: 1
		})
	);

	assert.deepEqual(restored, {
		tabs: [
			{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'reading' },
			{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' }
		],
		activeTabIndex: 1,
		selectedPath: 'Inbox/beta.md'
	});
});

test('switchToTab and markTabDirty update the selected tab state', () => {
	const switched = switchToTab(
		{
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' }
			],
			activeTabIndex: 0,
			selectedPath: 'Inbox/alpha.md',
			recentlyClosed: []
		},
		1
	);
	const dirtyState = markTabDirty(switched, 'Inbox/beta.md', true);

	assert.equal(dirtyState.activeTabIndex, 1);
	assert.equal(dirtyState.selectedPath, 'Inbox/beta.md');
	assert.equal(dirtyState.tabs[1]?.dirty, true);
});

// --- viewMode tests ---

test('openTab defaults viewMode to source', () => {
	const state = openTab(
		{ tabs: [], activeTabIndex: -1, selectedPath: null, recentlyClosed: [] },
		'Notes/test.md',
		[{ path: 'Notes/test.md', title: 'Test', type: 'note', archived: false }]
	);

	assert.equal(state.tabs[0]?.viewMode, 'source');
});

test('toggleViewMode switches source to reading for active tab', () => {
	const state = toggleViewMode({
		tabs: [
			{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'source' },
			{ path: 'Inbox/beta.md', title: 'Beta', dirty: false, viewMode: 'source' }
		],
		activeTabIndex: 1,
		selectedPath: 'Inbox/beta.md',
		recentlyClosed: []
	});

	assert.equal(state.tabs[0]?.viewMode, 'source');
	assert.equal(state.tabs[1]?.viewMode, 'reading');
});

test('toggleViewMode switches reading back to source', () => {
	const state = toggleViewMode({
		tabs: [
			{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false, viewMode: 'reading' }
		],
		activeTabIndex: 0,
		selectedPath: 'Inbox/alpha.md',
		recentlyClosed: []
	});

	assert.equal(state.tabs[0]?.viewMode, 'source');
});

test('toggleViewMode is a no-op when no active tab', () => {
	const original = {
		tabs: [] as Array<{ path: string; title: string; dirty: boolean; viewMode: 'source' | 'reading' }>,
		activeTabIndex: -1,
		selectedPath: null,
		recentlyClosed: []
	};
	const state = toggleViewMode(original);

	assert.deepEqual(state, original);
});

test('serializeTabState persists viewMode and restoreTabState recovers it', () => {
	const serialized = serializeTabState({
		tabs: [
			{ path: 'Notes/a.md', title: 'A', dirty: false, viewMode: 'reading' },
			{ path: 'Notes/b.md', title: 'B', dirty: true, viewMode: 'source' }
		],
		activeTabIndex: 0
	});

	const restored = restoreTabState(serialized);
	assert.equal(restored?.tabs[0]?.viewMode, 'reading');
	assert.equal(restored?.tabs[1]?.viewMode, 'source');
});

test('restoreTabState defaults viewMode to source for legacy data without viewMode', () => {
	const legacyJson = JSON.stringify({
		tabs: [
			{ path: 'Notes/old.md', title: 'Old' }
		],
		activeIndex: 0
	});

	const restored = restoreTabState(legacyJson);
	assert.equal(restored?.tabs[0]?.viewMode, 'source');
});
