import { describe, expect, it } from 'vitest';

import {
	dockSegmentKey,
	dockTabs,
	dockTitle,
	loadDockSegment,
	loadRailTab,
	normalizeDockSegment,
	normalizeRailTab,
	railTabKey,
	saveDockSegment,
	saveRailTab
} from './right-dock.ts';

function memoryStorage(seed: Record<string, string> = {}) {
	const map = new Map<string, string>(Object.entries(seed));
	return {
		getItem: (key: string) => (map.has(key) ? (map.get(key) as string) : null),
		setItem: (key: string, value: string) => {
			map.set(key, value);
		},
		dump: () => Object.fromEntries(map)
	};
}

describe('normalizeDockSegment', () => {
	it('accepts the known segments', () => {
		expect(normalizeDockSegment('context')).toBe('context');
		expect(normalizeDockSegment('chat')).toBe('chat');
	});

	it('rejects unknown values', () => {
		expect(normalizeDockSegment('toc')).toBeNull();
		expect(normalizeDockSegment('')).toBeNull();
		expect(normalizeDockSegment(null)).toBeNull();
		expect(normalizeDockSegment(undefined)).toBeNull();
		expect(normalizeDockSegment(42)).toBeNull();
	});
});

describe('dockSegmentKey', () => {
	it('namespaces the key per vault', () => {
		expect(dockSegmentKey('Sods')).toBe('notesmith:dock-segment:Sods');
	});

	it('returns null without a vault', () => {
		expect(dockSegmentKey('')).toBeNull();
		expect(dockSegmentKey(null)).toBeNull();
		expect(dockSegmentKey(undefined)).toBeNull();
	});
});

describe('loadDockSegment', () => {
	it('defaults to context when nothing is stored', () => {
		expect(loadDockSegment('Sods', memoryStorage())).toBe('context');
	});

	it('returns the stored segment for the vault', () => {
		const storage = memoryStorage({ 'notesmith:dock-segment:Sods': 'chat' });
		expect(loadDockSegment('Sods', storage)).toBe('chat');
	});

	it('ignores a corrupt stored value', () => {
		const storage = memoryStorage({ 'notesmith:dock-segment:Sods': 'bogus' });
		expect(loadDockSegment('Sods', storage)).toBe('context');
	});

	it('defaults to context without a vault or storage', () => {
		expect(loadDockSegment(null, memoryStorage())).toBe('context');
		expect(loadDockSegment('Sods', null)).toBe('context');
	});

	it('keeps segments isolated between vaults', () => {
		const storage = memoryStorage({ 'notesmith:dock-segment:Sods': 'chat' });
		expect(loadDockSegment('Other', storage)).toBe('context');
	});
});

describe('saveDockSegment', () => {
	it('persists the segment under the vault key', () => {
		const storage = memoryStorage();
		saveDockSegment('Sods', 'chat', storage);
		expect(storage.dump()).toEqual({ 'notesmith:dock-segment:Sods': 'chat' });
	});

	it('round-trips through load', () => {
		const storage = memoryStorage();
		saveDockSegment('Sods', 'chat', storage);
		expect(loadDockSegment('Sods', storage)).toBe('chat');
	});

	it('no-ops without a vault or storage', () => {
		const storage = memoryStorage();
		saveDockSegment(null, 'chat', storage);
		expect(storage.dump()).toEqual({});
		expect(() => saveDockSegment('Sods', 'chat', null)).not.toThrow();
	});
});

describe('normalizeRailTab', () => {
	it('accepts the known rail tabs', () => {
		expect(normalizeRailTab('metadata')).toBe('metadata');
		expect(normalizeRailTab('links')).toBe('links');
		expect(normalizeRailTab('toc')).toBe('toc');
	});

	it('rejects unknown values', () => {
		expect(normalizeRailTab('chat')).toBeNull();
		expect(normalizeRailTab('')).toBeNull();
		expect(normalizeRailTab(null)).toBeNull();
		expect(normalizeRailTab(undefined)).toBeNull();
	});
});

describe('railTabKey', () => {
	it('namespaces the key per vault', () => {
		expect(railTabKey('Sods')).toBe('notesmith:rail-tab:Sods');
	});

	it('returns null without a vault', () => {
		expect(railTabKey('')).toBeNull();
		expect(railTabKey(null)).toBeNull();
	});
});

describe('loadRailTab / saveRailTab', () => {
	it('defaults to metadata when nothing is stored', () => {
		expect(loadRailTab('Sods', memoryStorage())).toBe('metadata');
	});

	it('round-trips through save', () => {
		const storage = memoryStorage();
		saveRailTab('Sods', 'links', storage);
		expect(storage.dump()).toEqual({ 'notesmith:rail-tab:Sods': 'links' });
		expect(loadRailTab('Sods', storage)).toBe('links');
	});

	it('ignores a corrupt stored value', () => {
		const storage = memoryStorage({ 'notesmith:rail-tab:Sods': 'bogus' });
		expect(loadRailTab('Sods', storage)).toBe('metadata');
	});

	it('keeps tabs isolated between vaults', () => {
		const storage = memoryStorage({ 'notesmith:rail-tab:Sods': 'toc' });
		expect(loadRailTab('Other', storage)).toBe('metadata');
	});

	it('no-ops without a vault or storage', () => {
		const storage = memoryStorage();
		saveRailTab(null, 'toc', storage);
		expect(storage.dump()).toEqual({});
		expect(() => saveRailTab('Sods', 'toc', null)).not.toThrow();
	});
});

describe('dockTabs', () => {
	it('marks the active context sub-tab while the Context segment shows', () => {
		const tabs = dockTabs('context', 'links');
		expect(tabs.map((t) => t.id)).toEqual(['metadata', 'links', 'toc', 'chat']);
		expect(tabs.find((t) => t.id === 'links')?.active).toBe(true);
		expect(tabs.find((t) => t.id === 'metadata')?.active).toBe(false);
		expect(tabs.find((t) => t.id === 'chat')?.active).toBe(false);
	});

	it('marks Chat active and no context tab when the Chat segment shows', () => {
		const tabs = dockTabs('chat', 'links');
		expect(tabs.find((t) => t.id === 'chat')?.active).toBe(true);
		expect(tabs.filter((t) => t.kind === 'context').every((t) => !t.active)).toBe(true);
	});

	it('tags the Chat tab with the chat kind', () => {
		expect(dockTabs('context', 'metadata').find((t) => t.id === 'chat')?.kind).toBe('chat');
	});
});

describe('dockTitle', () => {
	it('returns the basename with extension', () => {
		expect(dockTitle('Projects/notesmith.md')).toBe('notesmith.md');
		expect(dockTitle('flat.md')).toBe('flat.md');
	});

	it('returns an empty string when no note is selected', () => {
		expect(dockTitle(null)).toBe('');
		expect(dockTitle(undefined)).toBe('');
		expect(dockTitle('')).toBe('');
	});
});
