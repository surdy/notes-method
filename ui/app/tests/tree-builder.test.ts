import assert from 'node:assert/strict';
import test from 'node:test';

import type { NoteSummary } from '../src/lib/api/notes.ts';
import { buildTree, sortTree } from '../src/lib/tree-builder.ts';

function note(path: string, title = path.split('/').at(-1)?.replace(/\.md$/, '') ?? path): NoteSummary {
	return {
		path,
		title,
		type: 'note',
		archived: false
	};
}

test('buildTree returns an empty root for no notes', () => {
	assert.deepEqual(buildTree([]), {
		name: '',
		path: '',
		children: [],
		notes: []
	});
});

test('buildTree keeps flat notes at the root and sorts them alphabetically', () => {
	assert.deepEqual(buildTree([note('Notes/beta.md', 'Beta'), note('Notes/alpha.md', 'Alpha')]), {
		name: '',
		path: '',
		children: [
			{
				name: 'Notes',
				path: 'Notes',
				children: [],
				notes: [note('Notes/alpha.md', 'Alpha'), note('Notes/beta.md', 'Beta')]
			}
		],
		notes: []
	});
});

test('buildTree nests notes under each folder segment', () => {
	assert.deepEqual(
		buildTree([
			note('Projects/Active/roadmap.md', 'Roadmap'),
			note('Projects/Archive/retrospective.md', 'Retrospective'),
			note('Inbox.md', 'Inbox')
		]),
		{
			name: '',
			path: '',
			children: [
				{
					name: 'Projects',
					path: 'Projects',
					children: [
						{
							name: 'Active',
							path: 'Projects/Active',
							children: [],
							notes: [note('Projects/Active/roadmap.md', 'Roadmap')]
						},
						{
							name: 'Archive',
							path: 'Projects/Archive',
							children: [],
							notes: [note('Projects/Archive/retrospective.md', 'Retrospective')]
						}
					],
					notes: []
				}
			],
			notes: [note('Inbox.md', 'Inbox')]
		}
	);
});

test('sortTree orders folders before notes alphabetically without mutating the input', () => {
	const original = {
		name: '',
		path: '',
		children: [
			{
				name: 'zeta',
				path: 'zeta',
				children: [],
				notes: [note('zeta/two.md', 'Two'), note('zeta/one.md', 'One')]
			},
			{
				name: 'alpha',
				path: 'alpha',
				children: [],
				notes: [note('alpha/beta.md', 'Beta'), note('alpha/alpha.md', 'Alpha')]
			}
		],
		notes: [note('z.md', 'Zed'), note('a.md', 'Aye')]
	};

	const sorted = sortTree(original);

	assert.deepEqual(sorted.children.map((child) => child.name), ['alpha', 'zeta']);
	assert.deepEqual(sorted.notes.map((entry) => entry.path), ['a.md', 'z.md']);
	assert.deepEqual(sorted.children[0]?.notes.map((entry) => entry.path), ['alpha/alpha.md', 'alpha/beta.md']);
	assert.deepEqual(sorted.children[1]?.notes.map((entry) => entry.path), ['zeta/one.md', 'zeta/two.md']);
	assert.deepEqual(original.children.map((child) => child.name), ['zeta', 'alpha']);
	assert.deepEqual(original.notes.map((entry) => entry.path), ['z.md', 'a.md']);
});
