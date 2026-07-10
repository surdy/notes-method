import { describe, expect, it } from 'vitest';
import type { Command } from './commands';
import { CATEGORY_ORDER, groupHeaderAt, orderEmptyCommands } from './command-palette';

function cmd(id: string, category: Command['category'], label = id): Command {
	return { id, label, category, execute: () => {} };
}

const commands: Command[] = [
	cmd('new-note', 'Notes'),
	cmd('capture', 'Notes'),
	cmd('open-daily', 'Navigation'),
	cmd('change-theme', 'Appearance'),
	cmd('ai-summarize', 'AI')
];

describe('orderEmptyCommands', () => {
	it('lists recents first, then remaining commands grouped by category order', () => {
		const result = orderEmptyCommands(commands, ['open-daily'], 5);
		expect(result.map((entry) => [entry.group, entry.command.id])).toEqual([
			['Recent', 'open-daily'],
			['Notes', 'new-note'],
			['Notes', 'capture'],
			['AI', 'ai-summarize'],
			['Appearance', 'change-theme']
		]);
	});

	it('does not duplicate a command that is also recent', () => {
		const result = orderEmptyCommands(commands, ['new-note'], 5);
		const ids = result.map((entry) => entry.command.id);
		expect(ids.filter((id) => id === 'new-note')).toHaveLength(1);
		expect(result[0]).toEqual({ command: commands[0], group: 'Recent' });
	});

	it('caps the Recent group at recentLimit', () => {
		const result = orderEmptyCommands(commands, ['new-note', 'capture', 'open-daily'], 2);
		expect(result.filter((entry) => entry.group === 'Recent')).toHaveLength(2);
	});

	it('includes every command exactly once', () => {
		const result = orderEmptyCommands(commands, ['ai-summarize'], 5);
		expect(result).toHaveLength(commands.length);
		expect(new Set(result.map((e) => e.command.id)).size).toBe(commands.length);
	});

	it('appends commands whose category is outside CATEGORY_ORDER', () => {
		const extra = [...commands, { id: 'weird', label: 'Weird', category: 'Zzz', execute() {} } as unknown as Command];
		const result = orderEmptyCommands(extra, [], 5);
		expect(result[result.length - 1].command.id).toBe('weird');
	});

	it('orders known categories per CATEGORY_ORDER', () => {
		const seen = orderEmptyCommands(commands, [], 5)
			.map((entry) => entry.group)
			.filter((group, i, arr) => arr.indexOf(group) === i);
		const indices = seen.map((group) => CATEGORY_ORDER.indexOf(group as Command['category']));
		expect(indices).toEqual([...indices].sort((a, b) => a - b));
	});
});

describe('groupHeaderAt', () => {
	it('returns a header only at group boundaries', () => {
		const order = orderEmptyCommands(commands, ['open-daily'], 5);
		expect(groupHeaderAt(order, 0)).toBe('Recent');
		expect(groupHeaderAt(order, 1)).toBe('Notes');
		expect(groupHeaderAt(order, 2)).toBeNull();
		expect(groupHeaderAt(order, 3)).toBe('AI');
	});

	it('returns null for out-of-range indices', () => {
		expect(groupHeaderAt([], 0)).toBeNull();
	});
});
