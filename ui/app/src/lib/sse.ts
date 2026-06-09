import { writable } from 'svelte/store';
import { API_BASE } from './api/core.ts';

export type VaultEventHandler = (event: VaultEvent) => void;
export type ConnectionState = 'connected' | 'reconnecting' | 'disconnected';

export interface ConfigDetail {
	key: 'sidebar' | 'vault';
	status: 'changed' | 'removed' | 'error';
	error?: string;
}

export interface VaultEvent {
	id?: number;
	vault: string;
	type: string;
	path: string;
	timestamp: string;
	config?: ConfigDetail;
	/**
	 * Blake3 hex hash of the note contents. Present on `note.created`,
	 * `note.updated`, and other content-change events; absent otherwise.
	 * Clients use this to recognise echoes of their own writes and
	 * suppress spurious "file changed on disk" warnings.
	 */
	hash?: string;
}

const NAMED_EVENT_TYPES = [
	'note.created',
	'note.updated',
	'note.moved',
	'note.deleted',
	'task.updated',
	'note.captured',
	'daily.created',
	'cache.rebuilt',
	'search.reindexed',
	'config.changed',
	'config.removed',
	'config.error',
	'vaults.changed',
	'shutting_down'
] as const;

const INITIAL_RETRY_MS = 1000;
const MAX_RETRY_MS = 30000;
const BACKOFF_FACTOR = 2;

export interface SSEConnection {
	close: () => void;
}

export const connectionState = writable<ConnectionState>('disconnected');
export const daemonShuttingDown = writable(false);

export function connectSSE(
	vault: string,
	onEvent: VaultEventHandler,
	onReconnect?: () => void
): SSEConnection {
	let source: EventSource | null = null;
	let retryMs = INITIAL_RETRY_MS;
	let retryTimer: ReturnType<typeof setTimeout> | null = null;
	let closed = false;
	let lastEventId: string | undefined;
	let isFirstOpen = true;

	const handleMessage = (e: MessageEvent<string>) => {
		try {
			const event: VaultEvent = JSON.parse(e.data);
			if (e.lastEventId) {
				lastEventId = e.lastEventId;
			}
			if (event.type === 'shutting_down') {
				console.info('Notesmith daemon is shutting down');
				daemonShuttingDown.set(true);
			}
			onEvent(event);
		} catch {
			// Ignore keep-alives and other non-JSON payloads.
		}
	};

	function connect() {
		if (closed) {
			return;
		}

		const url = `${API_BASE}/api/v/${encodeURIComponent(vault)}/events`;
		const sourceUrl = lastEventId ? `${url}?last_event_id=${encodeURIComponent(lastEventId)}` : url;
		source = new EventSource(sourceUrl);
		source.onmessage = handleMessage;
		for (const eventType of NAMED_EVENT_TYPES) {
			source.addEventListener(eventType, handleMessage as EventListener);
		}

		source.onopen = () => {
			retryMs = INITIAL_RETRY_MS;
			connectionState.set('connected');
			daemonShuttingDown.set(false);

			if (isFirstOpen) {
				isFirstOpen = false;
			} else if (onReconnect) {
				void Promise.resolve(onReconnect());
			}
		};

		source.onerror = () => {
			source?.close();
			source = null;

			if (closed || retryTimer) {
				return;
			}

			connectionState.set('reconnecting');
			retryTimer = setTimeout(() => {
				retryTimer = null;
				retryMs = Math.min(retryMs * BACKOFF_FACTOR, MAX_RETRY_MS);
				connect();
			}, retryMs);
		};
	}
 
	connect();

	return {
		close() {
			closed = true;
			if (retryTimer) {
				clearTimeout(retryTimer);
				retryTimer = null;
			}
			source?.close();
			source = null;
			connectionState.set('disconnected');
		}
	};
}
