/**
 * Pure helpers for the persona/customization layer (issues #210/#212, ADR 0016).
 *
 * Kept DOM- and store-free so preamble assembly and `@mention` parsing are
 * unit-testable in isolation.
 */

import type { CustomAgent, Instruction } from '../api/customizations.ts';

/** Upper bound on the assembled preamble, mirroring the Rust-side cap. */
export const PREAMBLE_CAP = 8000;

/**
 * Assemble the one-time session preamble from always-on discovered instructions
 * and the active persona's body. Instructions come first (global guidance), then
 * the persona (the active role). Empty bodies are dropped; the result is trimmed
 * and capped at {@link PREAMBLE_CAP}. Returns `null` when there is nothing to
 * inject, so the caller can omit the field entirely.
 */
export function assembleSessionPreamble(
	instructions: Instruction[],
	persona: CustomAgent | null
): string | null {
	const sections: string[] = [];
	for (const instruction of instructions) {
		const body = instruction.body.trim();
		if (body) sections.push(body);
	}
	if (persona) {
		const body = persona.body.trim();
		if (body) sections.push(body);
	}
	if (sections.length === 0) return null;
	const joined = sections.join('\n\n').trim();
	if (!joined) return null;
	return joined.length > PREAMBLE_CAP ? joined.slice(0, PREAMBLE_CAP) : joined;
}

/**
 * Parse a leading `@persona-id` mention from composer input (issue #212).
 *
 * Recognizes a mention only at the very start of the input (after optional
 * whitespace): `@<id> <rest>`. The id matches a discovered persona id by an
 * `[A-Za-z0-9._-]+` token. Returns the matched `id` and the `rest` of the
 * message with the mention stripped, or `null` when there is no leading mention.
 * Routing is session-switch (ADR 0016 decision 4): the caller switches the
 * active persona and sends `rest` (which may be empty — a bare switch).
 */
export function parseAgentMention(input: string): { id: string; rest: string } | null {
	const match = /^\s*@([A-Za-z0-9._-]+)(?:\s+([\s\S]*))?$/.exec(input);
	if (!match) return null;
	return { id: match[1], rest: (match[2] ?? '').trim() };
}
