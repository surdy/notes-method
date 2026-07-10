import type { Command } from './commands';

/**
 * Logic for the command palette's empty-query state (top-5 review item #3).
 * When the user opens the command palette without typing, we show recommended
 * commands grouped by category plus a "Recent" group, instead of an empty
 * "No matches" list. Kept separate from the Svelte view for unit testing.
 */

export interface GroupedCommand {
	command: Command;
	/** Section label this command is listed under, e.g. "Recent" or "Notes". */
	group: string;
}

/** Fixed display order for category groups; unknown categories are appended. */
export const CATEGORY_ORDER: Command['category'][] = [
	'Notes',
	'Navigation',
	'Tasks',
	'Templates',
	'AI',
	'Appearance',
	'Vault',
	'Settings'
];

const RECENT_GROUP = 'Recent';

/**
 * Build the ordered, grouped command list shown when the palette query is
 * empty: recently-used commands first (capped at `recentLimit`), then every
 * remaining command bucketed by `CATEGORY_ORDER`. Each command appears once.
 */
export function orderEmptyCommands(
	commands: Command[],
	recentIds: string[],
	recentLimit: number
): GroupedCommand[] {
	const byId = new Map(commands.map((command) => [command.id, command]));
	const used = new Set<string>();
	const out: GroupedCommand[] = [];

	for (const id of recentIds) {
		if (out.filter((entry) => entry.group === RECENT_GROUP).length >= recentLimit) break;
		const command = byId.get(id);
		if (command && !used.has(id)) {
			out.push({ command, group: RECENT_GROUP });
			used.add(id);
		}
	}

	const categories: Command['category'][] = [
		...CATEGORY_ORDER,
		...commands
			.map((command) => command.category)
			.filter((category) => !CATEGORY_ORDER.includes(category))
	];

	for (const category of categories) {
		for (const command of commands) {
			if (command.category === category && !used.has(command.id)) {
				out.push({ command, group: category });
				used.add(command.id);
			}
		}
	}

	return out;
}

/**
 * Given the grouped list, return the header label to render before the item at
 * `index`, or `null` when the item continues the previous group.
 */
export function groupHeaderAt(order: GroupedCommand[], index: number): string | null {
	const current = order[index];
	if (!current) return null;
	if (index === 0) return current.group;
	return order[index - 1].group === current.group ? null : current.group;
}
