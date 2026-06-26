import { API_BASE, ApiError, apiFetch, readErrorBody } from './core.ts';

/**
 * Where a customization came from. A `project` entry (vault `.notesmith/`)
 * overrides a `global` (`~/.config/notesmith/`) entry of the same id. Mirrors
 * `notesmith_customization::Source`.
 */
export type CustomizationSource = 'project' | 'global';

/**
 * A discovered custom agent (persona): a preamble prompt that runs on top of an
 * ACP agent backend (ADR 0016). Not a separate CLI. Mirrors
 * `notesmith_customization::CustomAgent`.
 */
export interface CustomAgent {
	id: string;
	name: string;
	description: string;
	/** ACP backend agent id (`copilot`/`claude`/…); `null` = use the selected agent. */
	backend: string | null;
	/** Model id to request for this persona; `null` when unset. */
	model: string | null;
	/**
	 * Whether this persona runs read-only (search/answer only, never writes).
	 * Authored via the `access: read-only` frontmatter key; defaults to `false`
	 * (read-write). Selecting the persona applies this to the chat session.
	 */
	readOnly: boolean;
	/** The persona's system/preamble prompt. */
	body: string;
	source: CustomizationSource;
}

/** A discovered skill: reusable instructions the agent can load. */
export interface Skill {
	id: string;
	name: string;
	description: string;
	body: string;
	source: CustomizationSource;
}

/** A discovered instruction: always-applied guidance. */
export interface Instruction {
	id: string;
	name: string;
	description: string;
	body: string;
	source: CustomizationSource;
}

/** The merged customization set for a vault. */
export interface Customizations {
	agents: CustomAgent[];
	skills: Skill[];
	instructions: Instruction[];
}

/** An empty set, used as a resilient fallback. */
export function emptyCustomizations(): Customizations {
	return { agents: [], skills: [], instructions: [] };
}

function base(vault: string): string {
	return `${API_BASE}/api/v/${encodeURIComponent(vault)}/customizations`;
}

async function fail(res: Response, fallback: string): Promise<never> {
	const { code, message } = await readErrorBody(res);
	throw new ApiError(message ?? `${fallback}: ${res.status}`, res.status, code);
}

/**
 * List the merged customization set for a vault: global entries overridden by
 * the vault's `.notesmith/` entries (project wins by id). Malformed files are
 * skipped server-side, so this resolves to a (possibly empty) set, never an
 * error from a bad file.
 */
export async function listCustomizations(vault: string): Promise<Customizations> {
	const res = await apiFetch(base(vault));
	if (!res.ok) return fail(res, 'Failed to list customizations');
	const data = (await res.json()) as Partial<Customizations>;
	return {
		agents: data.agents ?? [],
		skills: data.skills ?? [],
		instructions: data.instructions ?? []
	};
}
