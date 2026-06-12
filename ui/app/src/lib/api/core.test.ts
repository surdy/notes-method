import { afterEach, describe, expect, it, vi } from 'vitest';

import { apiFetch, isNetworkError, resolveApiBase, resolveDaemonOrigin, versionMismatch } from './core.ts';

afterEach(() => {
	versionMismatch.set(null);
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

describe('apiFetch', () => {
	it('retries transient network errors before succeeding', async () => {
		vi.useFakeTimers();
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockRejectedValueOnce(new TypeError('Failed to fetch'))
			.mockRejectedValueOnce(new TypeError('Failed to fetch'))
			.mockResolvedValueOnce(new Response('ok', { status: 200 }));
		vi.stubGlobal('fetch', fetchMock);

		const responsePromise = apiFetch('/api/status');

		await vi.runOnlyPendingTimersAsync();
		await vi.runOnlyPendingTimersAsync();

		const response = await responsePromise;

		expect(response.status).toBe(200);
		expect(fetchMock).toHaveBeenCalledTimes(3);
	});

	it('does not retry successful fetch responses with error status codes', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response('boom', { status: 500 }));
		vi.stubGlobal('fetch', fetchMock);

		const response = await apiFetch('/api/status');

		expect(response.status).toBe(500);
		expect(fetchMock).toHaveBeenCalledTimes(1);
	});
});

describe('isNetworkError', () => {
	it('detects fetch type errors', () => {
		expect(isNetworkError(new TypeError('Failed to fetch'))).toBe(true);
		expect(isNetworkError(new Error('Failed to fetch'))).toBe(false);
	});
});

describe('resolveApiBase', () => {
	it('uses apiBase from the embedded desktop app URL', () => {
		const url = new URL(
			'notesmith-app://localhost/app/?apiBase=http%3A%2F%2F100.64.0.10%3A27183&vault=work'
		);

		expect(resolveApiBase(url)).toBe('http://100.64.0.10:27183');
	});

	it('ignores missing or unsafe apiBase values', () => {
		expect(resolveApiBase(new URL('notesmith-app://localhost/app/'))).toBe('');
		expect(resolveApiBase(new URL('notesmith-app://localhost/app/?apiBase=javascript:alert(1)'))).toBe(
			''
		);
	});
});

describe('resolveDaemonOrigin', () => {
	it('prefers an explicit apiBase (embedded/remote desktop mode)', () => {
		expect(
			resolveDaemonOrigin('http://100.64.0.10:27183', {
				origin: 'notesmith-app://localhost',
				protocol: 'notesmith-app:'
			})
		).toBe('http://100.64.0.10:27183');
	});

	it('falls back to the http(s) page origin in daemon mode', () => {
		expect(
			resolveDaemonOrigin('', { origin: 'http://127.0.0.1:27183', protocol: 'http:' })
		).toBe('http://127.0.0.1:27183');
		expect(
			resolveDaemonOrigin('', { origin: 'https://notes.example', protocol: 'https:' })
		).toBe('https://notes.example');
	});

	it('returns empty when no apiBase and the origin is the custom app protocol', () => {
		expect(
			resolveDaemonOrigin('', { origin: 'notesmith-app://localhost', protocol: 'notesmith-app:' })
		).toBe('');
	});

	it('returns empty when no apiBase and no location is available', () => {
		expect(resolveDaemonOrigin('', null)).toBe('');
	});
});
