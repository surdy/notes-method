export interface SqlBlock {
	sql: string;
	blockEnd: number;
}

export function isNotesmithSqlFenceInfo(info: string): boolean {
	const normalized = info.trim().toLowerCase().replace(/\s+/g, ' ');
	return normalized === 'notesmith' || normalized === 'notesmith sql';
}

export function findSqlBlocks(doc: string): SqlBlock[] {
	const blocks: SqlBlock[] = [];
	const lines = doc.split('\n');

	let currentBlockLines: string[] | null = null;
	let offset = 0;

	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index] ?? '';
		const trimmed = line.trim();

		if (currentBlockLines) {
			if (trimmed === '```') {
				const sql = currentBlockLines.join('\n').trim();
				if (sql.length > 0) {
					blocks.push({ sql, blockEnd: offset + line.length });
				}
				currentBlockLines = null;
			} else {
				currentBlockLines.push(line);
			}
		} else if (trimmed.startsWith('```')) {
			const info = trimmed.slice(3).trim();
			if (isNotesmithSqlFenceInfo(info)) {
				currentBlockLines = [];
			}
		}

		offset += line.length;
		if (index < lines.length - 1) {
			offset += 1;
		}
	}

	return blocks;
}
