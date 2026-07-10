/**
 * Logic for the editor "Workspace Start" surface shown when no note is open
 * (actionable empty state). Kept separate from the Svelte view so it can be
 * unit-tested without a component render harness.
 */

export interface RecentSource {
	path: string;
	title: string;
	/** Epoch milliseconds when the note was last viewed, if known. */
	timestamp?: number;
	/** Backend `updated_at` string used as a fallback ordering/label. */
	updatedAt?: string;
}

export interface RecentEntry {
	path: string;
	title: string;
	/** Human-friendly "when", e.g. "2m ago", "yesterday", or a date. */
	label: string;
}

/**
 * A quick-start action shown as a card on the empty editor surface. `command`
 * is a command id from `buildCommands`, except the special `quick-switcher`
 * value which opens the file palette (page-level state).
 */
export interface StartAction {
	command: 'new-note' | 'open-daily' | 'capture' | 'quick-switcher';
	label: string;
	icon: string;
	shortcut: string;
	primary?: boolean;
}

export const START_ACTIONS: StartAction[] = [
	{ command: 'new-note', label: 'New note', icon: '＋', shortcut: '⌘N', primary: true },
	{ command: 'quick-switcher', label: 'Quick switcher', icon: '🔍', shortcut: '⌘O' },
	{ command: 'open-daily', label: "Today’s daily note", icon: '📅', shortcut: '⌘D' },
	{ command: 'capture', label: 'Quick capture', icon: '⚡', shortcut: '⌘⇧N' }
];

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec'
];

/**
 * Format a backend `updated_at` string ("YYYY-MM-DD HH:MM" or ISO) into a
 * short, timezone-stable "Mon D" label. Returns '' when unparseable so the
 * caller can omit the meta entirely rather than show noise.
 */
export function formatDateLabel(updatedAt: string): string {
	const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(updatedAt.trim());
	if (!match) return '';
	const month = Number(match[2]);
	const day = Number(match[3]);
	if (month < 1 || month > 12 || day < 1 || day > 31) return '';
	return `${MONTHS[month - 1]} ${day}`;
}

/**
 * Format a timestamp relative to `now`, degrading to an absolute date for
 * anything older than a week. Never throws on bad input.
 */
export function formatRelativeTime(timestamp: number, now: number = Date.now()): string {
	if (!Number.isFinite(timestamp)) return '';
	const diff = now - timestamp;
	if (diff < 0) return 'just now';
	if (diff < MINUTE) return 'just now';
	if (diff < HOUR) {
		const mins = Math.floor(diff / MINUTE);
		return `${mins}m ago`;
	}
	if (diff < DAY) {
		const hours = Math.floor(diff / HOUR);
		return `${hours}h ago`;
	}
	if (diff < 2 * DAY) return 'yesterday';
	if (diff < 7 * DAY) {
		const days = Math.floor(diff / DAY);
		return `${days}d ago`;
	}
	try {
		return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
	} catch {
		return '';
	}
}

/**
 * Merge recently-viewed entries (with precise timestamps) and recently-edited
 * entries (backend order) into a deduplicated, labelled list capped at `limit`.
 * Viewed entries win on ordering and label precision; edited entries fill any
 * remaining slots for a first run where nothing has been viewed yet.
 */
export function buildRecentList(
	viewed: RecentSource[],
	edited: RecentSource[],
	limit: number,
	now: number = Date.now()
): RecentEntry[] {
	const seen = new Set<string>();
	const out: RecentEntry[] = [];
	for (const item of [...viewed, ...edited]) {
		if (!item.path || seen.has(item.path)) continue;
		seen.add(item.path);
		out.push({
			path: item.path,
			title: item.title || item.path,
			label:
				typeof item.timestamp === 'number'
					? formatRelativeTime(item.timestamp, now)
					: formatDateLabel(item.updatedAt ?? '')
		});
		if (out.length >= limit) break;
	}
	return out;
}
