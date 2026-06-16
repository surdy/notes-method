import { API_BASE, ApiError, apiFetch, readErrorBody } from './core.ts';

/**
 * Where a prompt came from. A `vault` prompt overrides a `default` of the same
 * name. Mirrors `notesmith_prompts::PromptSource`.
 */
export type PromptSource = 'default' | 'vault';

/**
 * A static custom prompt. The `body` is sent verbatim to the agent; variable
 * substitution is intentionally not supported yet (see issue #193). The format
 * reserves a `variables` frontmatter field so `{{variables}}` can be added
 * later without a breaking change.
 */
export interface Prompt {
	name: string;
	description: string;
	body: string;
	source: PromptSource;
}

interface ListPromptsResponse {
	prompts: Prompt[];
}

function base(vault: string): string {
	return `${API_BASE}/api/v/${encodeURIComponent(vault)}/prompts`;
}

async function fail(res: Response, fallback: string): Promise<never> {
	const { code, message } = await readErrorBody(res);
	throw new ApiError(message ?? `${fallback}: ${res.status}`, res.status, code);
}

/**
 * List the merged static prompts for a vault: config-dir defaults overridden by
 * the vault's `_prompts/` entries (vault wins on a name collision).
 */
export async function listPrompts(vault: string): Promise<Prompt[]> {
	const res = await apiFetch(base(vault));
	if (!res.ok) return fail(res, 'Failed to list prompts');
	const data = (await res.json()) as ListPromptsResponse;
	return data.prompts ?? [];
}

/**
 * Resolve a single prompt by name from the merged list, or `null` if no prompt
 * with that name exists. Convenience for invoking a saved prompt.
 */
export async function getPrompt(vault: string, name: string): Promise<Prompt | null> {
	const prompts = await listPrompts(vault);
	return prompts.find((prompt) => prompt.name === name) ?? null;
}

/**
 * Resolve the text to send to the agent for a named prompt. Returns the prompt
 * `body` verbatim, or `null` if the prompt does not exist. This is the seam
 * where future `{{variable}}` interpolation will be applied without changing
 * call sites.
 */
export async function resolvePromptText(vault: string, name: string): Promise<string | null> {
	const prompt = await getPrompt(vault, name);
	return prompt?.body ?? null;
}
