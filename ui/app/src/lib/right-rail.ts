import type { NoteSummary } from './api';

export type RailMetadata = Record<string, unknown>;

export function escapeSqlLiteral(value: string): string {
	return value.replace(/'/g, "''");
}

export function pathStem(path: string): string {
	const filename = path.split('/').pop() ?? path;
	return filename.replace(/\.md$/i, '');
}

export function buildBacklinksQuery(path: string): string {
	const stem = escapeSqlLiteral(pathStem(path));
	return `SELECT DISTINCT b.source_path, COALESCE(b.source_title, b.source_path) AS source_title FROM v_backlinks b WHERE b.target_path = '${stem}' ORDER BY source_title`;
}

export function buildOutgoingLinksQuery(path: string): string {
	return `SELECT DISTINCT b.target_path, COALESCE(n.title, b.target_path) AS target FROM v_backlinks b LEFT JOIN v_notes n ON n.path = b.target_path WHERE b.source_path = '${escapeSqlLiteral(path)}' ORDER BY target`;
}

export function buildRailMetadata(
	note: NoteSummary | null | undefined,
	frontmatter: Record<string, unknown> | null | undefined
): RailMetadata | null {
	const metadata: RailMetadata = {};
	const tags = readTags(frontmatter?.tags ?? note?.tags);

	if (tags.length > 0) {
		metadata.tags = tags;
	}
	if (frontmatter) {
		for (const [key, value] of Object.entries(frontmatter)) {
			if (key.startsWith('_') || key === 'tags') {
				continue;
			}

			const normalized = normalizeMetadataValue(value);
			if (normalized !== undefined) {
				metadata[key] = normalized;
			}
		}
	}

	return Object.keys(metadata).length > 0 ? metadata : null;
}

export function isDashboardNote(frontmatter: Record<string, unknown> | null | undefined): boolean {
	return readString(frontmatter?.type) === 'dashboard';
}

function readString(value: unknown): string | undefined {
	return typeof value === 'string' && value.length > 0 ? value : undefined;
}
function readTags(value: unknown): string[] {
	if (typeof value === 'string' && value.length > 0) {
		return [value];
	}

	if (!Array.isArray(value)) {
		return [];
	}

	return value.filter((tag): tag is string => typeof tag === 'string' && tag.length > 0);
}

function normalizeMetadataValue(value: unknown): unknown {
	if (value === null || value === undefined) {
		return undefined;
	}

	if (typeof value === 'string') {
		return value.length > 0 ? value : undefined;
	}

	if (Array.isArray(value)) {
		const filtered = value.filter((item) => item !== null && item !== undefined && item !== '');
		return filtered.length > 0 ? filtered : undefined;
	}

	return value;
}
