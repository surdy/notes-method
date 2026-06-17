import { describe, expect, it } from 'vitest';
import type {
	ConnectionIdentity,
	ConnectionList,
	ConnectionTestResult
} from './connection-client.ts';
import {
	connectionBadge,
	otherServerTargets,
	titleServerSuffix
} from './badge-view.ts';

const local: ConnectionIdentity = { id: 'local', name: 'This Mac', remote: false };
const remote: ConnectionIdentity = { id: 'memory', name: 'memory', remote: true };

const reachable: ConnectionTestResult = { reachable: true, latency_ms: 42 };
const unreachable: ConnectionTestResult = { reachable: false, error: 'No response' };

describe('connectionBadge', () => {
	it('renders a dot-less laptop badge for local', () => {
		const badge = connectionBadge(local, null, false);
		expect(badge).toMatchObject({ icon: '💻', label: 'This Mac', dot: 'none', remote: false });
	});

	it('shows a neutral checking dot for a remote with no probe yet', () => {
		const badge = connectionBadge(remote, null, true);
		expect(badge.icon).toBe('☁');
		expect(badge.dot).toBe('checking');
		expect(badge.label).toBe('memory');
	});

	it('shows a live dot and latency suffix when the remote is reachable', () => {
		const badge = connectionBadge(remote, reachable, false);
		expect(badge.dot).toBe('live');
		expect(badge.label).toBe('memory · 42 ms');
	});

	it('shows an offline dot and error tooltip when the remote is unreachable', () => {
		const badge = connectionBadge(remote, unreachable, false);
		expect(badge.dot).toBe('offline');
		expect(badge.label).toBe('memory');
		expect(badge.title).toBe('memory — No response');
	});

	it('stays in checking while a probe is in flight even if a stale result exists', () => {
		const badge = connectionBadge(remote, reachable, true);
		expect(badge.dot).toBe('checking');
	});
});

describe('otherServerTargets', () => {
	const list: ConnectionList = {
		active_id: 'local',
		servers: [
			{ id: 'memory', name: 'memory', url: 'https://m.example', has_token: true },
			{ id: 'work', name: 'work', url: 'https://w.example', has_token: false }
		]
	};

	it('offers local + the other remotes when the current window is remote', () => {
		const targets = otherServerTargets(list, 'memory');
		expect(targets).toEqual([
			{ id: 'local', name: 'This Mac', kind: 'local' },
			{ id: 'work', name: 'work', kind: 'remote' }
		]);
	});

	it('omits local (no duplicate) when the current window is already local', () => {
		const targets = otherServerTargets(list, 'local');
		expect(targets.map((t) => t.id)).toEqual(['memory', 'work']);
	});
});

describe('titleServerSuffix', () => {
	it('is the server name for remote and null for local', () => {
		expect(titleServerSuffix(remote)).toBe('memory');
		expect(titleServerSuffix(local)).toBeNull();
	});
});
