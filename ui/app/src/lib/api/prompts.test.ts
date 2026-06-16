import { afterEach, describe, expect, it, vi } from 'vitest';

import { getPrompt, listPrompts, resolvePromptText } from './prompts.ts';

function jsonResponse(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

const samplePrompts = [
	{
		name: 'summarize',
		description: 'Concise summary of the current note.',
		body: 'Provide a concise summary of the current note.',
		source: 'default'
	},
	{
		name: 'standup',
		description: 'Daily standup',
		body: 'Draft my standup update.',
		source: 'vault'
	}
];

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('prompts api client', () => {
	it('listPrompts GETs the vault-scoped endpoint and unwraps `prompts`', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ prompts: samplePrompts }));
		vi.stubGlobal('fetch', fetchMock);

		const prompts = await listPrompts('work');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/work/prompts');
		expect(prompts).toHaveLength(2);
		expect(prompts[0].name).toBe('summarize');
		expect(prompts[1].source).toBe('vault');
	});

	it('listPrompts encodes the vault name', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(jsonResponse({ prompts: [] }));
		vi.stubGlobal('fetch', fetchMock);

		await listPrompts('my vault/2');
		expect(String(fetchMock.mock.calls[0][0])).toBe('/api/v/my%20vault%2F2/prompts');
	});

	it('listPrompts tolerates a response without a `prompts` field', async () => {
		const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(jsonResponse({}));
		vi.stubGlobal('fetch', fetchMock);

		const prompts = await listPrompts('work');
		expect(prompts).toEqual([]);
	});

	it('listPrompts throws an ApiError on a non-ok response', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ error: 'vault_not_found' }, 404));
		vi.stubGlobal('fetch', fetchMock);

		await expect(listPrompts('work')).rejects.toMatchObject({
			status: 404,
			code: 'vault_not_found'
		});
	});

	it('getPrompt resolves a single prompt by name', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ prompts: samplePrompts }));
		vi.stubGlobal('fetch', fetchMock);

		const prompt = await getPrompt('work', 'standup');
		expect(prompt?.body).toBe('Draft my standup update.');
	});

	it('getPrompt returns null for an unknown name', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ prompts: samplePrompts }));
		vi.stubGlobal('fetch', fetchMock);

		const prompt = await getPrompt('work', 'nope');
		expect(prompt).toBeNull();
	});

	it('resolvePromptText returns the verbatim body to send to the agent', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ prompts: samplePrompts }));
		vi.stubGlobal('fetch', fetchMock);

		const text = await resolvePromptText('work', 'summarize');
		expect(text).toBe('Provide a concise summary of the current note.');
	});

	it('resolvePromptText returns null for an unknown prompt', async () => {
		const fetchMock = vi
			.fn<typeof fetch>()
			.mockResolvedValue(jsonResponse({ prompts: samplePrompts }));
		vi.stubGlobal('fetch', fetchMock);

		const text = await resolvePromptText('work', 'missing');
		expect(text).toBeNull();
	});
});
