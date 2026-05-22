export type MarkdownTableAlignment = 'left' | 'center' | 'right';

export interface MarkdownTable {
	headers: string[];
	alignments: MarkdownTableAlignment[];
	rows: string[][];
}

export interface MarkdownTableCellUpdate {
	section: 'header' | 'body';
	rowIndex: number;
	columnIndex: number;
	value: string;
}

function parseCells(line: string): string[] {
	return line
		.trim()
		.replace(/^\|/, '')
		.replace(/\|$/, '')
		.split('|')
		.map((cell) => cell.trim());
}

function parseAlignment(cell: string): MarkdownTableAlignment {
	const value = cell.trim();
	const left = value.startsWith(':');
	const right = value.endsWith(':');
	if (left && right) return 'center';
	if (right) return 'right';
	return 'left';
}

function delimiterForAlignment(alignment: MarkdownTableAlignment): string {
	if (alignment === 'center') return ':---:';
	if (alignment === 'right') return '---:';
	return '---';
}

function formatDelimiterCell(alignment: MarkdownTableAlignment, width: number): string {
	const delimiterWidth = Math.max(width, 3);
	if (alignment === 'center') return `:${'-'.repeat(delimiterWidth - 2)}:`;
	if (alignment === 'right') return `${'-'.repeat(delimiterWidth - 1)}:`;
	return '-'.repeat(delimiterWidth);
}

function normalizeRow(row: string[], columns: number): string[] {
	const normalized = row.slice(0, columns);
	while (normalized.length < columns) {
		normalized.push('');
	}
	return normalized;
}

export function parseMarkdownTable(rawText: string): MarkdownTable {
	const lines = rawText
		.split(/\r?\n/)
		.map((line) => line.trim())
		.filter(Boolean);

	if (lines.length < 2) {
		return { headers: [], alignments: [], rows: [] };
	}

	const headers = parseCells(lines[0]);
	const delimiterCells = parseCells(lines[1]);
	const rows = lines.slice(2).map(parseCells);
	const columns = Math.max(headers.length, delimiterCells.length, ...rows.map((row) => row.length));

	return {
		headers: normalizeRow(headers, columns),
		alignments: normalizeRow(delimiterCells, columns).map(parseAlignment),
		rows: rows.map((row) => normalizeRow(row, columns))
	};
}

export function serializeMarkdownTable(table: MarkdownTable): string {
	const columns = Math.max(
		table.headers.length,
		table.alignments.length,
		...table.rows.map((row) => row.length)
	);
	const headers = normalizeRow(table.headers, columns);
	const alignments = normalizeRow(table.alignments, columns) as MarkdownTableAlignment[];
	const rows = table.rows.map((row) => normalizeRow(row, columns));
	const line = (cells: string[]) => `| ${cells.join(' | ')} |`;

	return [
		line(headers),
		line(alignments.map(delimiterForAlignment)),
		...rows.map((row) => line(row))
	].join('\n');
}

export function formatTable(table: MarkdownTable): string {
	const columns = Math.max(
		table.headers.length,
		table.alignments.length,
		...table.rows.map((row) => row.length)
	);
	const headers = normalizeRow(table.headers, columns);
	const alignments = normalizeRow(table.alignments, columns) as MarkdownTableAlignment[];
	const rows = table.rows.map((row) => normalizeRow(row, columns));
	const widths = headers.map((header, columnIndex) =>
		Math.max(header.length, ...rows.map((row) => row[columnIndex].length))
	);
	const formatRow = (cells: string[]) =>
		`| ${cells.map((cell, columnIndex) => cell.padEnd(widths[columnIndex], ' ')).join(' | ')} |`;

	return [
		formatRow(headers),
		`| ${alignments.map((alignment, columnIndex) => formatDelimiterCell(alignment, widths[columnIndex])).join(' | ')} |`,
		...rows.map((row) => formatRow(row))
	].join('\n');
}

export function updateMarkdownTableCell(
	table: MarkdownTable,
	update: MarkdownTableCellUpdate
): MarkdownTable {
	const headers = [...table.headers];
	const rows = table.rows.map((row) => [...row]);

	if (update.section === 'header') {
		headers[update.columnIndex] = update.value;
	} else if (rows[update.rowIndex]) {
		rows[update.rowIndex][update.columnIndex] = update.value;
	}

	return {
		headers,
		alignments: [...table.alignments],
		rows
	};
}

export function addMarkdownTableRow(table: MarkdownTable): MarkdownTable {
	return {
		headers: [...table.headers],
		alignments: [...table.alignments],
		rows: [...table.rows.map((row) => [...row]), table.headers.map(() => '')]
	};
}

export function addMarkdownTableColumn(table: MarkdownTable): MarkdownTable {
	const nextColumnNumber = table.headers.length + 1;
	return {
		headers: [...table.headers, `Column ${nextColumnNumber}`],
		alignments: [...table.alignments, 'left'],
		rows: table.rows.map((row) => [...row, ''])
	};
}

export function removeMarkdownTableRow(table: MarkdownTable): MarkdownTable {
	return {
		headers: [...table.headers],
		alignments: [...table.alignments],
		rows: table.rows.slice(0, -1).map((row) => [...row])
	};
}

