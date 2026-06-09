import { get } from 'svelte/store';

import { ApiError, versionMismatch } from './core.ts';

export type ErrorCategory =
	| 'service-not-running'
	| 'connection-lost'
	| 'endpoint-not-found'
	| 'server-error'
	| 'conflict'
	| 'rate-limited'
	| 'unknown';

export interface ClassifiedError {
	category: ErrorCategory;
	title: string;
	message: string;
	hint: string;
	action?: { label: string; type: 'start' | 'reconnect' | 'update' | 'rebuild' | 'retry' };
	retryable: boolean;
	retryCount?: number;
}

const SAFETY_HINT = 'Your markdown files are safe on disk.';

/**
 * The daemon returns either the structured code `"vault_not_found"` (newer
 * routes) or a message-style code `"vault not found: <name>"` (older
 * routes). Both signal the same condition.
 */
function isVaultNotFoundCode(code: string | undefined): boolean {
	if (!code) return false;
	return code === 'vault_not_found' || code.startsWith('vault not found');
}

const SERVICE_NOT_RUNNING_RE =
	/connection refused|econnrefused|failed to fetch|fetch failed|load failed|networkerror/i;
const CONNECTION_LOST_RE =
	/aborted|connection reset|econnreset|network connection was lost|network request failed|\bnetwork\b/i;

export function classifyError(error: unknown, endpointHint?: string): ClassifiedError {
	const retryCount = extractRetryCount(error);
	if (error instanceof TypeError) {
		const message = error.message.trim();
		if (CONNECTION_LOST_RE.test(message)) {
			return {
				category: 'connection-lost',
				title: 'Connection lost',
				message: `Notesmith lost contact with the background service while trying to ${describeOperation(endpointHint)}. Reconnect and try again.`,
				hint: SAFETY_HINT,
				action: { label: 'Reconnect', type: 'reconnect' },
				retryable: true,
				retryCount
			};
		}

		if (SERVICE_NOT_RUNNING_RE.test(message)) {
			return {
				category: 'service-not-running',
				title: 'Service not running',
				message: `Notesmith could not ${describeOperation(endpointHint)} because the background service is not reachable. Start the service, then try again.`,
				hint: SAFETY_HINT,
				action: { label: 'Start service', type: 'start' },
				retryable: true,
				retryCount
			};
		}
	}

	if (error instanceof ApiError) {
		if (error.status === 404) {
			// A 404 carrying `vault_not_found` (or the legacy "vault not
			// found: <name>" message) means the requested vault no longer
			// exists. This happens after the vault is removed from another
			// window (or from Settings). Surfacing the generic "version
			// mismatch" message would be misleading.
			if (isVaultNotFoundCode(error.code)) {
				return {
					category: 'endpoint-not-found',
					title: 'Vault no longer exists',
					message:
						'This vault has been removed. Open another vault or add a new one from Settings.',
					hint: SAFETY_HINT,
					retryable: false,
					retryCount
				};
			}
			if (endpointHint) {
				return classifyEndpointNotFound(endpointHint, retryCount);
			}
		}
		if (error.status === 409) {
			return {
				category: 'conflict',
				title: 'Save conflict',
				message: `Notesmith stopped before overwriting a newer version while trying to ${describeOperation(endpointHint)}. Reload the latest note content, then apply any edits you still want.`,
				hint: SAFETY_HINT,
				action: { label: 'Reload note', type: 'retry' },
				retryable: false,
				retryCount
			};
		}
		if (error.status === 429) {
			return {
				category: 'rate-limited',
				title: 'Too many requests',
				message: `Notesmith is being asked to ${describeOperation(endpointHint)} too quickly right now. Wait a moment, then try again.`,
				hint: SAFETY_HINT,
				action: { label: 'Try again', type: 'retry' },
				retryable: true,
				retryCount
			};
		}
		if (error.status >= 500) {
			return {
				category: 'server-error',
				title: 'Service error',
				message: `The background service hit a problem while trying to ${describeOperation(endpointHint)}. Try again in a moment.`,
				hint: SAFETY_HINT,
				action: { label: 'Try again', type: 'retry' },
				retryable: true,
				retryCount
			};
		}
	}

	return {
		category: 'unknown',
		title: 'Request failed',
		message: `Notesmith could not ${describeOperation(endpointHint)}. Check the background service, then try again.`,
		hint: SAFETY_HINT,
		action: { label: 'Try again', type: 'retry' },
		retryable: false,
		retryCount
	};
}

function classifyEndpointNotFound(endpointHint: string, retryCount?: number): ClassifiedError {
	const mismatch = get(versionMismatch);
	if (mismatch?.direction === 'service-outdated') {
		return {
			category: 'endpoint-not-found',
			title: 'Service update required',
			message: `This app can ${describeOperation(endpointHint)}, but the connected background service is older (service ${mismatch.serverVersion}, app ${mismatch.clientVersion}). Restart or update the service, then try again.`,
			hint: SAFETY_HINT,
			action: { label: 'Reload app', type: 'update' },
			retryable: false,
			retryCount
		};
	}

	if (mismatch?.direction === 'app-outdated') {
		return {
			category: 'endpoint-not-found',
			title: 'App update required',
			message: `The background service supports ${describeOperation(endpointHint)}, but this app is older (service ${mismatch.serverVersion}, app ${mismatch.clientVersion}). Reload or update Notesmith, then try again.`,
			hint: SAFETY_HINT,
			action: { label: 'Reload app', type: 'update' },
			retryable: false,
			retryCount
		};
	}

	return {
		category: 'endpoint-not-found',
		title: 'Version mismatch detected',
		message: `Notesmith could not ${describeOperation(endpointHint)} because the connected service does not recognize that endpoint. Reload or update Notesmith so the app and service are on the same version.`,
		hint: SAFETY_HINT,
		action: { label: 'Reload app', type: 'update' },
		retryable: false,
		retryCount
	};
}

function describeOperation(endpointHint?: string): string {
	switch (endpointHint) {
		case 'list-notes':
			return 'load your notes list';
		case 'note-detail':
			return 'open this note';
		case 'save-note':
			return 'save your latest changes';
		case 'toggle-task':
			return 'update that task';
		default:
			return 'finish that request';
	}
}

function extractRetryCount(error: unknown): number | undefined {
	if (!error || (typeof error !== 'object' && typeof error !== 'function')) {
		return undefined;
	}

	const value = Reflect.get(error, 'retryCount');
	return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}
