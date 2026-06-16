import { describe, expect, it } from 'vitest';

import type { Prompt } from '../api/prompts.ts';
import {
	filterSlashCommands,
	parseSlashQuery,
	slashCommandsFromPrompts,
	type SlashCommand
} from './slash-commands.ts';

function prompt(name: string, source: 'default' | 'vault' = 'default', body?: string): Prompt {
	return {
		name,
		description: `${name} description`,
		body: body ?? `Body for ${name}`,
		source
	};
}

const DEFAULT_NAMES = [
	'summarize',
	'rewrite',
	'outline',
	'fix',
	'tags',
	'links',
	'daily',
	'new',
	'ask'
];

describe('slashCommandsFromPrompts', () => {
	it('maps prompts to commands 1:1, preserving order and fields', () => {
		const prompts = DEFAULT_NAMES.map((n) => prompt(n));
		const cmds = slashCommandsFromPrompts(prompts);
		expect(cmds.map((c) => c.name)).toEqual(DEFAULT_NAMES);
		expect(cmds[0]).toEqual({
			name: 'summarize',
			description: 'summarize description',
			body: 'Body for summarize',
			source: 'default'
		});
	});

	it('includes vault prompts alongside defaults', () => {
		const prompts = [prompt('summarize', 'default'), prompt('weekly', 'vault')];
		const cmds = slashCommandsFromPrompts(prompts);
		expect(cmds).toHaveLength(2);
		expect(cmds.find((c) => c.name === 'weekly')?.source).toBe('vault');
	});

	it('returns an empty list for no prompts', () => {
		expect(slashCommandsFromPrompts([])).toEqual([]);
	});
});

describe('parseSlashQuery', () => {
	it('is inactive for empty input', () => {
		expect(parseSlashQuery('')).toEqual({ active: false, query: '' });
	});

	it('is inactive when input does not start with a slash', () => {
		expect(parseSlashQuery('hello')).toEqual({ active: false, query: '' });
		expect(parseSlashQuery('hi /sum')).toEqual({ active: false, query: '' });
	});

	it('activates on a bare slash with an empty query', () => {
		expect(parseSlashQuery('/')).toEqual({ active: true, query: '' });
	});

	it('extracts the partial command name after the slash, lowercased', () => {
		expect(parseSlashQuery('/sum')).toEqual({ active: true, query: 'sum' });
		expect(parseSlashQuery('/SUM')).toEqual({ active: true, query: 'sum' });
	});

	it('closes once whitespace is typed after the token', () => {
		expect(parseSlashQuery('/summarize ')).toEqual({ active: false, query: '' });
		expect(parseSlashQuery('/summarize the note')).toEqual({ active: false, query: '' });
		expect(parseSlashQuery('/sum\n')).toEqual({ active: false, query: '' });
	});
});

describe('filterSlashCommands', () => {
	const commands: SlashCommand[] = slashCommandsFromPrompts(DEFAULT_NAMES.map((n) => prompt(n)));

	it('returns all commands for an empty query', () => {
		expect(filterSlashCommands(commands, '')).toHaveLength(DEFAULT_NAMES.length);
		expect(filterSlashCommands(commands, '   ')).toHaveLength(DEFAULT_NAMES.length);
	});

	it('matches by case-insensitive prefix', () => {
		expect(filterSlashCommands(commands, 'su').map((c) => c.name)).toEqual(['summarize']);
		expect(filterSlashCommands(commands, 'SU').map((c) => c.name)).toEqual(['summarize']);
	});

	it('ranks prefix matches above substring matches', () => {
		const cmds = slashCommandsFromPrompts([
			prompt('tags'),
			prompt('stagger'),
			prompt('agenda')
		]);
		// query "ag": prefix "agenda" first, then substrings "tags", "stagger".
		expect(filterSlashCommands(cmds, 'ag').map((c) => c.name)).toEqual([
			'agenda',
			'tags',
			'stagger'
		]);
	});

	it('preserves original order within a tier (stable, respects backend precedence)', () => {
		const cmds = slashCommandsFromPrompts([
			prompt('daily', 'vault'),
			prompt('draft', 'default')
		]);
		expect(filterSlashCommands(cmds, 'd').map((c) => c.name)).toEqual(['daily', 'draft']);
	});

	it('returns nothing when no command matches', () => {
		expect(filterSlashCommands(commands, 'zzz')).toEqual([]);
	});
});

describe('selection resolves to prompt body', () => {
	it('a selected command yields its body text to insert into the composer', () => {
		const prompts = [prompt('summarize', 'default', 'Summarize the current note.')];
		const cmds = slashCommandsFromPrompts(prompts);
		const filtered = filterSlashCommands(cmds, parseSlashQuery('/sum').query);
		expect(filtered).toHaveLength(1);
		const selected = filtered[0];
		expect(selected.body).toBe('Summarize the current note.');
	});
});
