import type { TemplatePrompt } from './api/index.ts';
import type { InputStep } from './input-palette.svelte.ts';

/**
 * Turn a template's declared prompts into input-palette steps.
 *
 * `type: field-picker` prompts become a searchable list of values already in the
 * vault (via `fields.toml`'s `values` / `suggest_from`), while still accepting a
 * typed-in value — the first meeting with a new customer has to be able to name
 * one. Every other type is a plain text input.
 *
 * Suggestions are a convenience, never a gate: if the lookup fails or returns
 * nothing, the prompt degrades to text rather than blocking note creation.
 *
 * Kept as pure logic (suggestions arrive through `fetchSuggestions`) so it can
 * be unit-tested without a daemon.
 */
export async function buildPromptSteps(
	prompts: TemplatePrompt[],
	fetchSuggestions: (field: string) => Promise<string[]>
): Promise<InputStep[]> {
	return Promise.all(prompts.map((prompt) => buildStep(prompt, fetchSuggestions)));
}

async function buildStep(
	prompt: TemplatePrompt,
	fetchSuggestions: (field: string) => Promise<string[]>
): Promise<InputStep> {
	const label = `${prompt.name}${prompt.required ? '' : ' (optional)'}`;

	if (prompt.type !== 'field-picker') {
		return {
			mode: 'text',
			label,
			placeholder: `Enter ${prompt.name}...`,
			required: prompt.required
		};
	}

	const field = prompt.field ?? prompt.name;
	let suggestions: string[] = [];
	try {
		suggestions = await fetchSuggestions(field);
	} catch (error) {
		console.warn(`Could not load suggestions for ${field}; falling back to text input.`, error);
	}

	if (suggestions.length === 0) {
		return {
			mode: 'text',
			label,
			placeholder: `Enter ${prompt.name}...`,
			required: prompt.required
		};
	}

	return {
		mode: 'list',
		label,
		items: suggestions.map((value) => {
			const bare = stripWikilink(value);
			return { id: bare, label: bare };
		}),
		placeholder: `Search ${field}, or type a new one...`,
		allowCustom: true
	};
}

/**
 * Suggestions come from stored frontmatter, so wikilink list fields arrive
 * already wrapped (`[[Acme Corp]]`). Submit the bare name: templates re-wrap
 * with `as_wikilink`, and passing the wrapped form through would yield
 * `[[[[Acme Corp]]]]`. Non-wikilink fields (`priority`) pass through untouched.
 */
function stripWikilink(value: string): string {
	const wikilink = value.match(/^\[\[(.+)\]\]$/);
	return wikilink ? wikilink[1] : value;
}
