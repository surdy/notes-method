import { describe, it, expect } from 'vitest';
import type { NoteSummary } from '$lib/api';
import { resolveWikilink, stripAnchor, splitWikilinkTarget } from './wikilink-resolver.ts';

function note(path: string, title = ''): NoteSummary {
	return { path, title, tags: [] };
}

describe('stripAnchor', () => {
	it('drops a trailing heading anchor and trims', () => {
		expect(stripAnchor('Roadmap#Q1 Goals')).toBe('Roadmap');
		expect(stripAnchor('  Roadmap  ')).toBe('Roadmap');
		expect(stripAnchor('#only-anchor')).toBe('');
	});
});

describe('splitWikilinkTarget', () => {
	it('splits folder from title', () => {
		expect(splitWikilinkTarget('Projects/Roadmap')).toEqual({ folder: 'Projects', title: 'Roadmap' });
	});
	it('returns just a title when there is no folder', () => {
		expect(splitWikilinkTarget('Roadmap')).toEqual({ title: 'Roadmap' });
	});
	it('strips a .md extension', () => {
		expect(splitWikilinkTarget('Projects/Roadmap.md')).toEqual({
			folder: 'Projects',
			title: 'Roadmap'
		});
	});
});

describe('resolveWikilink — confident matches', () => {
	const notes = [
		note('Inbox/Roadmap.md', 'Roadmap'),
		note('Projects/Planning.md', 'Planning'),
		note('Areas/Health.md', 'Health')
	];

	it('matches an exact path', () => {
		expect(resolveWikilink('Inbox/Roadmap.md', notes).path).toBe('Inbox/Roadmap.md');
	});
	it('matches path + .md', () => {
		expect(resolveWikilink('Inbox/Roadmap', notes).path).toBe('Inbox/Roadmap.md');
	});
	it('matches strip-.md equality', () => {
		const n = [note('Roadmap.md', '')];
		expect(resolveWikilink('Roadmap', n).path).toBe('Roadmap.md');
	});
	it('matches an exact title', () => {
		expect(resolveWikilink('Planning', notes).path).toBe('Projects/Planning.md');
	});
	it('matches a basename', () => {
		expect(resolveWikilink('Health', [note('Areas/Health.md', '')]).path).toBe('Areas/Health.md');
	});
	it('ignores an #anchor when matching', () => {
		expect(resolveWikilink('Planning#Section', notes).path).toBe('Projects/Planning.md');
	});
	it('treats a unique case-insensitive basename as confident', () => {
		expect(resolveWikilink('roadmap', notes).path).toBe('Inbox/Roadmap.md');
	});
});

describe('resolveWikilink — ambiguous', () => {
	it('returns candidates when multiple notes share a name', () => {
		const notes = [note('Work/Notes.md', 'Notes'), note('Personal/Notes.md', 'Notes')];
		const res = resolveWikilink('Notes', notes);
		expect(res.path).toBeNull();
		expect(res.candidates.map((c) => c.path)).toEqual(['Work/Notes.md', 'Personal/Notes.md']);
	});
});

describe('resolveWikilink — missing', () => {
	const notes = [note('Inbox/Roadmap.md', 'Roadmap'), note('Projects/Planning.md', 'Planning')];

	it('returns no path and no candidates for a truly dead link', () => {
		const res = resolveWikilink('Nonexistent Note', notes);
		expect(res.path).toBeNull();
		expect(res.candidates).toEqual([]);
		expect(res.name).toBe('Nonexistent Note');
	});

	it('offers fuzzy substring candidates instead of silently navigating', () => {
		const res = resolveWikilink('Plan', notes);
		expect(res.path).toBeNull();
		expect(res.candidates.map((c) => c.path)).toContain('Projects/Planning.md');
	});

	it('caps fuzzy candidates at 8', () => {
		const many = Array.from({ length: 20 }, (_, i) => note(`Notes/Item ${i}.md`, `Item ${i}`));
		const res = resolveWikilink('Item', many);
		expect(res.path).toBeNull();
		expect(res.candidates.length).toBe(8);
	});

	it('returns an empty resolution for an empty target', () => {
		expect(resolveWikilink('', notes)).toEqual({ path: null, candidates: [], name: '' });
	});
});
