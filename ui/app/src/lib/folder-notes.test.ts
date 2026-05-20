import { describe, expect, it, vi } from 'vitest';

import type { NoteSummary, WriteNoteResponse } from './api';
import {
	createOrOpenFolderNote,
	folderNotePath,
	isFolderNoteSelected,
	listFolderPickerItems
} from './folder-notes.ts';
import { buildTree } from './tree-builder.ts';

function note(path: string): NoteSummary {
	return {
		path,
		title: '',
		type: '',
		archived: false
	};
}

describe('folder notes', () => {
	it('derives exact same-name markdown folder-note paths', () => {
		expect(folderNotePath('Customers/Acme')).toBe('Customers/Acme/Acme.md');
		expect(folderNotePath('')).toBeNull();
		expect(folderNotePath('.notesmith/prompts')).toBeNull();
	});

	it('lists non-dot folders as command-palette picker items', () => {
		const tree = buildTree([
			note('Customers/Acme/Acme.md'),
			note('Customers/Acme/Contacts/Jane Doe.md'),
			note('.notesmith/prompts/daily.md')
		]);

		expect(listFolderPickerItems(tree)).toEqual([
			{ id: 'Customers', label: 'Customers' },
			{ id: 'Customers/Acme', label: 'Acme', description: 'Customers/Acme' },
			{
				id: 'Customers/Acme/Contacts',
				label: 'Contacts',
				description: 'Customers/Acme/Contacts'
			}
		]);
	});

	it('opens an existing folder note instead of creating over it', async () => {
		const createNote = vi.fn<() => Promise<WriteNoteResponse>>();

		const result = await createOrOpenFolderNote({
			vault: 'work',
			folderPath: 'Customers/Acme',
			notes: [note('Customers/Acme/Acme.md')],
			createNote
		});

		expect(result).toEqual({ path: 'Customers/Acme/Acme.md', created: false });
		expect(createNote).not.toHaveBeenCalled();
	});

	it('creates a missing folder note with an H1 matching the folder name', async () => {
		const createNote = vi
			.fn<() => Promise<WriteNoteResponse>>()
			.mockResolvedValue({ path: 'Customers/Acme/Acme.md', hash: 'hash-a' });

		const result = await createOrOpenFolderNote({
			vault: 'work',
			folderPath: 'Customers/Acme',
			notes: [],
			createNote
		});

		expect(createNote).toHaveBeenCalledWith('work', 'Acme', '# Acme\n', 'Customers/Acme');
		expect(result).toEqual({ path: 'Customers/Acme/Acme.md', created: true });
	});

	it('selects only the folder row represented by the active folder note', () => {
		const tree = buildTree([
			note('Customers/Acme/Acme.md'),
			note('Customers/Acme/Contacts/Jane Doe.md')
		]);
		const customers = tree.children[0];
		const acme = customers.children[0];
		const contacts = acme.children[0];

		expect(isFolderNoteSelected(acme, 'Customers/Acme/Acme.md')).toBe(true);
		expect(isFolderNoteSelected(customers, 'Customers/Acme/Acme.md')).toBe(false);
		expect(isFolderNoteSelected(contacts, 'Customers/Acme/Contacts/Jane Doe.md')).toBe(false);
	});
});
