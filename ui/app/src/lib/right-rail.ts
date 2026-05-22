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
	return `SELECT DISTINCT b.backlink_path, COALESCE(n.title, b.backlink_path) AS source_title FROM v_backlinks b LEFT JOIN v_notes n ON b.backlink_path = n.path WHERE b.note_path = '${stem}' ORDER BY source_title`;
}

export function buildOutgoingLinksQuery(path: string): string {
	return `SELECT DISTINCT COALESCE(n.path, b.note_path) AS target_path, COALESCE(n.title, b.note_path) AS target FROM v_backlinks b LEFT JOIN v_notes n ON n.title = b.note_path WHERE b.backlink_path = '${escapeSqlLiteral(path)}' ORDER BY target`;
}

export function buildRailMetadata(
	note: NoteSummary | null | undefined,
	frontmatter: Record<string, unknown> | null | undefined
): RailMetadata | null {
	const metadata: RailMetadata = {};
	const type = readString(frontmatter?.type) ?? note?.type;
	const customer = readString(frontmatter?.customer) ?? note?.customer;
	const date = readString(frontmatter?.date) ?? note?.date;
	const tags = readTags(frontmatter?.tags);

	if (type) {
		metadata.type = type;
	}
	if (customer) {
		metadata.customer = customer;
	}
	if (date) {
		metadata.date = date;
	}
	if (tags.length > 0) {
		metadata.tags = tags;
	}
	if (frontmatter) {
		const knownKeys = new Set(['type', 'customer', 'date', 'tags']);
		for (const [key, value] of Object.entries(frontmatter)) {
			if (key.startsWith('_') || knownKeys.has(key)) {
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
