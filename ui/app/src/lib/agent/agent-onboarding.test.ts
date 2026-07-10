import { describe, expect, it } from 'vitest';
import { suggestedPrompts } from './agent-onboarding';

describe('suggestedPrompts', () => {
	it('returns note-focused suggestions embedding the active note title', () => {
		const prompts = suggestedPrompts('Weekly Review');
		expect(prompts).toHaveLength(4);
		expect(prompts.every((p) => p.prompt.includes('Weekly Review'))).toBe(true);
		expect(prompts[0].label).toBe('Summarize this note');
		expect(prompts.some((p) => p.partial)).toBe(false);
	});

	it('returns vault-level suggestions when no note is active', () => {
		for (const value of [undefined, null, '   ']) {
			const prompts = suggestedPrompts(value);
			expect(prompts).toHaveLength(4);
			expect(prompts[0].label).toBe('Summarize recent notes');
			// Lead-in prompts are flagged partial so the caret stays for the user.
			const partials = prompts.filter((p) => p.partial);
			expect(partials.length).toBeGreaterThan(0);
			expect(partials.every((p) => p.prompt.endsWith(' '))).toBe(true);
		}
	});

	it('trims whitespace-only titles down to the vault-level set', () => {
		expect(suggestedPrompts('  ')[0].label).toBe(suggestedPrompts(null)[0].label);
	});
});
