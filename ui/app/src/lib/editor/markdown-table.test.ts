import { describe, expect, it } from 'vitest';

import {
	addMarkdownTableColumn,
	addMarkdownTableRow,
	parseMarkdownTable,
	serializeMarkdownTable,
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
});
