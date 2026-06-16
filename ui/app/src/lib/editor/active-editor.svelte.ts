/**
 * Bridge between the agent chat panel and the active note's CodeMirror view
 * (issue #196). The chat lives in the right dock while the editor lives in the
 * main pane; they are separate component trees, so the editor registers its
 * {@link EditorView} here on mount and chat message actions reach it through
 * this shared store. Targeting logic lives in the pure `apply-output` module so
 * the transaction is a single, undoable change set.
 */

import type { EditorView } from '@codemirror/view';

import { applyOutputSpec, type ApplyMode } from './apply-output.ts';

export class ActiveEditorStore {
	/** The currently mounted editor view, or null when none is editable. */
	view = $state<EditorView | null>(null);

	get hasActiveEditor(): boolean {
		return this.view !== null;
	}

	/** Called by NoteEditor once its view is created. */
	register(view: EditorView): void {
		this.view = view;
	}

	/** Called by NoteEditor on teardown; only clears if it still owns the slot. */
	unregister(view: EditorView): void {
		if (this.view === view) {
			this.view = null;
		}
	}

	/**
	 * Apply agent output to the active note as one undo-able transaction. Returns
	 * false (and does nothing) when there is no active editor so callers can
	 * surface a toast.
	 */
	applyOutput(mode: ApplyMode, text: string): boolean {
		const view = this.view;
		if (!view) {
			return false;
		}
		view.dispatch(applyOutputSpec(view.state, mode, text));
		view.focus();
		return true;
	}
}

export const activeEditorStore = new ActiveEditorStore();
