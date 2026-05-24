import type { NoteSummary } from './api';

export function noteIcon(note: NoteSummary): string {
	const icon = readString(note.frontmatter?._icon);
	if (icon) {
		return icon;
	}

	return '📄';
}

function readString(value: unknown): string | undefined {
	return typeof value === 'string' && value.length > 0 ? value : undefined;
}
