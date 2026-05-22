import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import {
	detectInsideLinkParens,
	determinePasteAction,
	isUrl,
	matchesImageWhitelist
} from './paste-url.ts';

describe('determinePasteAction', () => {
	it('wraps selected text with a pasted URL as a markdown link', () => {
		expect(determinePasteAction('Notesmith', 'https://example.com', '')).toEqual({
			type: 'markdown-link',
			text: 'Notesmith',
			url: 'https://example.com'
		});
	});

	it('uses pasted text as the label when the selected text is a URL', () => {
		expect(determinePasteAction('https://example.com', 'Notesmith', '')).toEqual({
			type: 'markdown-link',
			text: 'Notesmith',
			url: 'https://example.com'
		});
	});

	it('creates an image embed when the pasted URL matches the image whitelist', () => {
		expect(determinePasteAction('Diagram', 'https://imgur.com/demo.png', 'imgur\\.com')).toEqual({
			type: 'image-embed',
			alt: 'Diagram',
			url: 'https://imgur.com/demo.png'
		});
	});

	it('creates a markdown link when the pasted URL does not match the image whitelist', () => {
		expect(determinePasteAction('Notesmith', 'https://example.com', 'imgur\\.com')).toEqual({
			type: 'markdown-link',
			text: 'Notesmith',
			url: 'https://example.com'
		});
	});

	it('does not create an image embed when the selected text is itself a URL', () => {
		expect(
			determinePasteAction('https://notesmith.app', 'https://imgur.com/demo.png', 'imgur\\.com')
		).toEqual({
			type: 'markdown-link',
			text: 'https://notesmith.app',
			url: 'https://imgur.com/demo.png'
		});
	});

	it('creates a wikilink alias when wikilink text is pasted onto a selection', () => {
		expect(determinePasteAction('Display Text', '[[Target Note]]', '')).toEqual({
			type: 'wikilink-alias',
			link: 'Target Note',
			alias: 'Display Text'
		});
	});

	it('passes through when neither side looks like a link action', () => {
		expect(determinePasteAction('Notesmith', 'just some text', '')).toEqual({ type: 'passthrough' });
	});

	it('never creates an image embed when the whitelist is empty', () => {
		expect(determinePasteAction('Diagram', 'https://imgur.com/demo.png', '')).toEqual({
			type: 'markdown-link',
			text: 'Diagram',
			url: 'https://imgur.com/demo.png'
		});
	});

	it('matches any whitelist pattern on separate lines', () => {
		expect(
			determinePasteAction(
				'Screenshot',
				'https://images.example.com/demo.webp',
				'youtu.?be|vimeo\n.*\\.(?:png|jpg|gif|webp)'
			)
		).toEqual({
			type: 'image-embed',
			alt: 'Screenshot',
			url: 'https://images.example.com/demo.webp'
		});
	});
});

describe('matchesImageWhitelist', () => {
	it('returns false for an empty whitelist', () => {
		expect(matchesImageWhitelist('https://imgur.com/demo.png', '')).toBe(false);
	});

	it('returns true when a single pattern matches', () => {
		expect(matchesImageWhitelist('https://imgur.com/demo.png', 'imgur\\.com')).toBe(true);
	});

	it('returns false when no pattern matches', () => {
		expect(matchesImageWhitelist('https://example.com/demo.png', 'imgur\\.com')).toBe(false);
	});

	it('returns true when any multiline pattern matches', () => {
		expect(
			matchesImageWhitelist(
				'https://images.example.com/demo.gif',
				'youtu.?be|vimeo\n.*\\.(?:png|jpg|gif)'
			)
		).toBe(true);
	});
});

describe('isUrl', () => {
	it('recognizes http and https URLs', () => {
		expect(isUrl('https://example.com')).toBe(true);
		expect(isUrl('http://foo.bar/baz')).toBe(true);
	});

	it('rejects plain text', () => {
		expect(isUrl('just some text')).toBe(false);
	});

	it('rejects non-url tokens', () => {
		expect(isUrl('not-a-url')).toBe(false);
	});
});

describe('detectInsideLinkParens', () => {
	it('returns the range inside markdown link parentheses when the cursor is inside them', () => {
		const doc = '[text]()';
		const cursor = doc.indexOf('(') + 1;
		const state = EditorState.create({ doc, selection: { anchor: cursor } });

		expect(detectInsideLinkParens(state, cursor)).toEqual({
			from: cursor,
			to: cursor
		});
	});

	it('returns null when the cursor is outside markdown link parentheses', () => {
		const doc = '[text]()';
		const cursor = doc.indexOf('text');
		const state = EditorState.create({ doc, selection: { anchor: cursor } });

		expect(detectInsideLinkParens(state, cursor)).toBeNull();
	});

	it('returns the existing URL range when the cursor is inside populated link parentheses', () => {
		const doc = '[text](existing-url)';
		const cursor = doc.indexOf('url');
		const state = EditorState.create({ doc, selection: { anchor: cursor } });

		expect(detectInsideLinkParens(state, cursor)).toEqual({
			from: doc.indexOf('existing-url'),
			to: doc.indexOf(')')
		});
	});
});
