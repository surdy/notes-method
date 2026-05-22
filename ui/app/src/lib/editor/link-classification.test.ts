import { describe, expect, it } from 'vitest';

import { isExternalLinkUrl } from './link-classification.ts';

describe('isExternalLinkUrl', () => {
	it.each([
		['https://example.com', true],
		['http://example.com/path', true],
		['HTTPS://Example.com', true],
		['mailto:alice@example.com', true],
		['tel:+15551234567', true],
		['ftp://files.example.com', true],
		['//cdn.example.com/asset.js', true],
		['obsidian://open?vault=foo', true],
		['notesmith://open?vault=foo', true]
	])('treats %s as external', (url, expected) => {
		expect(isExternalLinkUrl(url)).toBe(expected);
	});

	it.each([
		['some-note.md'],
		['folder/other.md'],
		['./relative.md'],
		['../up.md'],
		['#heading-anchor'],
		['Customers/Acme'],
		['']
	])('treats %s as internal', (url) => {
		expect(isExternalLinkUrl(url)).toBe(false);
	});
});
