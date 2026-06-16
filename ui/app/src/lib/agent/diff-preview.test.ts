import { describe, expect, it } from 'vitest';

import { formatDiffLines } from './diff-preview.ts';

describe('formatDiffLines', () => {
	it('emits removed lines for old text then added lines for new text', () => {
		const lines = formatDiffLines({
			path: 'notes/todo.md',
			oldText: 'one\ntwo',
			newText: 'one\nthree'
		});
		expect(lines).toEqual([
			{ kind: 'removed', marker: '-', text: 'one' },
			{ kind: 'removed', marker: '-', text: 'two' },
			{ kind: 'added', marker: '+', text: 'one' },
			{ kind: 'added', marker: '+', text: 'three' }
		]);
	});

	it('treats a new file (no old text) as all added', () => {
		const lines = formatDiffLines({ path: 'new.md', oldText: null, newText: 'hello\nworld' });
		expect(lines.every((l) => l.kind === 'added')).toBe(true);
		expect(lines).toHaveLength(2);
	});

	it('does not emit a spurious empty trailing line', () => {
		const lines = formatDiffLines({ path: 'x.md', oldText: null, newText: 'a\nb\n' });
		expect(lines.map((l) => l.text)).toEqual(['a', 'b']);
	});
});
