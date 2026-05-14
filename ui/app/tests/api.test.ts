import test from 'node:test';
import assert from 'node:assert/strict';

import { capture, routeApply } from '../src/lib/api/index.ts';
import {
	fetchDaemonStatus,
	fetchLogTail,
	reindexVault,
	restartDaemon
} from '../src/lib/api/status.ts';

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

test('fetchDaemonStatus reads daemon health from /api/status', async () => {
	let requestUrl = '';
	const payload = {
		version: '0.1.0',
		started_at: '2026-05-14T19:00:00Z',
		uptime_seconds: 3600,
		vaults: {
			work: { notes: 150, tasks: 42, search_indexed: true, watcher_active: true }
		},
		resources: { memory_rss_bytes: 52_428_800, sse_connections: 2 },
		watchers: { active: 1, total: 1 },
		indexes: { caches_ok: 1, search_ok: 1 }
	};
	const restoreFetch = installFetchStub(async (input) => {
		requestUrl = String(input);
		return new Response(JSON.stringify(payload), {
			status: 200,
			headers: { 'Content-Type': 'application/json' }
		});
	});

	try {
		const response = await fetchDaemonStatus();

		assert.equal(requestUrl, '/api/status');
		assert.deepEqual(response, payload);
	} finally {
		restoreFetch();
	}
});

test('restartDaemon posts to the admin restart endpoint', async () => {
	let requestUrl = '';
	let requestInit: RequestInit | undefined;
	const restoreFetch = installFetchStub(async (input, init) => {
		requestUrl = String(input);
		requestInit = init;
		return new Response(null, { status: 202 });
	});

	try {
		await restartDaemon();

		assert.equal(requestUrl, '/admin/restart');
		assert.equal(requestInit?.method, 'POST');
	} finally {
		restoreFetch();
	}
});

test('reindexVault posts the selected vault name to the reindex endpoint', async () => {
	let requestUrl = '';
	let requestInit: RequestInit | undefined;
	const restoreFetch = installFetchStub(async (input, init) => {
		requestUrl = String(input);
		requestInit = init;
		return new Response(null, { status: 202 });
	});

	try {
		await reindexVault('work vault');

		assert.equal(requestUrl, '/api/app/vaults/work%20vault/reindex');
		assert.equal(requestInit?.method, 'POST');
	} finally {
		restoreFetch();
	}
});

test('fetchLogTail reads plain text daemon logs', async () => {
	let requestUrl = '';
	const restoreFetch = installFetchStub(async (input) => {
		requestUrl = String(input);
		return new Response('log line 1\nlog line 2', {
			status: 200,
			headers: { 'Content-Type': 'text/plain' }
		});
	});

	try {
		const response = await fetchLogTail(200);

		assert.equal(requestUrl, '/admin/logs?tail=200');
		assert.equal(response, 'log line 1\nlog line 2');
	} finally {
		restoreFetch();
	}
});
