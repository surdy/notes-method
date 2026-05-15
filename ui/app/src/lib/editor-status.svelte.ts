import type { Text } from '@codemirror/state';

export class EditorStatusStore {
	line = $state(1);
	col = $state(1);
	wordCount = $state(0);

	update(line: number, col: number, wordCount: number) {
		this.line = line;
		this.col = col;
		this.wordCount = wordCount;
	}

	clear() {
		this.line = 1;
		this.col = 1;
		this.wordCount = 0;
	}
}

export function countWords(text: string): number {
	return text.split(/\s+/).filter(Boolean).length;
}

export function getCursorPosition(doc: Text, head: number) {
	const line = doc.lineAt(head);
	return {
		line: line.number,
		col: head - line.from + 1
	};
}

export const editorStatus = new EditorStatusStore();
