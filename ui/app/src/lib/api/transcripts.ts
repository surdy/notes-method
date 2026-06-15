import { API_BASE, ApiError, apiFetch, readErrorBody } from './core.ts';

/** Author of a transcript message. Mirrors `notesmith_transcript::Role`. */
export type Role = 'user' | 'agent' | 'system';

/** A persisted chat thread, scoped to a vault. */
export interface Thread {
	id: string;
	vault: string;
	title: string;
	agent: string | null;
	model: string | null;
	created_at: string;
	updated_at: string;
}

/** A persisted message within a thread. */
export interface Message {
	id: number;
	thread_id: string;
	seq: number;
	role: Role;
	content: string;
	created_at: string;
}

function base(vault: string): string {
	return `${API_BASE}/api/v/${encodeURIComponent(vault)}/agent/threads`;
}

async function fail(res: Response, fallback: string): Promise<never> {
	const { code, message } = await readErrorBody(res);
	throw new ApiError(message ?? `${fallback}: ${res.status}`, res.status, code);
}

/**
 * A 404 on a thread *collection* route (no thread id) means the daemon predates
 * the agent transcript API — a version-skew, not a missing thread. Surface a
 * clear, actionable message instead of a bare "404" (see `ApiError.code`).
 */
function outdatedServerError(): ApiError {
	return new ApiError(
		"Agent chat history isn't available: this Notesmith server is too old (missing the agent transcript API). Update the daemon to the latest version.",
		404,
		'transcript_api_unavailable'
	);
}

export async function listThreads(vault: string): Promise<Thread[]> {
	const res = await apiFetch(base(vault));
	if (res.status === 404) throw outdatedServerError();
	if (!res.ok) return fail(res, 'Failed to list threads');
	return res.json();
}

export async function createThread(
	vault: string,
	title: string,
	agent?: string | null,
	model?: string | null
): Promise<Thread> {
	const res = await apiFetch(base(vault), {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title, agent: agent ?? null, model: model ?? null })
	});
	if (res.status === 404) throw outdatedServerError();
	if (!res.ok) return fail(res, 'Failed to create thread');
	return res.json();
}

export async function getThread(vault: string, threadId: string): Promise<Thread> {
	const res = await apiFetch(`${base(vault)}/${encodeURIComponent(threadId)}`);
	if (!res.ok) return fail(res, 'Failed to get thread');
	return res.json();
}

export async function renameThread(
	vault: string,
	threadId: string,
	title: string
): Promise<Thread> {
	const res = await apiFetch(`${base(vault)}/${encodeURIComponent(threadId)}/rename`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title })
	});
	if (!res.ok) return fail(res, 'Failed to rename thread');
	return res.json();
}

export async function deleteThread(vault: string, threadId: string): Promise<void> {
	const res = await apiFetch(`${base(vault)}/${encodeURIComponent(threadId)}`, {
		method: 'DELETE'
	});
	if (!res.ok && res.status !== 404) return fail(res, 'Failed to delete thread');
}

export async function listMessages(vault: string, threadId: string): Promise<Message[]> {
	const res = await apiFetch(`${base(vault)}/${encodeURIComponent(threadId)}/messages`);
	if (!res.ok) return fail(res, 'Failed to load messages');
	return res.json();
}

export async function appendMessage(
	vault: string,
	threadId: string,
	role: Role,
	content: string
): Promise<Message> {
	const res = await apiFetch(`${base(vault)}/${encodeURIComponent(threadId)}/messages`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ role, content })
	});
	if (!res.ok) return fail(res, 'Failed to append message');
	return res.json();
}
