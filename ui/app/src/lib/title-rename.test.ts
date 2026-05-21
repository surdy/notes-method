import { describe, expect, it } from 'vitest';
import { computeStem, hasFrontmatterTitle, validateName } from './title-rename';

describe('computeStem', () => {
	it('returns the basename without extension', () => {
		expect(computeStem('Inbox/Foo.md')).toBe('Foo');
	});

	it('handles paths with no folder', () => {
		expect(computeStem('Foo.md')).toBe('Foo');
	});

	it('returns null for empty string', () => {
		expect(computeStem('')).toBeNull();
	});

	it('handles names without .md suffix', () => {
		expect(computeStem('Foo')).toBe('Foo');
	});

	it('strips .MD case-insensitively', () => {
		expect(computeStem('Foo.MD')).toBe('Foo');
	});

	it('returns null when the file is just an extension', () => {
		expect(computeStem('.md')).toBeNull();
	});
});

describe('validateName', () => {
	it('accepts a normal name', () => {
		expect(validateName('My Note')).toBeNull();
	});

	it('rejects empty name', () => {
		expect(validateName('')).toMatch(/empty/i);
	});

	it('rejects path separators', () => {
		expect(validateName('a/b')).toMatch(/invalid/i);
		expect(validateName('a\\b')).toMatch(/invalid/i);
	});

	it('rejects reserved chars', () => {
		for (const bad of ['a:b', 'a*b', 'a?b', 'a"b', 'a<b', 'a>b', 'a|b']) {
			expect(validateName(bad)).not.toBeNull();
		}
	});

	it('rejects . and ..', () => {
		expect(validateName('.')).not.toBeNull();
		expect(validateName('..')).not.toBeNull();
	});
});

describe('hasFrontmatterTitle', () => {
	it('returns false for null/undefined', () => {
		expect(hasFrontmatterTitle(null)).toBe(false);
		expect(hasFrontmatterTitle(undefined)).toBe(false);
	});

	it('returns false when title is missing', () => {
		expect(hasFrontmatterTitle({})).toBe(false);
	});

	it('returns false for empty/whitespace title', () => {
		expect(hasFrontmatterTitle({ title: '' })).toBe(false);
		expect(hasFrontmatterTitle({ title: '   ' })).toBe(false);
	});

	it('returns true for a non-empty string title', () => {
		expect(hasFrontmatterTitle({ title: 'My Title' })).toBe(true);
	});

	it('returns false for non-string title values', () => {
		expect(hasFrontmatterTitle({ title: 42 })).toBe(false);
		expect(hasFrontmatterTitle({ title: ['a'] })).toBe(false);
	});
});
