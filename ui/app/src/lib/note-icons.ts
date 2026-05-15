import type { NoteSummary } from './api';

const TYPE_ICONS: Record<string, string> = {
	daily: '📅',
	meeting: '🤝',
	customer: '🏢',
	stream: '🔀',
	note: '📝',
	'account-info': 'ℹ️',
	glossary: '📖',
	milestones: '🏁'
};

export function noteIcon(note: NoteSummary): string {
	const icon = readString(note.frontmatter?._icon);
	if (icon) {
		return icon;
	}

	return TYPE_ICONS[note.type] ?? '📄';
}

function readString(value: unknown): string | undefined {
	return typeof value === 'string' && value.length > 0 ? value : undefined;
}
