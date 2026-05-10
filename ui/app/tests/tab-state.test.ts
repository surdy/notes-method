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
	switchToTab
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
		{ path: 'Inbox/Daily/2026-05-10.md', title: 'Daily Note', dirty: false }
	]);
	assert.equal(state.activeTabIndex, 0);
	assert.equal(state.selectedPath, 'Inbox/Daily/2026-05-10.md');
});

test('openTab switches to an already open tab without duplicating it', () => {
	const state = openTab(
		{
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false }
			],
			activeTabIndex: 0,
			selectedPath: 'Inbox/alpha.md',
			recentlyClosed: []
		},
		'Inbox/beta.md',
		[]
	);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
		{ path: 'Inbox/beta.md', title: 'Beta', dirty: false }
	]);
	assert.equal(state.activeTabIndex, 1);
	assert.equal(state.selectedPath, 'Inbox/beta.md');
});

test('closeTab removes the active tab, activates the previous tab, and tracks it as recently closed', () => {
	const state = closeTab({
		tabs: [
			{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
			{ path: 'Inbox/beta.md', title: 'Beta', dirty: false },
			{ path: 'Inbox/gamma.md', title: 'Gamma', dirty: false }
		],
		activeTabIndex: 1,
		selectedPath: 'Inbox/beta.md',
		recentlyClosed: []
	}, 1);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
		{ path: 'Inbox/gamma.md', title: 'Gamma', dirty: false }
	]);
	assert.equal(state.activeTabIndex, 0);
	assert.equal(state.selectedPath, 'Inbox/alpha.md');
	assert.deepEqual(state.recentlyClosed, ['Inbox/beta.md']);
});

test('moveTab reorders tabs and keeps the moved tab active', () => {
	const state = moveTab(
		{
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false },
				{ path: 'Inbox/gamma.md', title: 'Gamma', dirty: false }
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
			tabs: [{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false }],
			activeTabIndex: 0,
			selectedPath: 'Inbox/alpha.md',
			recentlyClosed: ['Projects/roadmap.md']
		},
		[]
	);

	assert.deepEqual(state.tabs, [
		{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
		{ path: 'Projects/roadmap.md', title: 'roadmap', dirty: false }
	]);
	assert.equal(state.activeTabIndex, 1);
	assert.equal(state.selectedPath, 'Projects/roadmap.md');
	assert.deepEqual(state.recentlyClosed, []);
});

test('restoreTabState rebuilds clean tabs and selects the persisted active tab', () => {
	const restored = restoreTabState(
		serializeTabState({
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: true },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false }
			],
			activeTabIndex: 1
		})
	);

	assert.deepEqual(restored, {
		tabs: [
			{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
			{ path: 'Inbox/beta.md', title: 'Beta', dirty: false }
		],
		activeTabIndex: 1,
		selectedPath: 'Inbox/beta.md'
	});
});

test('switchToTab and markTabDirty update the selected tab state', () => {
	const switched = switchToTab(
		{
			tabs: [
				{ path: 'Inbox/alpha.md', title: 'Alpha', dirty: false },
				{ path: 'Inbox/beta.md', title: 'Beta', dirty: false }
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
