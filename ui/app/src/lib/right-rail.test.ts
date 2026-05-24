import { describe, expect, it } from 'vitest';

import type { NoteSummary } from './api';
import { buildRailMetadata } from './right-rail.ts';

const baseNote: NoteSummary = {
	path: 'General/Prototype Notes.md',
	title: 'Prototype Notes',
	tags: ['research'],
	frontmatter: null
};

describe('buildRailMetadata', () => {
	it('shows all public frontmatter keys and tags as generic metadata', () => {
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
			date: '2025-01-16',
			tags: ['research', 'prototype'],
			stage: 'discovery',
			score: 3
		});
	});

	it('returns null when only private frontmatter keys are present', () => {
		expect(
			buildRailMetadata(
				{ ...baseNote, tags: [] },
				{ _icon: '🔬', _system: 'internal' }
			)
		).toBeNull();
	});

	it('uses tags from note when frontmatter has none', () => {
		expect(
			buildRailMetadata(
				{ ...baseNote, tags: ['work', 'urgent'] },
				{ title: 'Something' }
			)
		).toEqual({
			title: 'Something',
			tags: ['work', 'urgent']
		});
	});
});
