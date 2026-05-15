import { writable } from 'svelte/store';

export const API_BASE = '';
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

export class ApiError extends Error {
	readonly status: number;

	constructor(message: string, status: number) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
	}
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
