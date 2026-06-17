/**
 * Pure view helpers for the status-bar connection **badge** (ADR 0017 C.2).
 *
 * Unlike the former global switcher (which described the whole server list and
 * could retarget the app), the badge describes a *single* window's own
 * connection — its {@link ConnectionIdentity} from `window_connection_info` —
 * plus an optional reachability probe for the live/offline dot. Kept separate
 * from the Svelte component so the icon/label/dot/title logic is unit-testable.
 */

import {
	LOCAL_ID,
	type ConnectionIdentity,
	type ConnectionList,
	type ConnectionTestResult
} from './connection-client.ts';

/** The live-status dot rendered next to the badge label. */
export type BadgeDot = 'none' | 'checking' | 'live' | 'offline';

/** Everything the badge component needs to render, derived purely. */
export interface ConnectionBadge {
	/** 💻 for local, ☁ for a remote server. */
	icon: string;
	/** Display label: the server name, with a `· <n> ms` suffix when probed live. */
	label: string;
	/** Live-status dot. Local connections show no dot (always available). */
	dot: BadgeDot;
	/** Whether this is a remote connection. */
	remote: boolean;
	/** Tooltip text (full name + reachability detail). */
	title: string;
}

/**
 * Build the badge model for a window's connection.
 *
 * - **Local**: laptop icon, no status dot, plain label.
 * - **Remote**: cloud icon plus a live/offline/checking dot. While `checking`
 *   (no probe result yet) the dot is neutral; once a probe resolves it reflects
 *   reachability, and a successful probe appends the latency to the label.
 */
export function connectionBadge(
	identity: ConnectionIdentity,
	status: ConnectionTestResult | null,
	checking: boolean
): ConnectionBadge {
	if (!identity.remote) {
		return {
			icon: '💻',
			label: identity.name,
			dot: 'none',
			remote: false,
			title: `${identity.name} — local daemon`
		};
	}

	let dot: BadgeDot = 'checking';
	if (!checking && status) {
		dot = status.reachable ? 'live' : 'offline';
	}

	const label =
		status?.reachable && typeof status.latency_ms === 'number'
			? `${identity.name} · ${status.latency_ms} ms`
			: identity.name;

	const title =
		status && !status.reachable
			? `${identity.name} — ${status.error ?? 'Unreachable'}`
			: identity.name;

	return { icon: '☁', label, dot, remote: true, title };
}

/** A connection the current vault can be opened on (for the "another server" menu). */
export interface ServerTarget {
	id: string;
	name: string;
	kind: 'local' | 'remote';
}

/**
 * The connections *other than* the current window's, for the "Open this vault on
 * another server…" submenu. Always offers the local daemon (unless that's the
 * current connection) followed by every saved remote server (minus the current).
 */
export function otherServerTargets(
	list: ConnectionList,
	currentServerId: string
): ServerTarget[] {
	const targets: ServerTarget[] = [];
	if (currentServerId !== LOCAL_ID) {
		targets.push({ id: LOCAL_ID, name: 'This Mac', kind: 'local' });
	}
	for (const server of list.servers) {
		if (server.id === currentServerId) continue;
		targets.push({ id: server.id, name: server.name, kind: 'remote' });
	}
	return targets;
}

/**
 * The window-title suffix for a connection: the server name for a remote window,
 * or `null` for local (no suffix). Used to badge remote window titles.
 */
export function titleServerSuffix(identity: ConnectionIdentity): string | null {
	return identity.remote ? identity.name : null;
}
