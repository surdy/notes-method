import { afterEach, describe, expect, it, vi } from 'vitest';

import { grant, listGrants, revoke } from './permissions.ts';

function jsonResponse(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

function emptyResponse(status = 204): Response {
	return new Response(null, { status });
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('permissions api client', () => {
	it('listGrants GETs the vault-scoped endpoint and returns the array', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse(['append_note', 'create_note']));
		vi.stubGlobal('fetch', fetchMock);

		const grants = await listGrants('work');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/agent/permissions');
		expect(grants).toEqual(['append_note', 'create_note']);
	});

	it('grant POSTs the tool name', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(emptyResponse());
		vi.stubGlobal('fetch', fetchMock);

		await grant('work', 'create_note');
		const [url, init] = fetchMock.mock.calls[0];
		expect(String(url)).toBe('/api/v/work/agent/permissions');
		expect(init?.method).toBe('POST');
		expect(JSON.parse(String(init?.body))).toEqual({ tool: 'create_note' });
	});

	it('revoke DELETEs the tool-scoped endpoint', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(emptyResponse());
		vi.stubGlobal('fetch', fetchMock);

		await revoke('work', 'create_note');
		const [url, init] = fetchMock.mock.calls[0];
		expect(String(url)).toBe('/api/v/work/agent/permissions/create_note');
		expect(init?.method).toBe('DELETE');
	});

	it('revoke tolerates a 404 (already absent)', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(emptyResponse(404));
		vi.stubGlobal('fetch', fetchMock);
		await expect(revoke('work', 'gone')).resolves.toBeUndefined();
	});

	it('listGrants surfaces an outdated-server error on 404', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(emptyResponse(404));
		vi.stubGlobal('fetch', fetchMock);
		await expect(listGrants('work')).rejects.toMatchObject({
			status: 404,
			code: 'permission_api_unavailable'
		});
	});
});
