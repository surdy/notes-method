import { describe, expect, it } from 'vitest';

import {
	addAttachment,
	assembleContextText,
	filterAttachments,
	parseMentionTrigger,
	removeAttachment,
	type Attachment
} from './context-attachments.ts';

function att(kind: Attachment['kind'], value: string, label = value): Attachment {
	return { kind, value, label };
}

describe('parseMentionTrigger', () => {
	it('is inactive for empty input', () => {
		expect(parseMentionTrigger('', 0)).toEqual({
			active: false,
			kind: null,
			query: '',
			start: 0
		});
	});

	it('activates on a bare @ at the start with an empty query', () => {
		expect(parseMentionTrigger('@', 1)).toEqual({
			active: true,
			kind: null,
			query: '',
			start: 0
		});
	});

	it('captures the partial query after @', () => {
		const r = parseMentionTrigger('@Acm', 4);
		expect(r).toMatchObject({ active: true, kind: null, query: 'Acm', start: 0 });
	});

	it('activates after whitespace mid-input and reports the @ start index', () => {
		const input = 'tell me about @Pro';
		const r = parseMentionTrigger(input, input.length);
		expect(r).toMatchObject({ active: true, kind: null, query: 'Pro', start: 14 });
	});

	it('detects a typed kind prefix (@folder:)', () => {
		const r = parseMentionTrigger('@folder:Cust', 12);
		expect(r).toMatchObject({ active: true, kind: 'folder', query: 'Cust', start: 0 });
	});

	it('detects each known kind prefix', () => {
		expect(parseMentionTrigger('@note:x', 7).kind).toBe('note');
		expect(parseMentionTrigger('@tag:x', 6).kind).toBe('tag');
		expect(parseMentionTrigger('@url:x', 6).kind).toBe('url');
	});

	it('treats an unknown prefix before : as a plain query (no kind)', () => {
		const r = parseMentionTrigger('@foo:bar', 8);
		expect(r).toMatchObject({ active: true, kind: null, query: 'foo:bar' });
	});

	it('does not activate for an email-like @ (no whitespace boundary)', () => {
		const input = 'mail me at foo@bar';
		expect(parseMentionTrigger(input, input.length).active).toBe(false);
	});

	it('closes once whitespace is typed after the token', () => {
		const input = '@note done';
		expect(parseMentionTrigger(input, input.length).active).toBe(false);
	});

	it('uses the caret, not the end of input', () => {
		// caret sits right after "@No"; trailing text is ignored
		const input = '@Note and more';
		const r = parseMentionTrigger(input, 3);
		expect(r).toMatchObject({ active: true, query: 'No', start: 0 });
	});

	it('clamps an out-of-range caret', () => {
		expect(parseMentionTrigger('@x', 99).active).toBe(true);
		expect(parseMentionTrigger('@x', -5).active).toBe(false);
	});
});

describe('assembleContextText', () => {
	it('returns empty string when there are no attachments', () => {
		expect(assembleContextText([])).toBe('');
	});

	it('builds a deterministic context block', () => {
		const text = assembleContextText([
			att('note', 'Projects/Acme.md'),
			att('folder', 'Customers/Acme'),
			att('tag', 'urgent'),
			att('url', 'https://example.com')
		]);
		expect(text).toBe(
			[
				'[Context]',
				'- note: Projects/Acme.md',
				'- folder: Customers/Acme',
				'- tag: #urgent',
				'- url: https://example.com',
				'Use your read/list tools to fetch referenced notes/folders/tags.'
			].join('\n')
		);
	});

	it('does not double-prefix a tag that already has #', () => {
		expect(assembleContextText([att('tag', '#urgent')])).toContain('- tag: #urgent');
	});
});

describe('addAttachment / removeAttachment', () => {
	it('adds and dedupes by kind+value', () => {
		let list: Attachment[] = [];
		list = addAttachment(list, att('note', 'A.md'));
		list = addAttachment(list, att('note', 'A.md'));
		expect(list).toHaveLength(1);
		list = addAttachment(list, att('note', 'B.md'));
		expect(list).toHaveLength(2);
		// Same value, different kind is allowed.
		list = addAttachment(list, att('tag', 'A.md'));
		expect(list).toHaveLength(3);
	});

	it('removes by kind+value', () => {
		const list = [att('note', 'A.md'), att('tag', 'urgent')];
		const next = removeAttachment(list, 'note', 'A.md');
		expect(next).toEqual([att('tag', 'urgent')]);
	});

	it('does not mutate the input list', () => {
		const list = [att('note', 'A.md')];
		const next = addAttachment(list, att('note', 'B.md'));
		expect(list).toHaveLength(1);
		expect(next).toHaveLength(2);
	});
});

describe('filterAttachments', () => {
	const candidates = [
		att('note', 'Projects/Acme.md', 'Projects/Acme.md'),
		att('note', 'Work/Beacme.md', 'Work/Beacme.md'),
		att('note', 'Daily/2024.md', 'Daily/2024.md')
	];

	it('returns all candidates for an empty query', () => {
		expect(filterAttachments(candidates, '')).toHaveLength(3);
	});

	it('ranks basename-prefix matches above substring matches, case-insensitively', () => {
		const r = filterAttachments(candidates, 'acme');
		expect(r.map((a) => a.value)).toEqual(['Projects/Acme.md', 'Work/Beacme.md']);
	});

	it('returns an empty list when nothing matches', () => {
		expect(filterAttachments(candidates, 'zzz')).toEqual([]);
	});
});
