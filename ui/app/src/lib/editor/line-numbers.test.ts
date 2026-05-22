import { describe, expect, it } from 'vitest';

import { createLineNumberExtensions } from './line-numbers.ts';

describe('createLineNumberExtensions', () => {
	it('includes gutter extensions when line numbers are enabled', () => {
		expect(createLineNumberExtensions(true)).toHaveLength(2);
	});

	it('omits gutter extensions when line numbers are disabled', () => {
		expect(createLineNumberExtensions(false)).toEqual([]);
	});
});
