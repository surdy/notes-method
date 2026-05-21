import { describe, it, expect } from 'vitest';
import { firstH1MatchesTitle, stripFirstH1IfMatchesTitle } from './duplicate-h1';

describe('firstH1MatchesTitle', () => {
	it('returns true when the document starts with an H1 matching the title', () => {
		const html = '<h1>My Note</h1><p>body</p>';
		expect(firstH1MatchesTitle(html, 'My Note')).toBe(true);
	});

	it('matches case-insensitively', () => {
		expect(firstH1MatchesTitle('<h1>My Note</h1>', 'my note')).toBe(true);
		expect(firstH1MatchesTitle('<h1>MY NOTE</h1>', 'My Note')).toBe(true);
	});

	it('trims whitespace in both H1 text and title', () => {
		expect(firstH1MatchesTitle('<h1>  My Note  </h1>', 'My Note')).toBe(true);
		expect(firstH1MatchesTitle('<h1>My Note</h1>', '  My Note  ')).toBe(true);
	});

	it('strips inline formatting from the H1 text before comparing', () => {
		expect(firstH1MatchesTitle('<h1><strong>My Note</strong></h1>', 'My Note')).toBe(true);
		expect(firstH1MatchesTitle('<h1><em>My</em> <strong>Note</strong></h1>', 'My Note')).toBe(
			true
		);
	});

	it('returns false when the first block is not an H1', () => {
		expect(firstH1MatchesTitle('<p>intro</p><h1>My Note</h1>', 'My Note')).toBe(false);
		expect(firstH1MatchesTitle('<h2>My Note</h2>', 'My Note')).toBe(false);
	});

	it('returns false when the H1 text differs from the title', () => {
		expect(firstH1MatchesTitle('<h1>Different</h1>', 'My Note')).toBe(false);
	});

	it('returns false for empty input', () => {
		expect(firstH1MatchesTitle('', 'My Note')).toBe(false);
		expect(firstH1MatchesTitle('<h1>My Note</h1>', '')).toBe(false);
	});

	it('ignores leading whitespace and newlines before the H1', () => {
		expect(firstH1MatchesTitle('\n  <h1>My Note</h1>', 'My Note')).toBe(true);
	});

	it('handles H1 with id attribute (anchor)', () => {
		expect(firstH1MatchesTitle('<h1 id="my-note">My Note</h1>', 'My Note')).toBe(true);
	});
});

describe('stripFirstH1IfMatchesTitle', () => {
	it('removes the leading H1 when it matches', () => {
		const html = '<h1>My Note</h1><p>body</p>';
		expect(stripFirstH1IfMatchesTitle(html, 'My Note')).toBe('<p>body</p>');
	});

	it('preserves leading whitespace structure when stripping', () => {
		const html = '<h1>My Note</h1>\n<p>body</p>';
		const result = stripFirstH1IfMatchesTitle(html, 'My Note');
		expect(result).toContain('<p>body</p>');
		expect(result).not.toContain('<h1');
	});

	it('returns html unchanged when H1 does not match', () => {
		const html = '<h1>Different</h1><p>body</p>';
		expect(stripFirstH1IfMatchesTitle(html, 'My Note')).toBe(html);
	});

	it('returns html unchanged when first block is not H1', () => {
		const html = '<p>intro</p><h1>My Note</h1>';
		expect(stripFirstH1IfMatchesTitle(html, 'My Note')).toBe(html);
	});

	it('handles H1 with attributes', () => {
		const html = '<h1 id="my-note" class="foo">My Note</h1><p>body</p>';
		expect(stripFirstH1IfMatchesTitle(html, 'My Note')).toBe('<p>body</p>');
	});

	it('only strips the first matching H1, not subsequent ones', () => {
		const html = '<h1>My Note</h1><p>body</p><h1>My Note</h1>';
		expect(stripFirstH1IfMatchesTitle(html, 'My Note')).toBe('<p>body</p><h1>My Note</h1>');
	});
});
