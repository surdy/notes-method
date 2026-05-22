import { Text } from '@codemirror/state';
import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
vi.unstubAllGlobals();
vi.resetModules();
});

describe('editorStatus', () => {
it('tracks cursor and word count updates and can be reset', async () => {
vi.stubGlobal('$state', <T>(value: T) => value);
const { EditorStatusStore } = await import('./editor-status.svelte.ts');
const status = new EditorStatusStore();

expect(status.line).toBe(1);
expect(status.col).toBe(1);
expect(status.wordCount).toBe(0);

status.update(4, 7, 123);
expect(status.line).toBe(4);
expect(status.col).toBe(7);
expect(status.wordCount).toBe(123);

status.clear();
expect(status.line).toBe(1);
expect(status.col).toBe(1);
expect(status.wordCount).toBe(0);
});

it('counts whitespace-delimited words', async () => {
vi.stubGlobal('$state', <T>(value: T) => value);
const { countWords } = await import('./editor-status.svelte.ts');

expect(countWords('')).toBe(0);
expect(countWords(' one  two\nthree\tfour ')).toBe(4);
});

it('derives line and column from a CodeMirror document position', async () => {
vi.stubGlobal('$state', <T>(value: T) => value);
const { getCursorPosition } = await import('./editor-status.svelte.ts');
const doc = Text.of(['hello world', 'second line']);
const head = doc.line(2).from + 7;

expect(getCursorPosition(doc, 0)).toEqual({ line: 1, col: 1 });
expect(getCursorPosition(doc, head)).toEqual({ line: 2, col: 8 });
});
});
