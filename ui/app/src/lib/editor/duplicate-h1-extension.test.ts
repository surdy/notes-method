import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import {
	duplicateH1HideExtension,
	setDuplicateH1TitleEffect
} from './duplicate-h1-extension';

function makeState(doc: string, title: string | null): EditorState {
	let state = EditorState.create({
		doc,
		extensions: [duplicateH1HideExtension()]
	});
	if (title !== null) {
		state = state.update({ effects: setDuplicateH1TitleEffect.of(title) }).state;
	}
	return state;
}

function hiddenRanges(state: EditorState): Array<[number, number]> {
	const providers = state.facet(EditorView.decorations);
	const ranges: Array<[number, number]> = [];
	for (const provider of providers) {
		if (typeof provider === 'function') continue;
		const iter = provider.iter();
		while (iter.value) {
			ranges.push([iter.from, iter.to]);
			iter.next();
		}
	}
	return ranges;
}

describe('duplicateH1HideExtension', () => {
	it('hides the leading H1 line when it matches the title', () => {
		const state = makeState('# My Note\n\nBody text\n', 'My Note');
		expect(hiddenRanges(state)).toHaveLength(1);
	});

	it('does not hide when title does not match', () => {
		const state = makeState('# Different\n\nBody\n', 'My Note');
		expect(hiddenRanges(state)).toHaveLength(0);
	});

	it('does not hide when first content line is not an H1', () => {
		const state = makeState('Plain text\n\n# My Note\n', 'My Note');
		expect(hiddenRanges(state)).toHaveLength(0);
	});

	it('does not hide H2 even if text matches title', () => {
		const state = makeState('## My Note\n', 'My Note');
		expect(hiddenRanges(state)).toHaveLength(0);
	});

	it('skips frontmatter and matches first H1 after it', () => {
		const state = makeState('---\ntitle: Foo\n---\n# My Note\n\nBody\n', 'My Note');
		expect(hiddenRanges(state)).toHaveLength(1);
	});

	it('matches case-insensitively and trims whitespace', () => {
		const state = makeState('#   my note   \n', 'My Note');
		expect(hiddenRanges(state)).toHaveLength(1);
	});

	it('does nothing when title is null', () => {
		const state = makeState('# My Note\n', null);
		expect(hiddenRanges(state)).toHaveLength(0);
	});

	it('updates when title effect changes', () => {
		let state = makeState('# My Note\n', 'Different');
		expect(hiddenRanges(state)).toHaveLength(0);
		state = state.update({ effects: setDuplicateH1TitleEffect.of('My Note') }).state;
		expect(hiddenRanges(state)).toHaveLength(1);
	});
});

