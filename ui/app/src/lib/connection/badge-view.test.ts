import { describe, expect, it } from 'vitest';
import type { ConnectionIdentity, ConnectionTestResult } from './connection-client.ts';
import { connectionBadge, connectionDetail, titleServerSuffix } from './badge-view.ts';

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

describe('connectionDetail', () => {
	it('describes the local daemon as always available with no dot', () => {
		const detail = connectionDetail(local, null, false);
		expect(detail).toEqual({
			name: 'This Mac',
			kind: 'local',
			kindLabel: 'Local daemon',
			statusLabel: 'Always available',
			dot: 'none',
			url: null
		});
	});

	it('shows a checking status for a remote with no probe yet', () => {
		const detail = connectionDetail(remote, null, true, 'https://m.example');
		expect(detail.kind).toBe('remote');
		expect(detail.kindLabel).toBe('Remote server');
		expect(detail.statusLabel).toBe('Checking…');
		expect(detail.dot).toBe('checking');
		expect(detail.url).toBe('https://m.example');
	});

	it('reports live status with latency when reachable', () => {
		const detail = connectionDetail(remote, reachable, false, 'https://m.example');
		expect(detail.statusLabel).toBe('Live · 42 ms');
		expect(detail.dot).toBe('live');
	});

	it('reports offline status with the error when unreachable', () => {
		const detail = connectionDetail(remote, unreachable, false, 'https://m.example');
		expect(detail.statusLabel).toBe('Offline — No response');
		expect(detail.dot).toBe('offline');
	});
});

describe('titleServerSuffix', () => {
	it('is the server name for remote and null for local', () => {
		expect(titleServerSuffix(remote)).toBe('memory');
		expect(titleServerSuffix(local)).toBeNull();
	});
});
