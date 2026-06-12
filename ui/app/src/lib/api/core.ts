import { writable } from 'svelte/store';

export const API_BASE = resolveApiBase();
export const CLIENT_VERSION = '0.1.0';
export const CLIENT_SCHEMA_VERSION = '1';
const MAX_SILENT_RETRIES = 3;
const RETRY_DELAYS = [500, 1000, 2000] as const;

export type VersionMismatchDirection = 'service-outdated' | 'app-outdated';

export interface VersionInfo {
	serverVersion: string;
	schemaVersion: string;
	clientVersion: string;
	compatible: boolean;
	direction?: VersionMismatchDirection;
}

export const versionMismatch = writable<VersionInfo | null>(null);

export function resolveApiBase(source: Pick<Location, 'search'> | URL | null = currentLocation()): string {
	if (!source) return '';
	const raw = new URLSearchParams(source.search).get('apiBase')?.trim();
	if (!raw) return '';
	try {
		const url = new URL(raw);
		if (url.protocol !== 'http:' && url.protocol !== 'https:') {
			return '';
		}
		const path = url.pathname === '/' ? '' : url.pathname.replace(/\/+$/, '');
		return `${url.origin}${path}`;
	} catch {
		return '';
	}
}

function currentLocation(): Pick<Location, 'search'> | null {
	return typeof globalThis.location === 'undefined' ? null : globalThis.location;
}

/**
 * Resolve the absolute daemon origin for wiring external processes — e.g. an
 * agent's MCP endpoint (ADR 0011/0012) — that cannot use the relative URLs the
 * in-page client relies on.
 *
 * Prefers the explicit `apiBase` (Embedded/remote desktop mode, where the
 * frontend is bundled and served from the `notesmith-app://` protocol). In
 * Daemon mode the frontend is served same-origin from the daemon, so `apiBase`
 * is empty and the daemon origin is `window.location.origin` — but only when
 * that origin is a real `http(s)` origin, never the custom app protocol used
 * for bundled assets.
 */
export function resolveDaemonOrigin(
	apiBase: string = API_BASE,
	location: Pick<Location, 'origin' | 'protocol'> | null = currentLocationOrigin()
): string {
	if (apiBase) {
		return apiBase;
	}
	if (location && (location.protocol === 'http:' || location.protocol === 'https:')) {
		return location.origin;
	}
	return '';
}

function currentLocationOrigin(): Pick<Location, 'origin' | 'protocol'> | null {
	return typeof globalThis.location === 'undefined' ? null : globalThis.location;
}

export class ApiError extends Error {
	readonly status: number;
	/**
	 * Optional machine-readable error code parsed from the daemon's JSON
	 * response body (the `error` field). Lets the UI distinguish, for
	 * example, a 404 caused by a vault that no longer exists from a 404
	 * caused by a missing endpoint (version mismatch).
	 */
	readonly code?: string;

	constructor(message: string, status: number, code?: string) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
		this.code = code;
	}
}

/**
 * Try to parse `{ "error": "...", "message": "..." }` from a fetch Response
 * body. Returns undefined if the body is not JSON or has no recognisable
 * error fields. Consumes the response body.
 */
export async function readErrorBody(res: Response): Promise<{ code?: string; message?: string }> {
	try {
		const data = await res.clone().json();
		if (data && typeof data === 'object') {
			const code = typeof data.error === 'string' ? data.error : undefined;
			const message = typeof data.message === 'string' ? data.message : undefined;
			return { code, message };
		}
	} catch {
		// Body wasn't JSON — fall through.
	}
	return {};
}

const NETWORK_ERROR_RE =
	/aborted|connection refused|connection reset|econnrefused|econnreset|failed to fetch|fetch failed|load failed|network(?:error| connection was lost| request failed)?/i;

export function encodePath(path: string): string {
	return path
		.split('/')
		.map((segment) => encodeURIComponent(segment))
		.join('/');
}

export function classifyVersionCompatibility(
	serverVersion: string,
	schemaVersion: string
): VersionInfo {
	const serverSchema = parseIntegerVersion(schemaVersion);
	const clientSchema = parseIntegerVersion(CLIENT_SCHEMA_VERSION);

	if (serverSchema !== null && clientSchema !== null && serverSchema !== clientSchema) {
		return {
			serverVersion,
			schemaVersion,
			clientVersion: CLIENT_VERSION,
			compatible: false,
			direction: serverSchema < clientSchema ? 'service-outdated' : 'app-outdated'
		};
	}

	const serverMajor = parseMajorVersion(serverVersion);
	const clientMajor = parseMajorVersion(CLIENT_VERSION);
	if (serverMajor !== null && clientMajor !== null) {
		if (serverMajor === clientMajor) {
			return {
				serverVersion,
				schemaVersion,
				clientVersion: CLIENT_VERSION,
				compatible: true
			};
		}

		return {
			serverVersion,
			schemaVersion,
			clientVersion: CLIENT_VERSION,
			compatible: false,
			direction: serverMajor < clientMajor ? 'service-outdated' : 'app-outdated'
		};
	}

	return {
		serverVersion,
		schemaVersion,
		clientVersion: CLIENT_VERSION,
		compatible: serverVersion === CLIENT_VERSION && schemaVersion === CLIENT_SCHEMA_VERSION
	};
}

export function checkVersionHeaders(response: Response): void {
	const serverVersion = response.headers.get('X-Notesmith-Server-Version');
	const schemaVersion = response.headers.get('X-Notesmith-Schema-Version');

	if (!serverVersion || !schemaVersion) {
		return;
	}

	const info = classifyVersionCompatibility(serverVersion, schemaVersion);
	versionMismatch.set(info.compatible ? null : info);
}

export async function apiFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
	const headers = new Headers(input instanceof Request ? input.headers : undefined);
	if (init?.headers) {
		for (const [key, value] of new Headers(init.headers).entries()) {
			headers.set(key, value);
		}
	}
	headers.set('X-Notesmith-Client-Version', CLIENT_VERSION);

	let lastError: unknown;
	for (let attempt = 0; attempt <= MAX_SILENT_RETRIES; attempt += 1) {
		try {
			const response = await fetch(input, { ...init, headers });
			checkVersionHeaders(response);
			return response;
		} catch (error) {
			lastError = annotateRetryCount(error, attempt);
			if (!isNetworkError(error) || attempt === MAX_SILENT_RETRIES) {
				throw lastError;
			}
			await delay(RETRY_DELAYS[attempt] ?? RETRY_DELAYS.at(-1) ?? 2000);
		}
	}

	throw lastError;
}

export function isNetworkError(error: unknown): boolean {
	return error instanceof TypeError && NETWORK_ERROR_RE.test(error.message);
}

function annotateRetryCount(error: unknown, retryCount: number): unknown {
	if (!error || (typeof error !== 'object' && typeof error !== 'function')) {
		return error;
	}

	try {
		Reflect.set(error, 'retryCount', retryCount);
	} catch {
		// Ignore metadata failures and rethrow the original error.
	}

	return error;
}

function delay(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

function parseMajorVersion(version: string): number | null {
	const match = /^(\d+)/.exec(version.trim());
	if (!match) {
		return null;
	}

	const major = Number.parseInt(match[1], 10);
	return Number.isNaN(major) ? null : major;
}

function parseIntegerVersion(version: string): number | null {
	const parsed = Number.parseInt(version, 10);
	return Number.isNaN(parsed) ? null : parsed;
}
