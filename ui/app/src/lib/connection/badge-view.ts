/**
 * Pure view helpers for the status-bar connection **badge** (ADR 0017 C.2).
 *
 * Unlike the former global switcher (which described the whole server list and
 * could retarget the app), the badge describes a *single* window's own
 * connection — its {@link ConnectionIdentity} from `window_connection_info` —
 * plus an optional reachability probe for the live/offline dot. Kept separate
 * from the Svelte component so the icon/label/dot/title logic is unit-testable.
 */

import { type ConnectionIdentity, type ConnectionTestResult } from './connection-client.ts';

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

/**
 * The connection-detail view-model for the badge's status popover (ADR 0017
 * C.2, indicator-only). Describes *this window's own* connection — never an
 * action to retarget it. Switching servers is the job of the File → New Window
 * menu (grouped by server, listing each server's real vaults).
 */
export interface ConnectionDetail {
	/** The connection's display name. */
	name: string;
	/** Whether this is a local daemon or a remote server. */
	kind: 'local' | 'remote';
	/** Human label for the kind: "Local daemon" / "Remote server". */
	kindLabel: string;
	/** Reachability summary: "Always available" / "Checking…" / "Live · 42 ms" / "Offline — …". */
	statusLabel: string;
	/** Status dot mirroring {@link connectionBadge}. */
	dot: BadgeDot;
	/** The server URL for a remote connection, or `null` for local. */
	url: string | null;
}

/**
 * Build the detail model shown when the badge is opened. Local connections are
 * always available (no probe); remote connections summarise the reachability
 * probe (checking → live+latency → offline+error).
 */
export function connectionDetail(
	identity: ConnectionIdentity,
	status: ConnectionTestResult | null,
	checking: boolean,
	url: string | null = null
): ConnectionDetail {
	if (!identity.remote) {
		return {
			name: identity.name,
			kind: 'local',
			kindLabel: 'Local daemon',
			statusLabel: 'Always available',
			dot: 'none',
			url: null
		};
	}

	let dot: BadgeDot = 'checking';
	let statusLabel = 'Checking…';
	if (!checking && status) {
		if (status.reachable) {
			dot = 'live';
			statusLabel =
				typeof status.latency_ms === 'number' ? `Live · ${status.latency_ms} ms` : 'Live';
		} else {
			dot = 'offline';
			statusLabel = status.error ? `Offline — ${status.error}` : 'Offline';
		}
	}

	return { name: identity.name, kind: 'remote', kindLabel: 'Remote server', statusLabel, dot, url };
}

/**
 * The window-title suffix for a connection: the server name for a remote window,
 * or `null` for local (no suffix). Used to badge remote window titles.
 */
export function titleServerSuffix(identity: ConnectionIdentity): string | null {
	return identity.remote ? identity.name : null;
}

/**
 * The inline source pill shown after the vault name in the sidebar top-left
 * label (Idea 2 → variant 2). It tells the user, at a glance, whether the vault
 * they're looking at lives on the local daemon or a named remote server.
 *
 * - **Local**: laptop glyph + the literal "Local".
 * - **Remote**: cloud glyph + the server's display name.
 */
export interface VaultSourceBadge {
	/** 💻 for local, ☁ for a remote server. */
	icon: string;
	/** "Local" for the local daemon, or the server name for a remote one. */
	label: string;
	/** Whether the vault is on a remote server (drives the accent styling). */
	remote: boolean;
}

/** Build the sidebar source pill for a window's connection identity. */
export function vaultSourceBadge(identity: ConnectionIdentity): VaultSourceBadge {
	return identity.remote
		? { icon: '☁', label: identity.name, remote: true }
		: { icon: '💻', label: 'Local', remote: false };
}
