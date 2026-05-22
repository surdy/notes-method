import { Prec, type EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';

import { addMarkdownTableRow, formatTable, parseMarkdownTable, type MarkdownTable } from './markdown-table.ts';

const TABLE_LINE_RE = /^\s*\|/;

type TableRange = { from: number; to: number; text: string };
type TableCellPosition = { row: number; col: number };

export function isInTable(state: EditorState, pos: number): boolean {
	return TABLE_LINE_RE.test(state.doc.lineAt(pos).text);
}

export function getTableRange(state: EditorState, pos: number): TableRange | null {
	const line = state.doc.lineAt(pos);
	if (!TABLE_LINE_RE.test(line.text)) {
		return null;
	}

	let startLine = line;
	while (startLine.number > 1) {
		const previous = state.doc.line(startLine.number - 1);
		if (!TABLE_LINE_RE.test(previous.text)) {
			break;
		}
		startLine = previous;
	}

	let endLine = line;
	while (endLine.number < state.doc.lines) {
		const next = state.doc.line(endLine.number + 1);
		if (!TABLE_LINE_RE.test(next.text)) {
			break;
		}
		endLine = next;
	}

	return {
		from: startLine.from,
		to: endLine.to,
		text: state.doc.sliceString(startLine.from, endLine.to)
	};
}

export function getCellPosition(state: EditorState, pos: number): TableCellPosition | null {
	const range = getTableRange(state, pos);
	if (!range) {
		return null;
	}

	const table = parseMarkdownTable(range.text);
	const columns = table.headers.length;
	if (columns === 0) {
		return null;
	}

	const currentLine = state.doc.lineAt(pos);
	const startLine = state.doc.lineAt(range.from);
	const row = currentLine.number - startLine.number;
	const relativePos = Math.max(0, Math.min(pos - currentLine.from, currentLine.length));
	const textBeforeCursor = currentLine.text.slice(0, relativePos);
	const pipeCount = [...textBeforeCursor].filter((char) => char === '|').length;
	const rawColumn = pipeCount === 0 ? 0 : pipeCount - 1;

	return {
		row,
		col: Math.max(0, Math.min(rawColumn, columns - 1))
	};
}

export function findCellOffset(formattedText: string, targetRow: number, targetCol: number): number {
	const lines = formattedText.split('\n');
	const safeRow = Math.max(0, Math.min(targetRow, lines.length - 1));
	const line = lines[safeRow] ?? '';
	const lineOffset = lines.slice(0, safeRow).reduce((total, current) => total + current.length + 1, 0);
	let pipeCount = 0;

	for (let index = 0; index < line.length; index += 1) {
		if (line[index] !== '|') {
			continue;
		}

		pipeCount += 1;
		if (pipeCount !== targetCol + 1) {
			continue;
		}

		let cellStart = index + 1;
		if (line[cellStart] === ' ') {
			cellStart += 1;
		}
		return lineOffset + cellStart;
	}

	return lineOffset;
}

function isNavigableTable(table: MarkdownTable): boolean {
	return table.headers.length > 0;
}

function ensureBodyRow(table: MarkdownTable, targetRow: number): MarkdownTable {
	let nextTable = table;
	while (targetRow >= 2 && nextTable.rows.length < targetRow - 1) {
		nextTable = addMarkdownTableRow(nextTable);
	}
	return nextTable;
}

function moveToCell(view: EditorView, range: TableRange, table: MarkdownTable, target: TableCellPosition): boolean {
	const formattedText = formatTable(table);
	const anchor = range.from + findCellOffset(formattedText, target.row, target.col);
	view.dispatch({
		changes: { from: range.from, to: range.to, insert: formattedText },
		selection: { anchor },
		scrollIntoView: true
	});
	return true;
}

function getTableContext(state: EditorState, pos: number, isEnabled: () => boolean): {
	range: TableRange;
	table: MarkdownTable;
	cell: TableCellPosition;
	lastColumn: number;
} | null {
	if (!isEnabled()) {
		return null;
	}

	const range = getTableRange(state, pos);
	if (!range) {
		return null;
	}

	const table = parseMarkdownTable(range.text);
	if (!isNavigableTable(table)) {
		return null;
	}

	const cell = getCellPosition(state, pos);
	if (!cell) {
		return null;
	}

	return {
		range,
		table,
		cell,
		lastColumn: table.headers.length - 1
	};
}

function tabTarget(table: MarkdownTable, cell: TableCellPosition): { table: MarkdownTable; target: TableCellPosition } {
	let target: TableCellPosition;
	if (cell.row === 1) {
		target = { row: 2, col: cell.col };
	} else if (cell.row === 0) {
		target = cell.col < table.headers.length - 1 ? { row: 0, col: cell.col + 1 } : { row: 2, col: 0 };
	} else {
		target = cell.col < table.headers.length - 1 ? { row: cell.row, col: cell.col + 1 } : { row: cell.row + 1, col: 0 };
	}

	return {
		table: ensureBodyRow(table, target.row),
		target
	};
}

function shiftTabTarget(
	table: MarkdownTable,
	cell: TableCellPosition,
	lastColumn: number
): { table: MarkdownTable; target: TableCellPosition } | null {
	if (cell.row === 0 && cell.col === 0) {
		return null;
	}

	if (cell.row === 1) {
		return { table, target: { row: 0, col: cell.col } };
	}

	if (cell.row === 0) {
		return { table, target: { row: 0, col: cell.col - 1 } };
	}

	if (cell.col > 0) {
		return { table, target: { row: cell.row, col: cell.col - 1 } };
	}

	return {
		table,
		target: cell.row === 2 ? { row: 0, col: lastColumn } : { row: cell.row - 1, col: lastColumn }
	};
}

function enterTarget(table: MarkdownTable, cell: TableCellPosition): { table: MarkdownTable; target: TableCellPosition } {
	const target = { row: cell.row <= 1 ? 2 : cell.row + 1, col: 0 };
	return {
		table: ensureBodyRow(table, target.row),
		target
	};
}

function moveAfterTable(view: EditorView, isEnabled: () => boolean): boolean {
	if (!isEnabled()) {
		return false;
	}

	const pos = view.state.selection.main.head;
	const range = getTableRange(view.state, pos);
	if (!range) {
		return false;
	}

	const endLine = view.state.doc.lineAt(Math.max(range.to - 1, range.from));
	const anchor = endLine.number < view.state.doc.lines ? view.state.doc.line(endLine.number + 1).from : range.to;
	view.dispatch({ selection: { anchor }, scrollIntoView: true });
	return true;
}

export function createTableEditorExtension(isEnabled: () => boolean = () => true): Extension {
	return Prec.high(
		keymap.of([
			{
				key: 'Tab',
				run: (view) => {
					const context = getTableContext(view.state, view.state.selection.main.head, isEnabled);
					if (!context) {
						return false;
					}

					const { table, target } = tabTarget(context.table, context.cell);
					return moveToCell(view, context.range, table, target);
				}
			},
			{
				key: 'Shift-Tab',
				run: (view) => {
					const context = getTableContext(view.state, view.state.selection.main.head, isEnabled);
					if (!context) {
						return false;
					}

					const result = shiftTabTarget(context.table, context.cell, context.lastColumn);
					if (!result) {
						return true;
					}

					return moveToCell(view, context.range, result.table, result.target);
				}
			},
			{
				key: 'Enter',
				run: (view) => {
					const context = getTableContext(view.state, view.state.selection.main.head, isEnabled);
					if (!context) {
						return false;
					}

					const { table, target } = enterTarget(context.table, context.cell);
					return moveToCell(view, context.range, table, target);
				}
			},
			{
				key: 'Escape',
				run: (view) => moveAfterTable(view, isEnabled)
			}
		])
	);
}
