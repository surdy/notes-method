import type { NoteSummary } from './api';

export interface Tab {
	path: string;
	title: string;
	dirty: boolean;
}

export interface TabState {
	tabs: Tab[];
	activeTabIndex: number;
	selectedPath: string | null;
	recentlyClosed: string[];
}

export interface PersistedTabState {
	tabs: Array<Pick<Tab, 'path' | 'title'>>;
	activeIndex: number;
}

export function openTab(state: TabState, path: string, notes: NoteSummary[]): TabState {
	const existingIndex = state.tabs.findIndex((tab) => tab.path === path);
	if (existingIndex >= 0) {
		return {
			...state,
			activeTabIndex: existingIndex,
			selectedPath: path
		};
	}

	const title = resolveTabTitle(path, notes);
	const tabs = [...state.tabs, { path, title, dirty: false }];

	return {
		...state,
		tabs,
		activeTabIndex: tabs.length - 1,
		selectedPath: path
	};
}

export function closeTab(state: TabState, index: number): TabState {
	if (index < 0 || index >= state.tabs.length) {
		return state;
	}

	const closedTab = state.tabs[index];
	const tabs = state.tabs.filter((_, tabIndex) => tabIndex !== index);
	const recentlyClosed = [...state.recentlyClosed, closedTab.path].slice(-10);

	if (tabs.length === 0) {
		return {
			tabs,
			activeTabIndex: -1,
			selectedPath: null,
			recentlyClosed
		};
	}

	let activeTabIndex = state.activeTabIndex;
	if (index <= state.activeTabIndex) {
		activeTabIndex = Math.max(0, state.activeTabIndex - 1);
	}

	return {
		tabs,
		activeTabIndex,
		selectedPath: tabs[activeTabIndex]?.path ?? null,
		recentlyClosed
	};
}

export function reopenLastClosedTab(state: TabState, notes: NoteSummary[]): TabState {
	const path = state.recentlyClosed.at(-1);
	if (!path) {
		return state;
	}

	return openTab(
		{
			...state,
			recentlyClosed: state.recentlyClosed.slice(0, -1)
		},
		path,
		notes
	);
}

export function switchToTab(state: TabState, index: number): TabState {
	if (index < 0 || index >= state.tabs.length) {
		return state;
	}

	return {
		...state,
		activeTabIndex: index,
		selectedPath: state.tabs[index]?.path ?? null
	};
}

export function markTabDirty(state: TabState, path: string, dirty: boolean): TabState {
	return {
		...state,
		tabs: state.tabs.map((tab) => (tab.path === path ? { ...tab, dirty } : tab))
	};
}

export function moveTab(state: TabState, fromIndex: number, toIndex: number): TabState {
	if (
		fromIndex === toIndex ||
		fromIndex < 0 ||
		toIndex < 0 ||
		fromIndex >= state.tabs.length ||
		toIndex >= state.tabs.length
	) {
		return state;
	}

	const tabs = [...state.tabs];
	const [tab] = tabs.splice(fromIndex, 1);
	tabs.splice(toIndex, 0, tab);

	let activeTabIndex = state.activeTabIndex;
	if (state.activeTabIndex === fromIndex) {
		activeTabIndex = toIndex;
	} else if (fromIndex < state.activeTabIndex && toIndex >= state.activeTabIndex) {
		activeTabIndex -= 1;
	} else if (fromIndex > state.activeTabIndex && toIndex <= state.activeTabIndex) {
		activeTabIndex += 1;
	}

	return {
		...state,
		tabs,
		activeTabIndex,
		selectedPath: tabs[activeTabIndex]?.path ?? null
	};
}

export function serializeTabState(state: Pick<TabState, 'tabs' | 'activeTabIndex'>): string {
	return JSON.stringify({
		tabs: state.tabs.map(({ path, title }) => ({ path, title })),
		activeIndex: state.activeTabIndex
	} satisfies PersistedTabState);
}

export function restoreTabState(stored: string | null): Omit<TabState, 'recentlyClosed'> | null {
	if (!stored) {
		return null;
	}

	try {
		const parsed = JSON.parse(stored) as PersistedTabState;
		if (!Array.isArray(parsed.tabs)) {
			return null;
		}

		const tabs = parsed.tabs
			.filter((tab) => typeof tab?.path === 'string' && typeof tab?.title === 'string')
			.map((tab) => ({
				path: tab.path,
				title: tab.title,
				dirty: false
			}));
		const activeTabIndex =
			typeof parsed.activeIndex === 'number' &&
			parsed.activeIndex >= 0 &&
			parsed.activeIndex < tabs.length
				? parsed.activeIndex
				: -1;

		return {
			tabs,
			activeTabIndex,
			selectedPath: activeTabIndex >= 0 ? tabs[activeTabIndex]?.path ?? null : null
		};
	} catch {
		return null;
	}
}

function resolveTabTitle(path: string, notes: NoteSummary[]): string {
	const note = notes.find((candidate) => candidate.path === path);
	return note?.title || path.split('/').at(-1)?.replace(/\.md$/, '') || path;
}
