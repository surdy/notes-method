import type { ConnectionState } from '$lib/sse';

/**
 * Visual state for the connection-status pill. Drives both the dot colour and
 * the label. Kept as a pure function so the (now non-trivial) decision can be
 * unit-tested without mounting the Svelte component.
 */
export type StatusVisualState =
	| 'live'
	| 'no-vault'
	| 'connecting'
	| 'reconnecting'
	| 'offline'
	| 'restart-required'
	| 'rebuilding';

export interface StatusVisualInputs {
	/** A local or server-side index rebuild is in progress. */
	isRebuilding: boolean;
	/** The daemon requires a restart to apply config changes. */
	restartRequired: boolean;
	/** The vault this pill is reporting on, or '' when none is selected. */
	currentVault: string;
	/** Live SSE connection state. */
	connectionState: ConnectionState;
	/** A daemon status poll succeeded recently (within the poll window). */
	hasRecentStatus: boolean;
	/**
	 * The first status poll has resolved at least once (success or failure)
	 * since this surface mounted. Distinguishes the initial "establishing the
	 * connection" window from a genuine offline daemon.
	 */
	firstStatusResolved: boolean;
}

/**
 * Decide the pill's visual state. The key UX rule: during the initial
 * connection window — e.g. right after a connection switch reloads the page
 * onto a new daemon URL, or on first launch — show a neutral `connecting`
 * state rather than flashing `offline`. We only report `offline` once we have
 * actually connected and a status poll has resolved without a recent success.
 */
export function connectionVisualState(input: StatusVisualInputs): StatusVisualState {
	const {
		isRebuilding,
		restartRequired,
		currentVault,
		connectionState,
		hasRecentStatus,
		firstStatusResolved
	} = input;

	if (isRebuilding) return 'rebuilding';
	if (restartRequired) return 'restart-required';

	// No vault selected: SSE is not open, derive state from the daemon status
	// poll only. Stay neutral until the first poll resolves.
	if (!currentVault) {
		if (hasRecentStatus) return 'no-vault';
		return firstStatusResolved ? 'offline' : 'connecting';
	}

	if (connectionState === 'connected' && hasRecentStatus) return 'live';
	if (connectionState === 'reconnecting') return 'reconnecting';

	// Initial establishment window: SSE still handshaking, or the first status
	// poll hasn't returned yet. Surface "Connecting…" instead of "Offline".
	if (connectionState !== 'connected' || !firstStatusResolved) return 'connecting';

	return 'offline';
}

export interface PillLabelOverrides {
	isRebuilding: boolean;
	restarting: boolean;
	daemonShuttingDown: boolean;
}

/** Human-readable label for the pill, layering restart/rebuild overrides. */
export function connectionPillLabel(
	visualState: StatusVisualState,
	overrides: PillLabelOverrides
): string {
	if (overrides.isRebuilding) return 'Rebuilding index';
	if (overrides.restarting || overrides.daemonShuttingDown) return 'Restarting…';

	switch (visualState) {
		case 'live':
			return 'Live';
		case 'no-vault':
			return 'No vault open';
		case 'connecting':
			return 'Connecting…';
		case 'reconnecting':
			return 'Reconnecting';
		case 'restart-required':
			return 'Restart required';
		case 'rebuilding':
			return 'Rebuilding index';
		default:
			return 'Offline';
	}
}
