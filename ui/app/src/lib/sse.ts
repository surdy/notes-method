import { writable } from 'svelte/store';

export type VaultEventHandler = (event: VaultEvent) => void;

export interface ConfigDetail {
	key: 'sidebar' | 'vault';
	status: 'changed' | 'removed' | 'error';
	error?: string;
}

export interface VaultEvent {
	vault: string;
	type: string;
	path: string;
	timestamp: string;
	config?: ConfigDetail;
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

export const daemonShuttingDown = writable(false);

export function connectSSE(
	vault: string,
	onEvent: VaultEventHandler,
	onReconnect?: () => void
): EventSource {
	const source = new EventSource(`/api/v/${encodeURIComponent(vault)}/events`);

	const handleMessage = (e: MessageEvent<string>) => {
		try {
			const event: VaultEvent = JSON.parse(e.data);
			if (event.type === 'shutting_down') {
				console.info('Notesmith daemon is shutting down');
				daemonShuttingDown.set(true);
			}
			onEvent(event);
		} catch {
			// Ignore keep-alives and other non-JSON payloads.
		}
	};

	source.onmessage = handleMessage;
	for (const eventType of NAMED_EVENT_TYPES) {
		source.addEventListener(eventType, handleMessage as EventListener);
	}

	source.onerror = () => {
		console.warn('SSE connection error, will reconnect...');
	};

	// EventSource fires 'open' on each (re)connection; skip the first open.
	let isFirstOpen = true;
	source.onopen = () => {
		daemonShuttingDown.set(false);
		if (isFirstOpen) {
			isFirstOpen = false;
		} else if (onReconnect) {
			onReconnect();
		}
	};

	return source;
}
