import { describe, expect, it } from 'vitest';

import type { ConnectionList } from './connection-client.ts';
import {
	activeOption,
	connectionOptions,
	LOCAL_LABEL,
	pillIcon,
	pillLabel
} from './connection-view.ts';

const localActive: ConnectionList = {
	active_id: 'local',
	servers: [
		{ id: 'home', name: 'Home', url: 'https://home.example.com', has_token: true },
		{ id: 'work', name: 'Work', url: 'http://10.0.0.5:27183', has_token: false }
	]
};

const remoteActive: ConnectionList = { ...localActive, active_id: 'home' };

describe('connectionOptions', () => {
	it('lists This Mac first, then every server', () => {
		const options = connectionOptions(localActive);
		expect(options.map((o) => o.name)).toEqual([LOCAL_LABEL, 'Home', 'Work']);
		expect(options[0].kind).toBe('local');
		expect(options[1].kind).toBe('remote');
	});

	it('marks the active option', () => {
		expect(connectionOptions(localActive).find((o) => o.active)?.id).toBe('local');
		expect(connectionOptions(remoteActive).find((o) => o.active)?.id).toBe('home');
	});

	it('carries the has_token flag through', () => {
		const home = connectionOptions(localActive).find((o) => o.id === 'home');
		expect(home?.hasToken).toBe(true);
	});
});

describe('activeOption', () => {
	it('returns local when active', () => {
		expect(activeOption(localActive).kind).toBe('local');
	});

	it('returns the active server when remote', () => {
		expect(activeOption(remoteActive).name).toBe('Home');
	});

	it('falls back to local for an unknown active id', () => {
		expect(activeOption({ active_id: 'ghost', servers: [] }).kind).toBe('local');
	});
});

describe('pillIcon', () => {
	it('uses a laptop for local and a cloud for remote', () => {
		expect(pillIcon(localActive)).toBe('💻');
		expect(pillIcon(remoteActive)).toBe('☁');
	});
});

describe('pillLabel', () => {
	it('shows This Mac for local regardless of status', () => {
		expect(pillLabel(localActive, { reachable: true, latency_ms: 5 })).toBe(LOCAL_LABEL);
	});

	it('appends latency for a reachable remote', () => {
		expect(pillLabel(remoteActive, { reachable: true, latency_ms: 42 })).toBe('Home · 42 ms');
	});

	it('shows just the name when the remote is unreachable or untested', () => {
		expect(pillLabel(remoteActive, null)).toBe('Home');
		expect(pillLabel(remoteActive, { reachable: false, error: 'x' })).toBe('Home');
	});
});
