/**
 * Pure catalogue + mapping for inline editor commands (issue #195). A right-click
 * menu and the command palette both expose these six actions on a text selection;
 * each one sends a concise static instruction to the shared chat agent and applies
 * the result back into the editor.
 *
 * The selection itself rides to the agent via {@link EditorContext.selection}, NOT
 * inside the instruction string — so the visible chat bubble stays short and the
 * mapping here is trivially unit-testable without a DOM.
 */

import type { ApplyMode } from '../editor/apply-output.ts';

export type InlineCommandId = 'rewrite' | 'summarize' | 'expand' | 'fix' | 'continue' | 'custom';

export interface InlineCommandDef {
	id: InlineCommandId;
	label: string;
	instruction: string;
	applyMode: ApplyMode;
}

/** The six inline commands, in display order. */
export const INLINE_COMMANDS: InlineCommandDef[] = [
	{
		id: 'rewrite',
		label: 'Rewrite',
		instruction:
			'Rewrite the selected text to improve clarity and flow while preserving meaning. Return only the rewritten text.',
		applyMode: 'replace'
	},
	{
		id: 'summarize',
		label: 'Summarize',
		instruction: 'Summarize the selected text concisely. Return only the summary.',
		applyMode: 'replace'
	},
	{
		id: 'expand',
		label: 'Expand',
		instruction: 'Expand the selected text with more detail. Return only the expanded text.',
		applyMode: 'replace'
	},
	{
		id: 'fix',
		label: 'Fix',
		instruction:
			'Fix spelling, grammar, and punctuation in the selected text without changing its meaning. Return only the corrected text.',
		applyMode: 'replace'
	},
	{
		id: 'continue',
		label: 'Continue writing',
		instruction:
			'Continue writing from the selected text in the same voice and style. Return only the continuation.',
		applyMode: 'insert'
	},
	{
		id: 'custom',
		label: 'Custom prompt…',
		instruction: '',
		applyMode: 'replace'
	}
];

function defFor(id: InlineCommandId): InlineCommandDef {
	const def = INLINE_COMMANDS.find((c) => c.id === id);
	if (!def) {
		throw new Error(`Unknown inline command: ${id}`);
	}
	return def;
}

/**
 * Resolve the instruction sent to the agent. For `custom`, the user-provided
 * prompt is used verbatim; for the rest, the static directive from the catalogue.
 */
export function instructionFor(id: InlineCommandId, customPrompt?: string): string {
	if (id === 'custom') {
		return (customPrompt ?? '').trim();
	}
	return defFor(id).instruction;
}

/** The apply mode for a command: `replace` for most, `insert` for `continue`. */
export function applyModeFor(id: InlineCommandId): ApplyMode {
	return defFor(id).applyMode;
}
