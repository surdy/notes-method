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
