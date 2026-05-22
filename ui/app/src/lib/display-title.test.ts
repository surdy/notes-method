import { describe, it, expect } from 'vitest';
import { displayTitleFor } from './display-title';

describe('displayTitleFor', () => {
	it('strips .md extension from filename', () => {
		expect(displayTitleFor({ path: 'My Note.md' })).toBe('My Note');
	});

	it('uses basename from nested path', () => {
		expect(displayTitleFor({ path: 'Customers/Acme/Account Info.md' })).toBe('Account Info');
	});

	it('returns frontmatter title when present and non-empty', () => {
		expect(
			displayTitleFor({ path: 'daily/2026-05-20.md', frontmatter: { title: 'Today' } })
		).toBe('Today');
	});

	it('trims whitespace in frontmatter title', () => {
		expect(displayTitleFor({ path: 'a.md', frontmatter: { title: '  Padded  ' } })).toBe('Padded');
	});

	it('falls back to filename when frontmatter title is empty string', () => {
		expect(displayTitleFor({ path: 'My Note.md', frontmatter: { title: '' } })).toBe('My Note');
	});

	it('falls back to filename when frontmatter title is whitespace only', () => {
		expect(displayTitleFor({ path: 'My Note.md', frontmatter: { title: '   ' } })).toBe('My Note');
	});

	it('falls back to filename when frontmatter title is not a string', () => {
		expect(displayTitleFor({ path: 'My Note.md', frontmatter: { title: 42 } })).toBe('My Note');
		expect(displayTitleFor({ path: 'My Note.md', frontmatter: { title: ['a'] } })).toBe('My Note');
		expect(displayTitleFor({ path: 'My Note.md', frontmatter: { title: null } })).toBe('My Note');
	});

	it('handles paths without .md extension', () => {
		expect(displayTitleFor({ path: 'README' })).toBe('README');
	});

	it('handles empty path defensively', () => {
		expect(displayTitleFor({ path: '' })).toBe('Untitled');
	});

	it('handles path that is just a slash', () => {
		expect(displayTitleFor({ path: '/' })).toBe('Untitled');
	});

	it('strips .md case-insensitively', () => {
		expect(displayTitleFor({ path: 'Note.MD' })).toBe('Note');
		expect(displayTitleFor({ path: 'Note.Md' })).toBe('Note');
	});

	it('accepts null frontmatter', () => {
		expect(displayTitleFor({ path: 'Note.md', frontmatter: null })).toBe('Note');
	});

	it('accepts undefined frontmatter', () => {
		expect(displayTitleFor({ path: 'Note.md', frontmatter: undefined })).toBe('Note');
	});
});
