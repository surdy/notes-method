import { describe, expect, it } from 'vitest';

import { findActiveHeadingIndex, parseHeadings } from './headings.ts';

describe('parseHeadings', () => {
	it('extracts markdown headings with levels, text, and offsets', () => {
		const headings = parseHeadings(
			['# Title', 'Intro paragraph', '## Section', '### Nested heading  ', '##No space'].join('\n')
		);

		expect(headings).toEqual([
			{ level: 1, text: 'Title', from: 0 },
			{ level: 2, text: 'Section', from: 24 },
			{ level: 3, text: 'Nested heading', from: 35 }
		]);
	});
});

describe('findActiveHeadingIndex', () => {
	it('returns the nearest heading at or before the cursor position', () => {
		const headings = parseHeadings(['# Title', 'Body', '## Section', 'More body', '### Nested'].join('\n'));

		expect(findActiveHeadingIndex(headings, 0)).toBe(0);
		expect(findActiveHeadingIndex(headings, 10)).toBe(0);
		expect(findActiveHeadingIndex(headings, 13)).toBe(1);
		expect(findActiveHeadingIndex(headings, 34)).toBe(2);
	});

	it('returns -1 when the cursor is before the first heading', () => {
		expect(findActiveHeadingIndex([{ level: 2, text: 'Section', from: 10 }], 9)).toBe(-1);
	});
});
