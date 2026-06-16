import { describe, expect, it } from 'vitest';

import { assembleSessionPreamble, parseAgentMention, PREAMBLE_CAP } from './persona.ts';
import type { CustomAgent, Instruction } from '../api/customizations.ts';

function instruction(id: string, body: string): Instruction {
	return { id, name: id, description: '', body, source: 'project' };
}

function persona(id: string, body: string): CustomAgent {
	return { id, name: id, description: '', backend: null, model: null, body, source: 'project' };
}

describe('assembleSessionPreamble', () => {
	it('returns null when there is nothing to inject', () => {
		expect(assembleSessionPreamble([], null)).toBeNull();
		expect(assembleSessionPreamble([instruction('a', '   ')], persona('p', ''))).toBeNull();
	});

	it('puts instructions before the persona body', () => {
		const out = assembleSessionPreamble(
			[instruction('tone', 'Be concise.')],
			persona('researcher', 'You are a researcher.')
		);
		expect(out).toBe('Be concise.\n\nYou are a researcher.');
	});

	it('joins multiple instructions and drops empty bodies', () => {
		const out = assembleSessionPreamble(
			[instruction('a', 'First.'), instruction('b', '  '), instruction('c', 'Third.')],
			null
		);
		expect(out).toBe('First.\n\nThird.');
	});

	it('caps the assembled preamble length', () => {
		const huge = 'x'.repeat(PREAMBLE_CAP + 500);
		const out = assembleSessionPreamble([], persona('p', huge));
		expect(out).not.toBeNull();
		expect(out!.length).toBe(PREAMBLE_CAP);
	});
});

describe('parseAgentMention', () => {
	it('parses a leading mention and the remaining text', () => {
		expect(parseAgentMention('@researcher summarize this note')).toEqual({
			id: 'researcher',
			rest: 'summarize this note'
		});
	});

	it('tolerates leading whitespace', () => {
		expect(parseAgentMention('   @editor fix grammar')).toEqual({
			id: 'editor',
			rest: 'fix grammar'
		});
	});

	it('treats a bare mention as a switch with empty rest', () => {
		expect(parseAgentMention('@researcher')).toEqual({ id: 'researcher', rest: '' });
	});

	it('accepts dot/dash/underscore in ids', () => {
		expect(parseAgentMention('@my-agent.v2 hi')).toEqual({ id: 'my-agent.v2', rest: 'hi' });
	});

	it('returns null when there is no leading mention', () => {
		expect(parseAgentMention('hello @researcher')).toBeNull();
		expect(parseAgentMention('no mention here')).toBeNull();
		expect(parseAgentMention('@')).toBeNull();
	});
});
