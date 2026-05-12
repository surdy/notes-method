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

export function connectSSE(
	vault: string,
	onEvent: VaultEventHandler,
	onReconnect?: () => void
): EventSource {
	const source = new EventSource(`/api/v/${encodeURIComponent(vault)}/events`);

	source.onmessage = (e) => {
		try {
			const event: VaultEvent = JSON.parse(e.data);
			onEvent(event);
		} catch {
			// Ignore keep-alives and other non-JSON payloads.
		}
	};

	source.onerror = () => {
		console.warn('SSE connection error, will reconnect...');
	};

	// EventSource fires 'open' on each (re)connection; skip the first open.
	let isFirstOpen = true;
	source.onopen = () => {
		if (isFirstOpen) {
			isFirstOpen = false;
		} else if (onReconnect) {
			onReconnect();
		}
	};

	return source;
}