export function removeMarkdownTableColumn(table: MarkdownTable): MarkdownTable {
	if (table.headers.length <= 1) {
		return {
			headers: [...table.headers],
			alignments: [...table.alignments],
			rows: table.rows.map((row) => [...row])
		};
	}

	return {
		headers: table.headers.slice(0, -1),
		alignments: table.alignments.slice(0, -1),
		rows: table.rows.map((row) => row.slice(0, -1))
	};
}

export function insertRowAt(table: MarkdownTable, index: number): MarkdownTable {
	const columns = table.headers.length;
	const blank = Array.from({ length: columns }, () => '');
	const rows = table.rows.map((row) => [...row]);
	const clamped = Math.max(0, Math.min(index, rows.length));
	rows.splice(clamped, 0, blank);
	return { headers: [...table.headers], alignments: [...table.alignments], rows };
}

export function deleteRowAt(table: MarkdownTable, index: number): MarkdownTable {
	if (index < 0 || index >= table.rows.length) return { ...table, rows: table.rows.map((r) => [...r]) };
	const rows = table.rows.filter((_, i) => i !== index).map((r) => [...r]);
	return { headers: [...table.headers], alignments: [...table.alignments], rows };
}

export function moveRow(table: MarkdownTable, from: number, to: number): MarkdownTable {
	if (from < 0 || from >= table.rows.length || to < 0 || to >= table.rows.length || from === to) {
		return { ...table, rows: table.rows.map((r) => [...r]) };
	}
	const rows = table.rows.map((r) => [...r]);
	const [moved] = rows.splice(from, 1);
	rows.splice(to, 0, moved);
	return { headers: [...table.headers], alignments: [...table.alignments], rows };
}

export function duplicateRow(table: MarkdownTable, index: number): MarkdownTable {
	if (index < 0 || index >= table.rows.length) return { ...table, rows: table.rows.map((r) => [...r]) };
	const rows = table.rows.map((r) => [...r]);
	rows.splice(index + 1, 0, [...table.rows[index]]);
	return { headers: [...table.headers], alignments: [...table.alignments], rows };
}

export function insertColumnAt(table: MarkdownTable, index: number): MarkdownTable {
	const clamped = Math.max(0, Math.min(index, table.headers.length));
	const headers = [...table.headers];
	headers.splice(clamped, 0, '');
	const alignments = [...table.alignments];
	alignments.splice(clamped, 0, 'left');
	const rows = table.rows.map((row) => {
		const r = [...row];
		r.splice(clamped, 0, '');
		return r;
	});
	return { headers, alignments, rows };
}

export function deleteColumnAt(table: MarkdownTable, index: number): MarkdownTable {
	if (table.headers.length <= 1 || index < 0 || index >= table.headers.length) {
		return { headers: [...table.headers], alignments: [...table.alignments], rows: table.rows.map((r) => [...r]) };
	}
	const headers = table.headers.filter((_, i) => i !== index);
	const alignments = table.alignments.filter((_, i) => i !== index);
	const rows = table.rows.map((row) => row.filter((_, i) => i !== index));
	return { headers, alignments, rows };
}

export function moveColumn(table: MarkdownTable, from: number, to: number): MarkdownTable {
	if (from < 0 || from >= table.headers.length || to < 0 || to >= table.headers.length || from === to) {
		return { headers: [...table.headers], alignments: [...table.alignments], rows: table.rows.map((r) => [...r]) };
	}
	const headers = [...table.headers];
	const [movedH] = headers.splice(from, 1);
	headers.splice(to, 0, movedH);
	const alignments = [...table.alignments];
	const [movedA] = alignments.splice(from, 1);
	alignments.splice(to, 0, movedA);
	const rows = table.rows.map((row) => {
		const r = [...row];
		const [movedC] = r.splice(from, 1);
		r.splice(to, 0, movedC);
		return r;
	});
	return { headers, alignments, rows };
}

export function duplicateColumn(table: MarkdownTable, index: number): MarkdownTable {
	if (index < 0 || index >= table.headers.length) {
		return { headers: [...table.headers], alignments: [...table.alignments], rows: table.rows.map((r) => [...r]) };
	}
	const headers = [...table.headers];
	headers.splice(index + 1, 0, table.headers[index]);
	const alignments = [...table.alignments];
	alignments.splice(index + 1, 0, table.alignments[index]);
	const rows = table.rows.map((row) => {
		const r = [...row];
		r.splice(index + 1, 0, row[index] ?? '');
		return r;
	});
	return { headers, alignments, rows };
}

export function setColumnAlignment(
	table: MarkdownTable,
	index: number,
	alignment: MarkdownTableAlignment
): MarkdownTable {
	if (index < 0 || index >= table.alignments.length) {
		return { headers: [...table.headers], alignments: [...table.alignments], rows: table.rows.map((r) => [...r]) };
	}
	const alignments = [...table.alignments];
	alignments[index] = alignment;
	return { headers: [...table.headers], alignments, rows: table.rows.map((r) => [...r]) };
}
