import type { NoteSummary } from './api';

export function noteIcon(note: NoteSummary): string {
	return configuredNoteIcon(note) ?? '📄';
}

export function configuredNoteIcon(note: NoteSummary): string | undefined {
	return readString(note.frontmatter?._icon);
}

function readString(value: unknown): string | undefined {
	return typeof value === 'string' && value.length > 0 ? value : undefined;
}
