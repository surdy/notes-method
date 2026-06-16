/**
 * Pure, framework-free logic for the chat composer's `/` slash-command palette.
 *
 * A slash command is just a saved {@link Prompt} (a built-in default or a vault
 * `_prompts/` override, merged by the backend — see issue #193) surfaced in the
 * composer. Keeping this module DOM- and Svelte-free makes parsing, filtering,
 * and the prompts→commands mapping trivial to unit-test.
 */

import type { Prompt } from '../api/prompts.ts';

/** A command shown in the palette. Mirrors {@link Prompt} shape. */
export interface SlashCommand {
	name: string;
	description: string;
	body: string;
	source: 'default' | 'vault';
}

/** Result of inspecting the composer value for an active slash query. */
export interface SlashQuery {
	/** True when the palette should be open for the current input. */
	active: boolean;
	/** The partial command name typed after the leading `/` (lowercased). */
	query: string;
}

/** Map the merged prompt list to palette commands (1:1, preserving order). */
export function slashCommandsFromPrompts(prompts: Prompt[]): SlashCommand[] {
	return prompts.map((p) => ({
		name: p.name,
		description: p.description,
		body: p.body,
		source: p.source
	}));
}

/**
 * Decide whether the composer value activates the slash palette and extract the
 * partial command name.
 *
 * Rule (kept deliberately simple and unambiguous): the palette is active only
 * when the *entire* input begins with a single `/` and contains no whitespace
 * yet — i.e. the user is still typing the command token. As soon as a space (or
 * newline) is typed, the input is treated as a real message and the palette
 * closes. A bare `/` yields an empty query (show all commands).
 *
 * Examples:
 *   ""        -> { active: false, query: '' }
 *   "/"       -> { active: true,  query: '' }
 *   "/sum"    -> { active: true,  query: 'sum' }
 *   "/sum ab" -> { active: false, query: '' }  (space ends the token)
 *   "hi /sum" -> { active: false, query: '' }  (must start at position 0)
 */
export function parseSlashQuery(input: string): SlashQuery {
	if (!input.startsWith('/')) return { active: false, query: '' };
	const token = input.slice(1);
	if (/\s/.test(token)) return { active: false, query: '' };
	return { active: true, query: token.toLowerCase() };
}

/**
 * Filter commands by a query (case-insensitive). Prefix matches rank above
 * substring matches; within each tier original order is preserved (stable), so
 * the backend's ordering — and thus vault-vs-default precedence — is respected.
 * An empty query returns every command in its original order.
 */
export function filterSlashCommands(commands: SlashCommand[], query: string): SlashCommand[] {
	const q = query.trim().toLowerCase();
	if (!q) return [...commands];

	const prefix: SlashCommand[] = [];
	const substring: SlashCommand[] = [];
	for (const cmd of commands) {
		const name = cmd.name.toLowerCase();
		if (name.startsWith(q)) prefix.push(cmd);
		else if (name.includes(q)) substring.push(cmd);
	}
	return [...prefix, ...substring];
}
