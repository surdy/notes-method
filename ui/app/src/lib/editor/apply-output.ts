/**
 * Pure helpers that compute the CodeMirror transaction for applying agent
 * output to the active note (issue #196). Each helper returns a single
 * {@link TransactionSpec} with exactly one change set, so dispatching it is one
 * undo step. The Svelte glue (NoteEditor / ChatMessage via the active-editor
 * store) calls into these so the targeting logic is unit-testable without a DOM.
 */

import type { EditorState, TransactionSpec } from '@codemirror/state';

export type ApplyMode = 'insert' | 'replace' | 'append';

/** Tag edits as paste-like so undo coalesces the whole insertion into one step. */
const USER_EVENT = 'input.paste';

/** Insert text at the cursor (main selection head), leaving the cursor after it. */
export function insertAtCursorSpec(state: EditorState, text: string): TransactionSpec {
	const head = state.selection.main.head;
	return {
		changes: { from: head, insert: text },
		selection: { anchor: head + text.length },
		userEvent: USER_EVENT,
		scrollIntoView: true
	};
}

/**
 * Replace the main selection with text. With an empty selection this is
 * equivalent to {@link insertAtCursorSpec}. The cursor ends after the
 * replacement.
 */
export function replaceSelectionSpec(state: EditorState, text: string): TransactionSpec {
	const { from, to } = state.selection.main;
	return {
		changes: { from, to, insert: text },
		selection: { anchor: from + text.length },
		userEvent: USER_EVENT,
		scrollIntoView: true
	};
}

/**
 * Append text to the end of the document, ignoring the current selection. A
 * single newline is inserted as a separator unless the document is empty or
 * already ends with one.
 */
export function appendToNoteSpec(state: EditorState, text: string): TransactionSpec {
	const end = state.doc.length;
	const needsSeparator = end > 0 && state.doc.sliceString(end - 1, end) !== '\n';
	const insert = needsSeparator ? `\n${text}` : text;
	return {
		changes: { from: end, insert },
		selection: { anchor: end + insert.length },
		userEvent: USER_EVENT,
		scrollIntoView: true
	};
}

/** Dispatch to the correct targeting helper for the requested {@link ApplyMode}. */
export function applyOutputSpec(state: EditorState, mode: ApplyMode, text: string): TransactionSpec {
	switch (mode) {
		case 'insert':
			return insertAtCursorSpec(state, text);
		case 'replace':
			return replaceSelectionSpec(state, text);
		case 'append':
			return appendToNoteSpec(state, text);
	}
}
