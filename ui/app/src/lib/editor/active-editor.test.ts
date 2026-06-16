import { EditorState } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ActiveEditorStore } from './active-editor.svelte.ts';

// The store uses Svelte's `$state` rune. In the unit test runtime there is no
// Svelte compiler, so stub it as identity (the repo convention; see
// break-glass.test.ts) and dynamically import the singleton per test.
beforeEach(() => {
	vi.stubGlobal('$state', <T>(value: T) => value);
});

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

async function freshStore(): Promise<ActiveEditorStore> {
	const { ActiveEditorStore } = await import('./active-editor.svelte.ts');
	return new ActiveEditorStore();
}

/** A minimal EditorView stand-in that applies dispatched specs to its state. */
function mockView(doc: string, anchor?: number, head?: number) {
	const view = {
		state: EditorState.create({
			doc,
			selection: anchor === undefined ? undefined : { anchor, head: head ?? anchor }
		}),
		focus: vi.fn(),
		dispatch: vi.fn()
	};
	view.dispatch.mockImplementation((spec) => {
		view.state = view.state.update(spec).state;
	});
	return view as unknown as EditorView & {
		focus: ReturnType<typeof vi.fn>;
		dispatch: ReturnType<typeof vi.fn>;
	};
}

describe('ActiveEditorStore', () => {
	it('reports no active editor before registration', async () => {
		const store = await freshStore();
		expect(store.hasActiveEditor).toBe(false);
		expect(store.applyOutput('append', 'x')).toBe(false);
	});

	it('appends to the registered view as a single dispatch', async () => {
		const store = await freshStore();
		const view = mockView('first');
		store.register(view);

		expect(store.hasActiveEditor).toBe(true);
		expect(store.applyOutput('append', 'second')).toBe(true);
		expect(view.dispatch).toHaveBeenCalledTimes(1);
		expect(view.focus).toHaveBeenCalledTimes(1);
		expect(view.state.doc.toString()).toBe('first\nsecond');
	});

	it('inserts at the cursor of the registered view', async () => {
		const store = await freshStore();
		const view = mockView('ac', 1);
		store.register(view);
		expect(store.applyOutput('insert', 'b')).toBe(true);
		expect(view.state.doc.toString()).toBe('abc');
	});

	it('replaces the selection of the registered view', async () => {
		const store = await freshStore();
		const view = mockView('hello world', 6, 11);
		store.register(view);
		expect(store.applyOutput('replace', 'there')).toBe(true);
		expect(view.state.doc.toString()).toBe('hello there');
	});

	it('unregister clears only when the same view still owns the slot', async () => {
		const store = await freshStore();
		const first = mockView('a');
		const second = mockView('b');
		store.register(first);
		store.register(second);

		store.unregister(first);
		expect(store.hasActiveEditor).toBe(true);

		store.unregister(second);
		expect(store.hasActiveEditor).toBe(false);
	});
});
