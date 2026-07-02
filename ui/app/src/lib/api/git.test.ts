import { afterEach, describe, expect, it, vi } from 'vitest';

import { changedFileCount, commitCheckpoint, getGitStatus, type GitStatus } from './git.ts';

function jsonResponse(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('changedFileCount', () => {
	it('counts distinct files across changed, staged, and untracked', () => {
		const status: GitStatus = {
			changed: ['a.md', 'b.md'],
			staged: ['b.md'],
			untracked: ['c.md'],
			clean: false
		};
		expect(changedFileCount(status)).toBe(3);
	});

	it('returns 0 for a clean tree', () => {
		expect(changedFileCount({ changed: [], staged: [], untracked: [], clean: true })).toBe(0);
	});
});

describe('git api client', () => {
	it('getGitStatus GETs the vault-scoped endpoint', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
			jsonResponse({ changed: ['x.md'], staged: [], untracked: [], clean: false })
		);
		vi.stubGlobal('fetch', fetchMock);

		const status = await getGitStatus('work');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/git/status');
		expect(status.changed).toEqual(['x.md']);
	});

	it('commitCheckpoint POSTs an empty body when no message', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ committed: true, sha: 'abc', files: ['x.md'] }));
		vi.stubGlobal('fetch', fetchMock);

		const result = await commitCheckpoint('work');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/git/commit');
		const init = fetchMock.mock.calls[0][1]!;
		expect(init.method).toBe('POST');
		expect(init.body).toBe('{}');
		expect(result.committed).toBe(true);
	});

	it('commitCheckpoint includes an explicit message', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ committed: true, sha: 'abc', files: [] }));
		vi.stubGlobal('fetch', fetchMock);

		await commitCheckpoint('work', 'manual checkpoint');
		expect(fetchMock.mock.calls[0][1]!.body).toBe(JSON.stringify({ message: 'manual checkpoint' }));
	});
});
