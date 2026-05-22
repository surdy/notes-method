import { EditorState, type TransactionSpec } from '@codemirror/state';
import { keymap, type EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';

import {
	createTableEditorExtension,
	findCellOffset,
	getCellPosition,
	getTableRange,
	isInTable
} from './table-editor.ts';

function createTestState(doc: string, cursorPos: number): EditorState {
	return EditorState.create({
		doc,
		selection: { anchor: cursorPos }
	});
}

function runShortcut(doc: string, cursorPos: number, key: string): { result: boolean; state: EditorState } {
	let state = EditorState.create({
		doc,
		selection: { anchor: cursorPos },
		extensions: [createTableEditorExtension()]
	});
	const binding = state
		.facet(keymap)
		.flat()
		.find((candidate) => candidate.key === key);
	if (!binding?.run) {
		throw new Error(`${key} binding not found`);
	}

	const view = {
		get state() {
			return state;
		},
		dispatch(spec: TransactionSpec) {
			state = state.update(spec).state;
		}
	} as unknown as EditorView;

	return {
		result: binding.run(view),
		state
	};
}

function pressTab(doc: string, cursorPos: number): { handled: boolean; doc: string; cursor: number } {
	const { result, state } = runShortcut(doc, cursorPos, 'Tab');
	return {
		handled: result,
		doc: state.doc.toString(),
		cursor: state.selection.main.head
	};
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

describe('table editor structure shortcuts', () => {
	const table = [
		'| Name | Role | Team |',
		'| ---- | ---- | ---- |',
		'| Jane | CTO  | Eng  |',
		'| Alex | Dev  | Apps |'
	].join('\n');
	const singleColumnTable = ['| Name |', '| ---- |', '| Jane |'].join('\n');

	it('moves the current body row up with Mod-Shift-ArrowUp', () => {
		const { result, state } = runShortcut(table, table.indexOf('Dev'), 'Mod-Shift-ArrowUp');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role | Team |',
				'| ---- | ---- | ---- |',
				'| Alex | Dev  | Apps |',
				'| Jane | CTO  | Eng  |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 2, col: 1 });
	});

	it('treats Mod-Shift-ArrowUp as a no-op on the first body row', () => {
		const { result, state } = runShortcut(table, table.indexOf('CTO'), 'Mod-Shift-ArrowUp');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(table);
	});

	it('moves the current body row down with Mod-Shift-ArrowDown', () => {
		const { result, state } = runShortcut(table, table.indexOf('CTO'), 'Mod-Shift-ArrowDown');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role | Team |',
				'| ---- | ---- | ---- |',
				'| Alex | Dev  | Apps |',
				'| Jane | CTO  | Eng  |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 3, col: 1 });
	});

	it('treats Mod-Shift-ArrowDown as a no-op on the last body row', () => {
		const { result, state } = runShortcut(table, table.indexOf('Dev'), 'Mod-Shift-ArrowDown');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(table);
	});

	it('moves the current column left with Mod-Shift-ArrowLeft', () => {
		const { result, state } = runShortcut(table, table.indexOf('Apps'), 'Mod-Shift-ArrowLeft');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Team | Role |',
				'| ---- | ---- | ---- |',
				'| Jane | Eng  | CTO  |',
				'| Alex | Apps | Dev  |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 3, col: 1 });
	});

	it('treats Mod-Shift-ArrowLeft as a no-op on the first column', () => {
		const { result, state } = runShortcut(table, table.indexOf('Alex'), 'Mod-Shift-ArrowLeft');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(table);
	});

	it('moves the current column right with Mod-Shift-ArrowRight', () => {
		const { result, state } = runShortcut(table, table.indexOf('CTO'), 'Mod-Shift-ArrowRight');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Team | Role |',
				'| ---- | ---- | ---- |',
				'| Jane | Eng  | CTO  |',
				'| Alex | Apps | Dev  |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 2, col: 2 });
	});

	it('treats Mod-Shift-ArrowRight as a no-op on the last column', () => {
		const { result, state } = runShortcut(table, table.indexOf('Team'), 'Mod-Shift-ArrowRight');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(table);
	});

	it('inserts a row below the current body row with Mod-Shift-Enter', () => {
		const { result, state } = runShortcut(table, table.indexOf('Jane'), 'Mod-Shift-Enter');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role | Team |',
				'| ---- | ---- | ---- |',
				'| Jane | CTO  | Eng  |',
				'|      |      |      |',
				'| Alex | Dev  | Apps |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 3, col: 0 });
	});

	it('inserts the first body row when Mod-Shift-Enter is pressed on the header', () => {
		const { result, state } = runShortcut(table, table.indexOf('Name'), 'Mod-Shift-Enter');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role | Team |',
				'| ---- | ---- | ---- |',
				'|      |      |      |',
				'| Jane | CTO  | Eng  |',
				'| Alex | Dev  | Apps |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 2, col: 0 });
	});

	it('deletes the current body row with Mod-Shift-Backspace', () => {
		const { result, state } = runShortcut(table, table.indexOf('Jane'), 'Mod-Shift-Backspace');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role | Team |',
				'| ---- | ---- | ---- |',
				'| Alex | Dev  | Apps |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 2, col: 0 });
	});

	it('treats Mod-Shift-Backspace as a no-op on the header', () => {
		const { result, state } = runShortcut(table, table.indexOf('Name'), 'Mod-Shift-Backspace');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(table);
	});

	it('inserts a column after the current column with Mod-Shift-\\', () => {
		const { result, state } = runShortcut(table, table.indexOf('Role'), 'Mod-Shift-\\');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role |  | Team |',
				'| ---- | ---- | --- | ---- |',
				'| Jane | CTO  |  | Eng  |',
				'| Alex | Dev  |  | Apps |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 0, col: 2 });
	});

	it('appends a column when Mod-Shift-\\ is pressed on the last column', () => {
		const { result, state } = runShortcut(table, table.indexOf('Team'), 'Mod-Shift-\\');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Role | Team |  |',
				'| ---- | ---- | ---- | --- |',
				'| Jane | CTO  | Eng  |  |',
				'| Alex | Dev  | Apps |  |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 0, col: 3 });
	});

	it('deletes the current column with Mod-Shift-Delete', () => {
		const { result, state } = runShortcut(table, table.indexOf('CTO'), 'Mod-Shift-Delete');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(
			[
				'| Name | Team |',
				'| ---- | ---- |',
				'| Jane | Eng  |',
				'| Alex | Apps |'
			].join('\n')
		);
		expect(getCellPosition(state, state.selection.main.head)).toEqual({ row: 2, col: 1 });
	});

	it('treats Mod-Shift-Delete as a no-op for a single-column table', () => {
		const { result, state } = runShortcut(singleColumnTable, singleColumnTable.indexOf('Jane'), 'Mod-Shift-Delete');

		expect(result).toBe(true);
		expect(state.doc.toString()).toBe(singleColumnTable);
	});
});

describe('table editor Tab behavior', () => {
	it('bootstraps a single header line into a table', () => {
		const input = '| Name';
		const expected = ['| Name |', '| ---- |', '|      |'].join('\n');

		const result = pressTab(input, input.indexOf('Name'));

		expect(result.handled).toBe(true);
		expect(result.doc).toBe(expected);
		expect(result.cursor).toBe(expected.indexOf('|      |') + 2);
	});

	it('bootstraps multiple headers into a full table', () => {
		const input = '| Name | Role';
		const expected = ['| Name | Role |', '| ---- | ---- |', '|      |      |'].join('\n');

		const result = pressTab(input, input.indexOf('Role'));

		expect(result.handled).toBe(true);
		expect(result.doc).toBe(expected);
		expect(result.cursor).toBe(expected.indexOf('|      |      |') + 2);
	});

	it('keeps normal navigation when a delimiter row already exists', () => {
		const input = ['| Name | Role |', '| ---- | ---- |', '| Jane | CTO  |'].join('\n');

		const result = pressTab(input, input.indexOf('Name'));

		expect(result.handled).toBe(true);
		expect(result.doc).toBe(input);
		expect(result.cursor).toBe(input.indexOf('Role'));
	});

	it('treats a trailing pipe as a single header bootstrap', () => {
		const input = '| Name |';
		const expected = ['| Name |', '| ---- |', '|      |'].join('\n');

		const result = pressTab(input, input.indexOf('Name'));

		expect(result.handled).toBe(true);
		expect(result.doc).toBe(expected);
		expect(result.cursor).toBe(expected.indexOf('|      |') + 2);
	});
});
