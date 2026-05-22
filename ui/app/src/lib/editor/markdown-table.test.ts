import { describe, expect, it } from 'vitest';

import {
	addMarkdownTableColumn,
	addMarkdownTableRow,
	deleteColumnAt,
	deleteRowAt,
	duplicateColumn,
	duplicateRow,
	insertColumnAt,
	insertRowAt,
	moveColumn,
	moveRow,
	parseMarkdownTable,
	removeMarkdownTableColumn,
	removeMarkdownTableRow,
	serializeMarkdownTable,
	setColumnAlignment,
	updateMarkdownTableCell
} from './markdown-table.ts';

describe('markdown table model', () => {
	it('parses and serializes aligned pipe tables', () => {
		const table = parseMarkdownTable(`| Name | Role | Start |
|:-----|:----:|------:|
| Jane | CTO | 2024 |
| John | VP | 2025 |`);

		expect(table).toEqual({
			headers: ['Name', 'Role', 'Start'],
			alignments: ['left', 'center', 'right'],
			rows: [
				['Jane', 'CTO', '2024'],
				['John', 'VP', '2025']
			]
		});

		expect(serializeMarkdownTable(table)).toBe(`| Name | Role | Start |
| --- | :---: | ---: |
| Jane | CTO | 2024 |
| John | VP | 2025 |`);
	});

	it('edits header and body cells while preserving table shape', () => {
		const table = parseMarkdownTable(`| Name | Role |
| --- | --- |
| Jane | CTO |`);

		const renamedHeader = updateMarkdownTableCell(table, {
			section: 'header',
			rowIndex: 0,
			columnIndex: 1,
			value: 'Title'
		});
		const editedBody = updateMarkdownTableCell(renamedHeader, {
			section: 'body',
			rowIndex: 0,
			columnIndex: 1,
			value: 'Chief Technology Officer'
		});

		expect(serializeMarkdownTable(editedBody)).toBe(`| Name | Title |
| --- | --- |
| Jane | Chief Technology Officer |`);
	});

	it('visually adds blank rows and columns to the markdown table', () => {
		const table = parseMarkdownTable(`| Name | Role |
| --- | --- |
| Jane | CTO |`);

		const withRow = addMarkdownTableRow(table);
		const withColumn = addMarkdownTableColumn(withRow);

		expect(withColumn).toEqual({
			headers: ['Name', 'Role', 'Column 3'],
			alignments: ['left', 'left', 'left'],
			rows: [
				['Jane', 'CTO', ''],
				['', '', '']
			]
		});
		expect(serializeMarkdownTable(withColumn)).toBe(`| Name | Role | Column 3 |
| --- | --- | --- |
| Jane | CTO |  |
|  |  |  |`);
	});

	it('visually removes the last row and column from the markdown table', () => {
		const table = parseMarkdownTable(`| Name | Role | Column 3 |
| --- | --- | --- |
| Jane | CTO |  |
|  |  |  |`);

		const withoutRow = removeMarkdownTableRow(table);
		const withoutColumn = removeMarkdownTableColumn(withoutRow);

		expect(serializeMarkdownTable(withoutColumn)).toBe(`| Name | Role |
| --- | --- |
| Jane | CTO |`);
	});

	it('keeps at least one column when removing columns visually', () => {
		const table = parseMarkdownTable(`| Name |
| --- |
| Jane |`);

		expect(serializeMarkdownTable(removeMarkdownTableColumn(table))).toBe(`| Name |
| --- |
| Jane |`);
	});

	it('inserts a row at a specific index', () => {
		const table = parseMarkdownTable(`| Name | Role |
| --- | --- |
| Jane | CTO |
| John | VP |`);

		const result = insertRowAt(table, 1);
		expect(serializeMarkdownTable(result)).toBe(`| Name | Role |
| --- | --- |
| Jane | CTO |
|  |  |
| John | VP |`);
	});

	it('inserts a row at the beginning', () => {
		const table = parseMarkdownTable(`| Name |
| --- |
| Jane |`);

		const result = insertRowAt(table, 0);
		expect(result.rows[0]).toEqual(['']);
		expect(result.rows[1]).toEqual(['Jane']);
	});

	it('deletes a row at a specific index', () => {
		const table = parseMarkdownTable(`| Name | Role |
| --- | --- |
| Jane | CTO |
| John | VP |`);

		const result = deleteRowAt(table, 0);
		expect(serializeMarkdownTable(result)).toBe(`| Name | Role |
| --- | --- |
| John | VP |`);
	});

	it('moves a row down', () => {
		const table = parseMarkdownTable(`| Name |
| --- |
| A |
| B |
| C |`);

		const result = moveRow(table, 0, 1);
		expect(result.rows.map((r) => r[0])).toEqual(['B', 'A', 'C']);
	});

	it('moves a row up', () => {
		const table = parseMarkdownTable(`| Name |
| --- |
| A |
| B |
| C |`);

		const result = moveRow(table, 2, 1);
		expect(result.rows.map((r) => r[0])).toEqual(['A', 'C', 'B']);
	});

	it('duplicates a row', () => {
		const table = parseMarkdownTable(`| Name | Role |
| --- | --- |
| Jane | CTO |
| John | VP |`);

		const result = duplicateRow(table, 0);
		expect(result.rows).toHaveLength(3);
		expect(result.rows[0]).toEqual(['Jane', 'CTO']);
		expect(result.rows[1]).toEqual(['Jane', 'CTO']);
		expect(result.rows[2]).toEqual(['John', 'VP']);
	});

	it('inserts a column at a specific index', () => {
		const table = parseMarkdownTable(`| A | B |
| --- | --- |
| 1 | 2 |`);

		const result = insertColumnAt(table, 1);
		expect(result.headers).toEqual(['A', '', 'B']);
		expect(result.rows[0]).toEqual(['1', '', '2']);
	});

	it('deletes a column at a specific index', () => {
		const table = parseMarkdownTable(`| A | B | C |
| --- | --- | --- |
| 1 | 2 | 3 |`);

		const result = deleteColumnAt(table, 1);
		expect(result.headers).toEqual(['A', 'C']);
		expect(result.rows[0]).toEqual(['1', '3']);
	});

	it('does not delete the last remaining column', () => {
		const table = parseMarkdownTable(`| A |
| --- |
| 1 |`);

		const result = deleteColumnAt(table, 0);
		expect(result.headers).toEqual(['A']);
	});

	it('moves a column left', () => {
		const table = parseMarkdownTable(`| A | B | C |
| --- | :---: | ---: |
| 1 | 2 | 3 |`);

		const result = moveColumn(table, 2, 1);
		expect(result.headers).toEqual(['A', 'C', 'B']);
		expect(result.alignments).toEqual(['left', 'right', 'center']);
		expect(result.rows[0]).toEqual(['1', '3', '2']);
	});

	it('moves a column right', () => {
		const table = parseMarkdownTable(`| A | B | C |
| --- | --- | --- |
| 1 | 2 | 3 |`);

		const result = moveColumn(table, 0, 1);
		expect(result.headers).toEqual(['B', 'A', 'C']);
		expect(result.rows[0]).toEqual(['2', '1', '3']);
	});

	it('duplicates a column', () => {
		const table = parseMarkdownTable(`| A | B |
| --- | :---: |
| 1 | 2 |`);

		const result = duplicateColumn(table, 0);
		expect(result.headers).toEqual(['A', 'A', 'B']);
		expect(result.alignments).toEqual(['left', 'left', 'center']);
		expect(result.rows[0]).toEqual(['1', '1', '2']);
	});

	it('sets column alignment', () => {
		const table = parseMarkdownTable(`| A | B |
| --- | --- |
| 1 | 2 |`);

		const result = setColumnAlignment(table, 1, 'center');
		expect(result.alignments).toEqual(['left', 'center']);
		expect(serializeMarkdownTable(result)).toContain(':---:');
	});
});
