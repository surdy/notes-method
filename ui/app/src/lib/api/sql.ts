import { API_BASE } from './core';

export interface SqlQueryResult {
	columns: string[];
	rows: Record<string, unknown>[];
}

interface RawSqlQueryResult {
	columns: string[];
	rows: unknown[][];
	row_count: number;
	truncated: boolean;
}

export async function executeSql(vault: string, sql: string): Promise<SqlQueryResult> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/query/sql`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ sql })
	});
	if (!res.ok) throw new Error(`SQL query failed: ${res.status}`);

	const raw = (await res.json()) as RawSqlQueryResult;
	return {
		columns: raw.columns,
		rows: raw.rows.map((values) =>
			Object.fromEntries(raw.columns.map((column, index) => [column, values[index]]))
		)
	};
}
