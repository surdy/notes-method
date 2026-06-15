/**
 * Connection transport for the desktop's saved-server list. The Tauri shell
 * owns `servers.json` (the system of record) and exposes it over IPC; this
 * module abstracts that boundary behind {@link ConnectionClient} so the
 * Settings → Connection panel and the status-bar switcher share one client and
 * degrade gracefully when not running inside Tauri (plain browser / hosted).
 */

/** Reserved id for the implicit local daemon ("This Mac"). */
export const LOCAL_ID = 'local';

/** Token-less view of a saved server (matches the Rust `ServerView`). */
export interface ServerView {
	id: string;
	name: string;
	url: string;
	has_token: boolean;
}

/** The saved-server list plus the active connection id (`"local"` for local). */
export interface ConnectionList {
	active_id: string;
	servers: ServerView[];
}

/** Result of probing a candidate daemon URL (matches the Rust `ConnectionTestResult`). */
export interface ConnectionTestResult {
	reachable: boolean;
	latency_ms?: number;
	vault_count?: number;
	error?: string;
}

/** Fields submitted when adding a server. */
export interface ConnectionInput {
	name: string;
	url: string;
	token?: string | null;
}

/** Partial update; omitted fields are left unchanged. A blank token clears it. */
export interface ConnectionPatch {
	name?: string | null;
	url?: string | null;
	token?: string | null;
}

export interface ConnectionClient {
	/** Whether a desktop transport is available (false in a plain browser). */
	available(): boolean;
	list(): Promise<ConnectionList>;
	add(input: ConnectionInput): Promise<ServerView>;
	update(id: string, patch: ConnectionPatch): Promise<ServerView>;
	remove(id: string): Promise<void>;
	/** Switch the active connection. `null`/`"local"` selects the local daemon. */
	setActive(id: string | null): Promise<ConnectionList>;
	test(url: string, token?: string | null): Promise<ConnectionTestResult>;
	/** Subscribe to active-connection changes. Returns an unsubscribe fn. */
	onChanged(cb: (list: ConnectionList) => void): () => void;
}

const CONNECTION_CHANGED = 'notesmith://connection-changed';

interface TauriBridge {
	invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
	listen: (event: string, handler: (event: unknown) => void) => Promise<() => void>;
}

/** Unwrap a Tauri event object (`{ payload }`) to its payload, tolerating a raw payload. */
function eventPayload(event: unknown): unknown {
	if (event && typeof event === 'object' && 'payload' in event) {
		return (event as { payload: unknown }).payload;
	}
	return event;
}

/** Resolve the global Tauri bridge, or `null` when not running inside Tauri. */
export function resolveTauriBridge(): TauriBridge | null {
	const w = globalThis as unknown as {
		__TAURI__?: {
			core?: { invoke?: TauriBridge['invoke'] };
			event?: { listen?: TauriBridge['listen'] };
		};
	};
	const invoke = w.__TAURI__?.core?.invoke;
	const listen = w.__TAURI__?.event?.listen;
	if (!invoke || !listen) return null;
	return { invoke: invoke.bind(w.__TAURI__!.core), listen: listen.bind(w.__TAURI__!.event) };
}

/** Connection client backed by Tauri IPC commands + events. */
export class TauriConnectionClient implements ConnectionClient {
	constructor(private readonly bridge: TauriBridge) {}

	available(): boolean {
		return true;
	}

	async list(): Promise<ConnectionList> {
		return (await this.bridge.invoke('connection_list')) as ConnectionList;
	}

	async add(input: ConnectionInput): Promise<ServerView> {
		return (await this.bridge.invoke('connection_add', {
			name: input.name,
			url: input.url,
			token: input.token ?? null
		})) as ServerView;
	}

	async update(id: string, patch: ConnectionPatch): Promise<ServerView> {
		return (await this.bridge.invoke('connection_update', {
			id,
			name: patch.name ?? null,
			url: patch.url ?? null,
			token: patch.token ?? null
		})) as ServerView;
	}

	async remove(id: string): Promise<void> {
		await this.bridge.invoke('connection_remove', { id });
	}

	async setActive(id: string | null): Promise<ConnectionList> {
		return (await this.bridge.invoke('connection_set_active', { id })) as ConnectionList;
	}

	async test(url: string, token?: string | null): Promise<ConnectionTestResult> {
		return (await this.bridge.invoke('connection_test', {
			url,
			token: token ?? null
		})) as ConnectionTestResult;
	}

	onChanged(cb: (list: ConnectionList) => void): () => void {
		let unlisten: (() => void) | null = null;
		let disposed = false;
		void this.bridge
			.listen(CONNECTION_CHANGED, (event) => {
				const list = eventPayload(event) as ConnectionList | undefined;
				if (list) cb(list);
			})
			.then((fn) => {
				if (disposed) fn();
				else unlisten = fn;
			});
		return () => {
			disposed = true;
			unlisten?.();
		};
	}
}

/** A no-op client used when not running inside Tauri (plain browser / hosted). */
export class UnavailableConnectionClient implements ConnectionClient {
	available(): boolean {
		return false;
	}
	async list(): Promise<ConnectionList> {
		return { active_id: LOCAL_ID, servers: [] };
	}
	async add(): Promise<ServerView> {
		throw new Error('Connections can only be managed in the desktop app.');
	}
	async update(): Promise<ServerView> {
		throw new Error('Connections can only be managed in the desktop app.');
	}
	async remove(): Promise<void> {
		throw new Error('Connections can only be managed in the desktop app.');
	}
	async setActive(): Promise<ConnectionList> {
		throw new Error('Connections can only be managed in the desktop app.');
	}
	async test(): Promise<ConnectionTestResult> {
		throw new Error('Connections can only be managed in the desktop app.');
	}
	onChanged(): () => void {
		return () => {};
	}
}

/** Build the appropriate client for the current runtime. */
export function createConnectionClient(): ConnectionClient {
	const bridge = resolveTauriBridge();
	return bridge ? new TauriConnectionClient(bridge) : new UnavailableConnectionClient();
}

/**
 * Validate the add/edit form. Returns a human-readable error, or `null` when
 * the input is valid. Mirrors the backend `ServerEntry` validation so the UI
 * can surface problems inline before the IPC round-trip.
 */
export function validateConnectionForm(input: { name: string; url: string }): string | null {
	if (input.name.trim().length === 0) {
		return 'Name is required.';
	}
	const url = input.url.trim();
	if (url.length === 0) {
		return 'Server URL is required.';
	}
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		return 'Enter a valid URL, e.g. https://notes.example.com';
	}
	if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
		return 'Server URL must start with http:// or https://';
	}
	return null;
}

/** Summarize a test result as a short status line for the UI. */
export function describeTestResult(result: ConnectionTestResult): string {
	if (!result.reachable) {
		return result.error ?? 'Unreachable';
	}
	const parts: string[] = ['Reachable'];
	if (typeof result.latency_ms === 'number') {
		parts.push(`${result.latency_ms} ms`);
	}
	if (typeof result.vault_count === 'number') {
		parts.push(`${result.vault_count} vault${result.vault_count === 1 ? '' : 's'}`);
	}
	return parts.join(' · ');
}
