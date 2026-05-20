import { describe, expect, it } from 'vitest';

import type { NoteSummary } from './api';
import { buildTree } from './tree-builder.ts';

function note(path: string): NoteSummary {
	return {
		path,
		title: '',
		type: '',
		archived: false
	};
}

describe('buildTree folder notes', () => {
	it('attaches exact same-name markdown files to their folder and hides the duplicate child note', () => {
		const tree = buildTree([
			note('Customers/Acme/Acme.md'),
			note('Customers/Acme/Contacts/Jane Doe.md'),
			note('Customers/Acme/Notes.md')
		]);

		const customers = tree.children.find((child) => child.name === 'Customers');
		const acme = customers?.children.find((child) => child.name === 'Acme');

		expect(acme?.folderNote?.path).toBe('Customers/Acme/Acme.md');
		expect(acme?.notes.map((entry) => entry.path)).toEqual(['Customers/Acme/Notes.md']);
	});

	it('does not attach case-mismatched notes or notes in dot-prefixed folders', () => {
		const tree = buildTree([
			note('Customers/Acme/acme.md'),
			note('.notesmith/prompts/prompts.md'),
			note('.notesmith/prompts/Other.md')
		]);

		const customers = tree.children.find((child) => child.name === 'Customers');
		const acme = customers?.children.find((child) => child.name === 'Acme');
		const dotNotesmith = tree.children.find((child) => child.name === '.notesmith');
		const prompts = dotNotesmith?.children.find((child) => child.name === 'prompts');

		expect(acme?.folderNote).toBeUndefined();
		expect(acme?.notes.map((entry) => entry.path)).toEqual(['Customers/Acme/acme.md']);
		expect(prompts?.folderNote).toBeUndefined();
		expect(prompts?.notes.map((entry) => entry.path)).toEqual([
			'.notesmith/prompts/Other.md',
			'.notesmith/prompts/prompts.md'
		]);
	});
});
