import { describe, expect, it } from 'vitest';

import type { NoteSummary } from './api';
import { noteIcon } from './note-icons.ts';

const baseNote: NoteSummary = {
	path: 'General/Prototype Notes.md',
	title: 'Prototype Notes',
	type: 'note',
	archived: false,
	frontmatter: null
};

describe('noteIcon', () => {
	it('prefers an _icon frontmatter override', () => {
		expect(noteIcon({ ...baseNote, type: 'meeting', frontmatter: { _icon: '🔬' } })).toBe('🔬');
	});

	it('falls back to type-based icons and a generic document icon', () => {
		expect(noteIcon(baseNote)).toBe('📝');
		expect(noteIcon({ ...baseNote, type: 'daily' })).toBe('📅');
		expect(noteIcon({ ...baseNote, type: 'mystery' })).toBe('📄');
	});
});
