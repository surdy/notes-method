import { describe, expect, it } from 'vitest';

import {
	extractNoteSources,
	groundingKind,
	mergeSources,
	sortSources,
	type NoteSource
} from './sources.ts';

/** A `vault_search` result as serialized by the Rust `HybridHit` (snake_case). */
function searchResult(): string {
	return JSON.stringify([
		{
			path: 'people/Acme.md',
			title: 'Acme Corp',
			snippet: 'Acme is a customer…',
			score: 0.031,
			lexical_rank: 2,
			semantic_rank: 1,
			char_start: 10,
			char_end: 42
		},
		{
			path: 'notes/pricing.md',
			title: 'Pricing',
			snippet: 'Pricing tiers…',
			score: 0.017,
			lexical_rank: 1,
			semantic_rank: null,
			char_start: null,
			char_end: null
		}
	]);
}

/** A `get_note` result as serialized by the Rust ops layer. */
function noteResult(): string {
	return JSON.stringify({
		path: 'people/Acme.md',
		content: '# Acme\n\nBody text',
		hash: 'abc123',
		frontmatter: { title: 'Acme Corp', type: 'person' }
	});
}

/** A web-search-style result: URL-bearing, no note path. */
function webResult(): string {
	return JSON.stringify([
		{ title: 'Result', url: 'https://example.com/a', snippet: 'x' },
		{ title: 'Result 2', url: 'https://example.com/b', snippet: 'y' }
	]);
}

describe('groundingKind', () => {
	it('recognizes namespaced vault_search and search_notes as search', () => {
		expect(groundingKind('vault_search')).toBe('search');
		expect(groundingKind('notesmith-people-vault_search')).toBe('search');
		expect(groundingKind('search_notes')).toBe('search');
	});

	it('recognizes namespaced get_note as note', () => {
		expect(groundingKind('get_note')).toBe('note');
		expect(groundingKind('notesmith-people-get_note')).toBe('note');
	});

	it('returns null for non-grounding and web tools', () => {
		expect(groundingKind('list_notes')).toBeNull();
		expect(groundingKind('vault_stats')).toBeNull();
		expect(groundingKind('web_search')).toBeNull();
		expect(groundingKind('web_fetch')).toBeNull();
	});
});

describe('extractNoteSources', () => {
	it('extracts note sources from a vault_search result', () => {
		const sources = extractNoteSources('vault_search', searchResult());
		expect(sources).toHaveLength(2);
		expect(sources[0]).toEqual<NoteSource>({
			path: 'people/Acme.md',
			title: 'Acme Corp',
			score: 0.031,
			snippet: 'Acme is a customer…',
			lexicalRank: 2,
			semanticRank: 1
		});
		expect(sources[1].semanticRank).toBeNull();
	});

	it('extracts a single source from a get_note result, titled from frontmatter', () => {
		const sources = extractNoteSources('notesmith-people-get_note', noteResult());
		expect(sources).toEqual<NoteSource[]>([
			{
				path: 'people/Acme.md',
				title: 'Acme Corp',
				score: null,
				snippet: null,
				lexicalRank: null,
				semanticRank: null
			}
		]);
	});

	it('excludes web-search results (web-only path yields no note sources)', () => {
		expect(extractNoteSources('web_search', webResult())).toEqual([]);
	});

	it('does not treat list_notes / vault_stats output as sources', () => {
		const listNotes = JSON.stringify([{ path: 'a.md', title: 'A' }]);
		expect(extractNoteSources('list_notes', listNotes)).toEqual([]);
		const stats = JSON.stringify({ totals: { notes: 3 }, tags: [] });
		expect(extractNoteSources('vault_stats', stats)).toEqual([]);
	});

	it('falls back to search-hit shape when the tool name is unrecognized', () => {
		// An agent may title the MCP call "Search vault" rather than the raw id.
		const sources = extractNoteSources('Search vault', searchResult());
		expect(sources.map((s) => s.path)).toEqual(['people/Acme.md', 'notes/pricing.md']);
	});

	it('ignores errors and malformed content without throwing', () => {
		expect(extractNoteSources('vault_search', searchResult(), true)).toEqual([]);
		expect(extractNoteSources('vault_search', 'not json')).toEqual([]);
		expect(extractNoteSources('vault_search', '')).toEqual([]);
	});
});

describe('mergeSources and sortSources', () => {
	it('de-duplicates by path, keeping the richest fields and best score', () => {
		const first: NoteSource[] = [
			{ path: 'a.md', title: null, score: 0.01, snippet: null, lexicalRank: 3, semanticRank: null }
		];
		const second: NoteSource[] = [
			{
				path: 'a.md',
				title: 'A',
				score: 0.05,
				snippet: 'hi',
				lexicalRank: null,
				semanticRank: 1
			}
		];
		const merged = mergeSources(first, second);
		expect(merged).toHaveLength(1);
		expect(merged[0]).toEqual<NoteSource>({
			path: 'a.md',
			title: 'A',
			score: 0.05,
			snippet: 'hi',
			lexicalRank: 3,
			semanticRank: 1
		});
	});

	it('sorts by score desc, scored before unscored, then by path', () => {
		const sources: NoteSource[] = [
			{ path: 'z.md', title: null, score: null, snippet: null, lexicalRank: null, semanticRank: null },
			{ path: 'b.md', title: null, score: 0.1, snippet: null, lexicalRank: null, semanticRank: null },
			{ path: 'a.md', title: null, score: 0.3, snippet: null, lexicalRank: null, semanticRank: null }
		];
		expect(sortSources(sources).map((s) => s.path)).toEqual(['a.md', 'b.md', 'z.md']);
	});
});
