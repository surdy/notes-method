import { describe, expect, it } from 'vitest';
import type { NoteSummary } from './api';
import { buildTree } from './tree-builder';
import {
	filterTree,
	nextTypeaheadIndex,
	noteLabel,
	treeNoteCount,
	wrapIndex
} from './sidebar-tree';

function note(path: string, title = ''): NoteSummary {
	return { path, title, tags: [] } as NoteSummary;
}

const tree = buildTree([
	note('Inbox/Alpha.md', 'Alpha'),
	note('Inbox/Beta.md', 'Beta'),
	note('Projects/Website/Home.md', 'Home'),
	note('Projects/Website/About.md', 'About'),
	note('Readme.md', 'Readme')
]);

describe('noteLabel', () => {
	it('prefers title, falling back to filename without extension', () => {
		expect(noteLabel(note('a/b/Thing.md', 'My Title'))).toBe('My Title');
		expect(noteLabel(note('a/b/Thing.md'))).toBe('Thing');
	});
});

describe('filterTree', () => {
	it('returns the tree unchanged for an empty query', () => {
		expect(filterTree(tree, '   ')).toBe(tree);
	});

	it('keeps only notes matching the query and their ancestor folders', () => {
		const filtered = filterTree(tree, 'home');
		expect(filtered).not.toBeNull();
		expect(treeNoteCount(filtered)).toBe(1);
		const website = filtered!.children[0].children[0];
		expect(website.name).toBe('Website');
		expect(website.notes.map((n) => n.title)).toEqual(['Home']);
	});

	it('keeps the whole subtree when a folder name matches', () => {
		const filtered = filterTree(tree, 'website');
		expect(treeNoteCount(filtered)).toBe(2);
	});

	it('returns null when nothing matches', () => {
		expect(filterTree(tree, 'zzznope')).toBeNull();
	});

	it('is case-insensitive and matches substrings', () => {
		expect(treeNoteCount(filterTree(tree, 'ALPH'))).toBe(1);
	});
});

describe('treeNoteCount', () => {
	it('counts all notes across the tree and returns 0 for null', () => {
		expect(treeNoteCount(tree)).toBe(5);
		expect(treeNoteCount(null)).toBe(0);
	});
});

describe('wrapIndex', () => {
	it('wraps at both ends', () => {
		expect(wrapIndex(3, 0, -1)).toBe(2);
		expect(wrapIndex(3, 2, 1)).toBe(0);
		expect(wrapIndex(3, 1, 1)).toBe(2);
	});

	it('guards against an empty list', () => {
		expect(wrapIndex(0, 0, 1)).toBe(0);
	});
});

describe('nextTypeaheadIndex', () => {
	const labels = ['Alpha', 'Beta', 'About', 'beacon'];

	it('finds the next label starting with the buffer, wrapping forward', () => {
		expect(nextTypeaheadIndex(labels, 0, 'be')).toBe(1);
		expect(nextTypeaheadIndex(labels, 1, 'be')).toBe(3);
		expect(nextTypeaheadIndex(labels, 3, 'be')).toBe(1);
	});

	it('is case-insensitive and matches prefixes only', () => {
		expect(nextTypeaheadIndex(labels, -1, 'a')).toBe(0);
		expect(nextTypeaheadIndex(labels, -1, 'bout')).toBeNull();
	});

	it('returns null for empty buffer or empty list', () => {
		expect(nextTypeaheadIndex(labels, 0, '  ')).toBeNull();
		expect(nextTypeaheadIndex([], 0, 'a')).toBeNull();
	});
});
