import { describe, expect, it } from 'vitest';

import {
	dockSegmentKey,
	loadDockSegment,
	normalizeDockSegment,
	saveDockSegment
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
