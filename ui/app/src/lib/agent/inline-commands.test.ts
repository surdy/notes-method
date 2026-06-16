import { describe, expect, it } from 'vitest';

import {
	INLINE_COMMANDS,
	applyModeFor,
	instructionFor,
	type InlineCommandId
} from './inline-commands.ts';

describe('inline-commands catalogue', () => {
	it('exposes all six commands in order', () => {
		expect(INLINE_COMMANDS.map((c) => c.id)).toEqual([
			'rewrite',
			'summarize',
			'expand',
			'fix',
			'continue',
			'custom'
		]);
	});

	it('maps replace for editing commands and insert for continue', () => {
		const modes = Object.fromEntries(INLINE_COMMANDS.map((c) => [c.id, c.applyMode]));
		expect(modes).toEqual({
			rewrite: 'replace',
			summarize: 'replace',
			expand: 'replace',
			fix: 'replace',
			continue: 'insert',
			custom: 'replace'
		});
	});

	it('gives every built-in command a non-empty static instruction', () => {
		for (const cmd of INLINE_COMMANDS) {
			if (cmd.id === 'custom') continue;
			expect(cmd.instruction.length).toBeGreaterThan(0);
		}
	});
});

describe('instructionFor', () => {
	it('returns the static instruction for built-in commands', () => {
		expect(instructionFor('summarize')).toBe(
			'Summarize the selected text concisely. Return only the summary.'
		);
	});

	it('returns the trimmed custom prompt for the custom command', () => {
		expect(instructionFor('custom', '  Make it formal  ')).toBe('Make it formal');
	});

	it('returns an empty string for a custom command with no prompt', () => {
		expect(instructionFor('custom')).toBe('');
	});
});

describe('applyModeFor', () => {
	it('returns the catalogue apply mode for each id', () => {
		const ids: InlineCommandId[] = ['rewrite', 'summarize', 'expand', 'fix', 'continue', 'custom'];
		expect(ids.map(applyModeFor)).toEqual([
			'replace',
			'replace',
			'replace',
			'replace',
			'insert',
			'replace'
		]);
	});
});
