export type VaultEventHandler = (event: VaultEvent) => void;

export interface VaultEvent {
	vault: string;
	type: string;
	path: string;
	timestamp: string;
}

export function connectSSE(vault: string, onEvent: VaultEventHandler): EventSource {
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

	return source;
}
