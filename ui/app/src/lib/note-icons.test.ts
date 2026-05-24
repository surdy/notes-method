import { describe, expect, it } from 'vitest';

import type { NoteSummary } from './api';
import { noteIcon } from './note-icons.ts';

const baseNote: NoteSummary = {
	path: 'General/Prototype Notes.md',
	title: 'Prototype Notes',
	tags: [],
	frontmatter: null
};

describe('noteIcon', () => {
	it('prefers an _icon frontmatter override', () => {
		expect(noteIcon({ ...baseNote, frontmatter: { _icon: '🔬' } })).toBe('🔬');
	});

	it('returns generic document icon when no _icon set', () => {
		expect(noteIcon(baseNote)).toBe('📄');
	});
});
