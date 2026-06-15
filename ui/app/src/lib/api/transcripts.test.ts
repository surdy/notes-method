import { afterEach, describe, expect, it, vi } from 'vitest';

import {
	appendMessage,
	createThread,
	deleteThread,
	listMessages,
	listThreads,
	renameThread
} from './transcripts.ts';

function jsonResponse(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('transcripts api client', () => {
	it('listThreads GETs the vault-scoped endpoint', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
			jsonResponse([{ id: 't1', vault: 'work', title: 'Hi' }])
		);
		vi.stubGlobal('fetch', fetchMock);

		const threads = await listThreads('work');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/agent/threads');
		expect(threads[0].id).toBe('t1');
	});

	it('createThread POSTs title/agent/model', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ id: 't9', vault: 'work', title: 'Plan' }, 201));
		vi.stubGlobal('fetch', fetchMock);

		const thread = await createThread('work', 'Plan', 'copilot', 'gpt-5');
		const [url, init] = fetchMock.mock.calls[0];
		expect(String(url)).toBe('/api/v/work/agent/threads');
		expect(init?.method).toBe('POST');
		expect(JSON.parse(String(init?.body))).toEqual({
			title: 'Plan',
			agent: 'copilot',
			model: 'gpt-5'
		});
		expect(thread.id).toBe('t9');
	});

	it('appendMessage POSTs role and content', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
			jsonResponse({ id: 1, thread_id: 't1', seq: 1, role: 'user', content: 'hi' }, 201)
		);
		vi.stubGlobal('fetch', fetchMock);

		const msg = await appendMessage('work', 't1', 'user', 'hi');
		const [url, init] = fetchMock.mock.calls[0];
		expect(String(url)).toBe('/api/v/work/agent/threads/t1/messages');
		expect(JSON.parse(String(init?.body))).toEqual({ role: 'user', content: 'hi' });
		expect(msg.seq).toBe(1);
	});

	it('listMessages GETs the messages endpoint', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
			jsonResponse([{ id: 1, thread_id: 't1', seq: 1, role: 'agent', content: 'yo' }])
		);
		vi.stubGlobal('fetch', fetchMock);

		const msgs = await listMessages('work', 't1');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/agent/threads/t1/messages');
		expect(msgs[0].role).toBe('agent');
	});

	it('renameThread POSTs to the rename endpoint', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ id: 't1', vault: 'work', title: 'New' }));
		vi.stubGlobal('fetch', fetchMock);

		const t = await renameThread('work', 't1', 'New');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/agent/threads/t1/rename');
		expect(t.title).toBe('New');
	});

	it('deleteThread DELETEs and tolerates a 404', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(new Response(null, { status: 404 }));
		vi.stubGlobal('fetch', fetchMock);

		await deleteThread('work', 'gone');
		expect(fetchMock.mock.calls[0][1]?.method).toBe('DELETE');
	});

	it('surfaces errors as ApiError carrying the status code', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(jsonResponse({ error: 'nope' }, 500));
		vi.stubGlobal('fetch', fetchMock);

		await expect(listThreads('work')).rejects.toMatchObject({ status: 500 });
	});

	it('explains an outdated server when createThread hits a missing route (404)', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(new Response('Not Found', { status: 404 }));
		vi.stubGlobal('fetch', fetchMock);

		await expect(createThread('work', 'Plan')).rejects.toMatchObject({
			status: 404,
			code: 'transcript_api_unavailable',
			message: expect.stringContaining('too old')
		});
	});

	it('explains an outdated server when listThreads hits a missing route (404)', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(new Response('Not Found', { status: 404 }));
		vi.stubGlobal('fetch', fetchMock);

		await expect(listThreads('work')).rejects.toMatchObject({
			status: 404,
			code: 'transcript_api_unavailable',
			message: expect.stringContaining('too old')
		});
	});
});
