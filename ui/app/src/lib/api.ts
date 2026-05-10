const API_BASE = '';

function encodePath(path: string): string {
	return path
		.split('/')
		.map((segment) => encodeURIComponent(segment))
		.join('/');
}

export interface NoteSummary {
	path: string;
	title: string;
	type: string;
	customer?: string;
	date?: string;
	archived: boolean;
}

export interface NoteDetail {
	path: string;
	body: string;
	frontmatter: Record<string, unknown>;
}

export async function listNotes(vault: string): Promise<NoteSummary[]> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes`);
	if (!res.ok) throw new Error(`Failed to list notes: ${res.status}`);
	return res.json();
}

export async function getNote(vault: string, path: string): Promise<NoteDetail> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes/${encodePath(path)}`);
	if (!res.ok) throw new Error(`Failed to get note: ${res.status}`);
	return res.json();
}

export async function getNoteHtml(vault: string, path: string): Promise<string> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/html/${encodePath(path)}`);
	if (!res.ok) throw new Error(`Failed to render note: ${res.status}`);
	return res.text();
}

export async function searchNotes(vault: string, query: string): Promise<NoteSummary[]> {
	const res = await fetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/search?q=${encodeURIComponent(query)}`
	);
	if (!res.ok) throw new Error(`Search failed: ${res.status}`);
	return res.json();
}
