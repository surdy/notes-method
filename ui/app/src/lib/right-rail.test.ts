import { describe, expect, it } from 'vitest';

import type { NoteSummary } from './api';
import { buildRailMetadata } from './right-rail.ts';

const baseNote: NoteSummary = {
	path: 'General/Prototype Notes.md',
	title: 'Prototype Notes',
	type: 'note',
	customer: '[[Acme Corp]]',
	date: '2025-01-15',
	archived: false,
	frontmatter: null
};

describe('buildRailMetadata', () => {
	it('keeps known fields first and includes additional public frontmatter keys', () => {
		expect(
			buildRailMetadata(baseNote, {
				date: '2025-01-16',
				tags: ['research', 'prototype'],
				stage: 'discovery',
				score: 3,
				_icon: '🔬',
				_internal: 'hidden',
				empty: ''
			})
		).toEqual({
			type: 'note',
			customer: '[[Acme Corp]]',
			date: '2025-01-16',
			tags: ['research', 'prototype'],
			stage: 'discovery',
			score: 3
		});
	});

	it('returns null when only private frontmatter keys are present', () => {
		expect(
			buildRailMetadata(
				{ ...baseNote, type: '', customer: undefined, date: undefined },
				{ _icon: '🔬', _system: 'internal' }
			)
		).toBeNull();
	});
});
