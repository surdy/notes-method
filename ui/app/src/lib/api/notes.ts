import { API_BASE, ApiError, apiFetch, encodePath } from './core.ts';

export interface NoteSummary {
	path: string;
	title: string;
	type: string;
	customer?: string;
	date?: string;
	created_at?: string;
	updated_at?: string;
	archived: boolean;
}

export interface NoteDetail {
	path: string;
	body: string;
	frontmatter: Record<string, unknown> | null;
	raw_frontmatter?: string | null;
	tasks?: NoteTask[];
	hash: string;
}

export interface WriteNoteResponse {
	path: string;
	hash: string;
}

export type TaskMutationStatus =
	| 'todo'
	| 'in_progress'
	| 'blocked'
	| 'waiting'
	| 'on_hold'
	| 'done'
	| 'cancelled';

export interface SourcePosition {
	line: number;
	column: number;
	offset: number;
	length: number;
}

export interface NoteTask {
	status: string;
	content: string;
	position: SourcePosition;
	content_hash?: string | null;
}

export async function listNotes(vault: string): Promise<NoteSummary[]> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes`);
	if (!res.ok) throw new Error(`Failed to list notes: ${res.status}`);
	return res.json();
}

export async function getNote(vault: string, path: string): Promise<NoteDetail> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes/${encodePath(path)}`);
	if (!res.ok) throw new ApiError(`Failed to get note: ${res.status}`, res.status);
	return res.json();
}

export async function getNoteHtml(vault: string, path: string): Promise<string> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/html/${encodePath(path)}`);
	if (!res.ok) throw new Error(`Failed to render note: ${res.status}`);
	return res.text();
}

export async function getNoteHtmlInline(vault: string, path: string): Promise<string> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/html/${encodePath(path)}?inline_styles=true`
	);
	if (!res.ok) throw new Error(`Failed to render note HTML: ${res.status}`);
	return res.text();
}

export async function searchNotes(vault: string, query: string): Promise<NoteSummary[]> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/search?q=${encodeURIComponent(query)}`
	);
	if (!res.ok) throw new Error(`Search failed: ${res.status}`);
	return res.json();
}

export async function createNote(
	vault: string,
	title: string,
	content: string,
	folder?: string
): Promise<WriteNoteResponse> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title, content, folder })
	});
	if (!res.ok) throw new Error(`Failed to create note: ${res.status}`);
	return res.json();
}

export async function putNote(
	vault: string,
	path: string,
	content: string,
	expectedHash?: string | null
): Promise<WriteNoteResponse> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes/${encodePath(path)}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ content, expected_hash: expectedHash ?? undefined })
	});
	if (!res.ok) throw new ApiError(`Failed to save note: ${res.status}`, res.status);
	return res.json();
}

export async function toggleTaskStatus(
	vault: string,
	notePath: string,
	taskHash: string,
	status: TaskMutationStatus
): Promise<WriteNoteResponse> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/tasks/toggle`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			note_path: notePath,
			task_hash: taskHash,
			status
		})
	});
	if (!res.ok) throw new ApiError(`Failed to toggle task: ${res.status}`, res.status);
	return res.json();
}

export async function ensureDaily(
	vault: string,
	date?: string
): Promise<{ path: string; created: boolean }> {
	const day = date ?? new Date().toISOString().slice(0, 10);
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/daily/${day}`, {
		method: 'POST'
	});
	if (!res.ok) throw new Error(`Failed to ensure daily: ${res.status}`);
	return res.json();
}
