import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import {
	appendToNoteSpec,
	applyOutputSpec,
	insertAtCursorSpec,
	insertOrReplaceAtCursorSpec,
	replaceSelectionSpec
} from './apply-output.ts';

function stateWith(doc: string, anchor?: number, head?: number): EditorState {
	return EditorState.create({
		doc,
		selection: anchor === undefined ? undefined : { anchor, head: head ?? anchor }
	});
}

/** Apply a TransactionSpec and return the resulting document text. */
function applied(state: EditorState, spec: ReturnType<typeof applyOutputSpec>): string {
	return state.update(spec).state.doc.toString();
}

describe('insertAtCursorSpec', () => {
	it('inserts text at the cursor position', () => {
		const state = stateWith('hello world', 5);
		const spec = insertAtCursorSpec(state, ' brave');
		expect(applied(state, spec)).toBe('hello brave world');
	});

	it('inserts at the start when the cursor is at position 0', () => {
		const state = stateWith('world', 0);
		expect(applied(state, insertAtCursorSpec(state, 'hello '))).toBe('hello world');
	});

	it('places the cursor after the inserted text', () => {
		const state = stateWith('ab', 1);
		const next = state.update(insertAtCursorSpec(state, 'XYZ')).state;
		expect(next.doc.toString()).toBe('aXYZb');
		expect(next.selection.main.head).toBe(4);
	});

	it('expresses the edit as a single change set', () => {
		const state = stateWith('abc', 1);
		const spec = insertAtCursorSpec(state, 'Z');
		expect(Array.isArray(spec.changes)).toBe(false);
		expect(state.update(spec).changes.empty).toBe(false);
	});
});

describe('replaceSelectionSpec', () => {
	it('replaces the selected range', () => {
		const state = stateWith('hello world', 6, 11);
		expect(applied(state, replaceSelectionSpec(state, 'there'))).toBe('hello there');
	});

	it('replaces a backwards selection (head before anchor)', () => {
		const state = stateWith('hello world', 11, 6);
		expect(applied(state, replaceSelectionSpec(state, 'there'))).toBe('hello there');
	});

	it('behaves like insert when there is no selection', () => {
		const state = stateWith('ac', 1);
		expect(applied(state, replaceSelectionSpec(state, 'b'))).toBe('abc');
	});

	it('places the cursor after the replacement', () => {
		const state = stateWith('hello world', 6, 11);
		const next = state.update(replaceSelectionSpec(state, 'there')).state;
		expect(next.selection.main.head).toBe(11);
	});

	it('expresses the edit as a single change set', () => {
		const state = stateWith('hello world', 6, 11);
		const spec = replaceSelectionSpec(state, 'there');
		expect(Array.isArray(spec.changes)).toBe(false);
	});
});

describe('insertOrReplaceAtCursorSpec', () => {
	it('inserts at the cursor when there is no selection', () => {
		const state = stateWith('ac', 1);
		expect(applied(state, insertOrReplaceAtCursorSpec(state, 'b'))).toBe('abc');
	});

	it('replaces the selected range when there is a selection', () => {
		const state = stateWith('hello world', 6, 11);
		expect(applied(state, insertOrReplaceAtCursorSpec(state, 'there'))).toBe('hello there');
	});

	it('places the cursor after the inserted or replaced text', () => {
		const state = stateWith('hello world', 6, 11);
		const next = state.update(insertOrReplaceAtCursorSpec(state, 'there')).state;
		expect(next.selection.main.head).toBe(11);
	});
});

describe('appendToNoteSpec', () => {
	it('appends to the end of the document with a newline separator', () => {
		const state = stateWith('first line');
		expect(applied(state, appendToNoteSpec(state, 'second line'))).toBe('first line\nsecond line');
	});

	it('does not add an extra newline when the document already ends with one', () => {
		const state = stateWith('first line\n');
		expect(applied(state, appendToNoteSpec(state, 'second line'))).toBe('first line\nsecond line');
	});

	it('appends without a leading newline to an empty document', () => {
		const state = stateWith('');
		expect(applied(state, appendToNoteSpec(state, 'content'))).toBe('content');
	});

	it('ignores the current selection and always targets the end', () => {
		const state = stateWith('abc', 1);
		expect(applied(state, appendToNoteSpec(state, 'xyz'))).toBe('abc\nxyz');
	});

	it('expresses the edit as a single change set', () => {
		const state = stateWith('abc');
		const spec = appendToNoteSpec(state, 'xyz');
		expect(Array.isArray(spec.changes)).toBe(false);
	});
});

describe('applyOutputSpec', () => {
	it('dispatches to the insert behaviour', () => {
		const state = stateWith('ac', 1);
		expect(applied(state, applyOutputSpec(state, 'insert', 'b'))).toBe('abc');
	});

	it('dispatches to the replace behaviour', () => {
		const state = stateWith('hello world', 6, 11);
		expect(applied(state, applyOutputSpec(state, 'replace', 'there'))).toBe('hello there');
	});

	it('dispatches to the append behaviour', () => {
		const state = stateWith('a');
		expect(applied(state, applyOutputSpec(state, 'append', 'b'))).toBe('a\nb');
	});

	it('dispatches to the selection-aware cursor behaviour (insert with no selection)', () => {
		const state = stateWith('ac', 1);
		expect(applied(state, applyOutputSpec(state, 'cursor', 'b'))).toBe('abc');
	});

	it('dispatches to the selection-aware cursor behaviour (replace with a selection)', () => {
		const state = stateWith('hello world', 6, 11);
		expect(applied(state, applyOutputSpec(state, 'cursor', 'there'))).toBe('hello there');
	});
});
