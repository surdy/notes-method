import test from 'node:test';
import assert from 'node:assert/strict';

import { capture, routeApply } from '../src/lib/api.ts';

function installFetchStub(
	handler: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> | Response
) {
	const originalFetch = globalThis.fetch;
	Object.defineProperty(globalThis, 'fetch', {
		value: handler,
		configurable: true,
		writable: true
	});

	return () => {
		Object.defineProperty(globalThis, 'fetch', {
			value: originalFetch,
			configurable: true,
			writable: true
		});
	};
}

test('capture posts to the capture endpoint', async () => {
	let requestUrl = '';
	let requestInit: RequestInit | undefined;
	const restoreFetch = installFetchStub(async (input, init) => {
		requestUrl = String(input);
		requestInit = init;
		return new Response(JSON.stringify({ path: 'Inbox/2026-05-13.md', hash: 'hash-1' }), {
			status: 200,
			headers: { 'Content-Type': 'application/json' }
		});
	});

	try {
		const response = await capture('work', 'Remember this', 'Idea');

		assert.equal(requestUrl, '/api/v/work/capture');
		assert.equal(requestInit?.method, 'POST');
		assert.deepEqual(JSON.parse(String(requestInit?.body)), {
			text: 'Remember this',
			title: 'Idea'
		});
		assert.deepEqual(response, { path: 'Inbox/2026-05-13.md', hash: 'hash-1' });
	} finally {
		restoreFetch();
	}
});

test('routeApply posts explicit paths only', async () => {
	let requestUrl = '';
	let requestInit: RequestInit | undefined;
	const restoreFetch = installFetchStub(async (input, init) => {
		requestUrl = String(input);
		requestInit = init;
		return new Response(JSON.stringify({ routed: 1, results: [{ from: 'Inbox/note.md', to: 'Archive/note.md' }] }), {
			status: 200,
			headers: { 'Content-Type': 'application/json' }
		});
	});

	try {
		const response = await routeApply('work', ['Inbox/note.md']);

		assert.equal(requestUrl, '/api/v/work/route/apply');
		assert.equal(requestInit?.method, 'POST');
		assert.deepEqual(JSON.parse(String(requestInit?.body)), {
			paths: ['Inbox/note.md']
		});
		assert.deepEqual(response, {
			routed: 1,
			results: [{ from: 'Inbox/note.md', to: 'Archive/note.md' }]
		});
	} finally {
		restoreFetch();
	}
});
