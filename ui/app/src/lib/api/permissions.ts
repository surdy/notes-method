import { API_BASE, ApiError, apiFetch, readErrorBody } from './core.ts';

/**
 * REST client for per-vault persisted agent "Always Allow" grants (issue #189).
 *
 * Mirrors `transcripts.ts`: persistence is frontend-orchestrated. The chat
 * store fetches a vault's grants at session start (to pre-seed the ACP session
 * so granted tools never re-prompt) and POSTs a new grant when the user picks
 * "Always Allow". The daemon owns the durable store; the Rust agent layer stays
 * HTTP-free.
 */

function base(vault: string): string {
	return `${API_BASE}/api/v/${encodeURIComponent(vault)}/agent/permissions`;
}

async function fail(res: Response, fallback: string): Promise<never> {
	const { code, message } = await readErrorBody(res);
	throw new ApiError(message ?? `${fallback}: ${res.status}`, res.status, code);
}

/**
 * A 404 on the permissions collection route means the daemon predates the
 * agent-permission API — a version-skew, not a missing grant. Surface a clear,
 * actionable message instead of a bare "404" (matching `transcripts.ts`).
 */
function outdatedServerError(): ApiError {
	return new ApiError(
		"Persisted agent permissions aren't available: this Notesmith server is too old (missing the agent permission API). Update the daemon to the latest version.",
		404,
		'permission_api_unavailable'
	);
}

/** List the tool names the user has granted "Always Allow" for `vault`. */
export async function listGrants(vault: string): Promise<string[]> {
	const res = await apiFetch(base(vault));
	if (res.status === 404) throw outdatedServerError();
	if (!res.ok) return fail(res, 'Failed to list permission grants');
	return res.json();
}

/** Persist an "Always Allow" grant for `tool` in `vault`. */
export async function grant(vault: string, tool: string): Promise<void> {
	const res = await apiFetch(base(vault), {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ tool })
	});
	if (res.status === 404) throw outdatedServerError();
	if (!res.ok) return fail(res, 'Failed to grant permission');
}

/** Revoke a persisted grant for `tool` in `vault`. */
export async function revoke(vault: string, tool: string): Promise<void> {
	const res = await apiFetch(`${base(vault)}/${encodeURIComponent(tool)}`, {
		method: 'DELETE'
	});
	if (!res.ok && res.status !== 404) return fail(res, 'Failed to revoke permission');
}
