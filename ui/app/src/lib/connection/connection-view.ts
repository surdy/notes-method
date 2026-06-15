/**
 * Pure view helpers for the status-bar connection switcher. Kept separate from
 * the Svelte component so the dropdown/pill logic is unit-testable.
 */

import { LOCAL_ID, type ConnectionList, type ConnectionTestResult } from './connection-client.ts';

/** Display label for the implicit local daemon. */
export const LOCAL_LABEL = 'This Mac';

/** A single entry in the switcher dropdown (local + each saved server). */
export interface ConnectionOption {
	id: string;
	name: string;
	url: string | null;
	active: boolean;
	kind: 'local' | 'remote';
	hasToken: boolean;
}

/** The dropdown options: `This Mac` first, then every saved server. */
export function connectionOptions(list: ConnectionList): ConnectionOption[] {
	const options: ConnectionOption[] = [
		{
			id: LOCAL_ID,
			name: LOCAL_LABEL,
			url: null,
			active: list.active_id === LOCAL_ID,
			kind: 'local',
			hasToken: false
		}
	];
	for (const server of list.servers) {
		options.push({
			id: server.id,
			name: server.name,
			url: server.url,
			active: list.active_id === server.id,
			kind: 'remote',
			hasToken: server.has_token
		});
	}
	return options;
}

/** The currently-active option, defaulting to local when the id is unknown. */
export function activeOption(list: ConnectionList): ConnectionOption {
	return (
		connectionOptions(list).find((option) => option.active) ?? {
			id: LOCAL_ID,
			name: LOCAL_LABEL,
			url: null,
			active: true,
			kind: 'local',
			hasToken: false
		}
	);
}

/** The pill icon: a laptop for local, a cloud for a remote server. */
export function pillIcon(list: ConnectionList): string {
	return activeOption(list).kind === 'local' ? '💻' : '☁';
}

/**
 * The pill label: `This Mac` for local, the server name for remote — with a
 * `· <latency> ms` suffix when a fresh reachability probe succeeded.
 */
export function pillLabel(list: ConnectionList, status: ConnectionTestResult | null): string {
	const active = activeOption(list);
	if (active.kind === 'local') {
		return LOCAL_LABEL;
	}
	if (status?.reachable && typeof status.latency_ms === 'number') {
		return `${active.name} · ${status.latency_ms} ms`;
	}
	return active.name;
}
