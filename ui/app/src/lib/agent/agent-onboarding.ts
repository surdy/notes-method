/**
 * Suggested prompts and onboarding copy for the agent/chat panel (UX review
 * item: "Agent/chat onboarding lacks task examples and fallback help").
 *
 * Kept as pure logic so the suggestion set can be unit-tested and the Svelte
 * view stays thin.
 */

export interface PromptSuggestion {
	/** Short label shown on the clickable chip. */
	label: string;
	/** Full text inserted into the composer when the chip is clicked. */
	prompt: string;
	/**
	 * When true the inserted prompt is incomplete and expects the user to keep
	 * typing (the trailing text is a lead-in). The view can keep focus in the
	 * composer with the caret at the end.
	 */
	partial?: boolean;
}

/**
 * Example prompts to seed an empty conversation. When a note is active the
 * suggestions operate on that note; otherwise they are vault-level.
 */
export function suggestedPrompts(activeNoteTitle?: string | null): PromptSuggestion[] {
	const title = activeNoteTitle?.trim();
	if (title) {
		return [
			{ label: 'Summarize this note', prompt: `Summarize "${title}" in a few bullet points.` },
			{
				label: 'Find related notes',
				prompt: `Find notes related to "${title}" and explain how they connect.`
			},
			{
				label: 'Suggest next steps',
				prompt: `Based on "${title}", what follow-up tasks or open questions should I capture?`
			},
			{
				label: 'Improve the writing',
				prompt: `Suggest edits to improve the clarity and structure of "${title}".`
			}
		];
	}
	return [
		{ label: 'Summarize recent notes', prompt: 'Summarize the notes I added most recently.' },
		{ label: 'Search a topic', prompt: 'What have I written about ', partial: true },
		{ label: 'Surface open tasks', prompt: 'List the open tasks across my vault, grouped by note.' },
		{ label: 'Draft a note', prompt: 'Help me draft a new note about ', partial: true }
	];
}
