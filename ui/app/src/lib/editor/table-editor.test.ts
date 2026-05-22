import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import { findCellOffset, getCellPosition, getTableRange, isInTable } from './table-editor.ts';

function createTestState(doc: string, cursorPos: number): EditorState {
	return EditorState.create({
		doc,
		selection: { anchor: cursorPos }
	});
}

describe('table editor helpers', () => {
	it('detects when the cursor is on a markdown table line', () => {
		const doc = ['Intro', '  | Name | Role |', 'Outro'].join('\n');
		const state = createTestState(doc, doc.indexOf('Name'));

		expect(isInTable(state, state.selection.main.head)).toBe(true);
	});

	it('returns false when the cursor is outside a markdown table', () => {
		const doc = ['Intro', 'No table here', '| Name | Role |'].join('\n');
		const state = createTestState(doc, doc.indexOf('No table here'));

		expect(isInTable(state, state.selection.main.head)).toBe(false);
	});

	it('finds the full range of contiguous table lines around the cursor', () => {
		const doc = [
			'Intro',
			'| Name | Role |',
			'| --- | --- |',
			'| Jane | CTO |',
			'Outro'
		].join('\n');
		const state = createTestState(doc, doc.indexOf('Jane'));
		const range = getTableRange(state, state.selection.main.head);

		expect(range).toEqual({
			from: doc.indexOf('| Name | Role |'),
			to: doc.indexOf('| Jane | CTO |') + '| Jane | CTO |'.length,
			text: ['| Name | Role |', '| --- | --- |', '| Jane | CTO |'].join('\n')
		});
	});

	it('returns null for table range outside a table', () => {
		const doc = ['Intro', '| Name | Role |', '| --- | --- |', '| Jane | CTO |'].join('\n');
		const state = createTestState(doc, doc.indexOf('Intro'));

		expect(getTableRange(state, state.selection.main.head)).toBeNull();
	});

	it('maps header, delimiter, and body cursor positions to raw table coordinates', () => {
		const doc = ['| Name | Role |', '| --- | --- |', '| Jane | CTO |'].join('\n');
		const headerState = createTestState(doc, doc.indexOf('Role'));
		const delimiterState = createTestState(doc, doc.lastIndexOf('--- |') + 1);
		const bodyState = createTestState(doc, doc.lastIndexOf('Jane'));

		expect(getCellPosition(headerState, headerState.selection.main.head)).toEqual({ row: 0, col: 1 });
		expect(getCellPosition(delimiterState, delimiterState.selection.main.head)).toEqual({ row: 1, col: 1 });
		expect(getCellPosition(bodyState, bodyState.selection.main.head)).toEqual({ row: 2, col: 0 });
	});

	it('finds the start offset of a formatted cell after column padding is applied', () => {
		const formatted = [
			'| Name     | Role           |',
			'| -------- | :------------: |',
			'| Jonathan | VP Engineering |'
		].join('\n');

		const headerOffset = findCellOffset(formatted, 0, 1);
		const bodyOffset = findCellOffset(formatted, 2, 1);

		expect(formatted.slice(headerOffset, headerOffset + 'Role'.length)).toBe('Role');
		expect(formatted.slice(bodyOffset, bodyOffset + 'VP Engineering'.length)).toBe('VP Engineering');
	});

	it('keeps empty-cell offsets at the start of the padded cell content area', () => {
		const formatted = [
			'| Name | Role     |',
			'| ---- | -------- |',
			'|      | Engineer |'
		].join('\n');
		const lineOffset = formatted.lastIndexOf('|      | Engineer |');

		expect(findCellOffset(formatted, 2, 0) - lineOffset).toBe(2);
	});
});
