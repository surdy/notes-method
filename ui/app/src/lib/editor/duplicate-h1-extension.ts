import { StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';

/**
 * Live-preview extension that hides the first H1 line of a note when
 * its text duplicates the note's displayed title (filename or
 * frontmatter `title:`). Pairs with the reading-view stripping in
 * `duplicate-h1.ts`.
 *
 * The host (NoteEditor) dispatches `setDuplicateH1TitleEffect` whenever
 * the active note changes so this extension knows what to compare
 * against. When the title is `null`, no hiding is applied.
 */

export const setDuplicateH1TitleEffect = StateEffect.define<string | null>();

const titleField = StateField.define<string | null>({
	create: () => null,
	update: (value, tr) => {
		for (const effect of tr.effects) {
			if (effect.is(setDuplicateH1TitleEffect)) {
				return effect.value;
			}
		}
		return value;
	}
});

function normalize(value: string): string {
	return value.trim().toLowerCase();
}

const hideLineDeco = Decoration.replace({});

const decorationsField = StateField.define<DecorationSet>({
	create: (state) => buildDecorations(state.field(titleField), state.doc),
	update: (value, tr) => {
		if (!tr.docChanged && !tr.effects.some((e) => e.is(setDuplicateH1TitleEffect))) {
			return value;
		}
		return buildDecorations(tr.state.field(titleField), tr.state.doc);
	},
	provide: (field) => EditorView.decorations.from(field)
});

function buildDecorations(
	title: string | null,
	doc: { lines: number; line: (n: number) => { from: number; to: number; text: string } }
): DecorationSet {
	if (!title || doc.lines === 0) return Decoration.none;
	const target = normalize(title);
	if (!target) return Decoration.none;

	let startLine = 1;
	const first = doc.line(1).text;
	if (first.trim() === '---') {
		for (let i = 2; i <= doc.lines; i++) {
			if (doc.line(i).text.trim() === '---') {
				startLine = i + 1;
				break;
			}
		}
	}

	for (let i = startLine; i <= doc.lines; i++) {
		const line = doc.line(i);
		const text = line.text;
		if (text.trim() === '') continue;
		const match = text.match(/^(#{1,6})\s+(.+?)\s*$/);
		if (!match || match[1].length !== 1) {
			return Decoration.none;
		}
		const heading = normalize(match[2]);
		if (heading !== target) return Decoration.none;
		const from = line.from;
		const to = i < doc.lines ? doc.line(i + 1).from : line.to;
		return Decoration.set([hideLineDeco.range(from, to)]);
	}

	return Decoration.none;
}

export function duplicateH1HideExtension(): Extension {
	return [titleField, decorationsField];
}
