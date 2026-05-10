import type { NoteSummary } from './api';

export type RailMetadata = Record<string, unknown>;

export function escapeSqlLiteral(value: string): string {
	return value.replace(/'/g, "''");
}

export function buildBacklinksQuery(path: string): string {
	return `SELECT source_path, source_title FROM v_backlinks WHERE target_path = '${escapeSqlLiteral(path)}' ORDER BY source_title`;
}

export function buildOutgoingLinksQuery(path: string): string {
	return `SELECT target_path, target FROM v_backlinks WHERE source_path = '${escapeSqlLiteral(path)}' ORDER BY target`;
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
