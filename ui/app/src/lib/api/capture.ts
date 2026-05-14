import { API_BASE } from './core.ts';
import type { WriteNoteResponse } from './notes.ts';

export async function capture(
	vault: string,
	content: string,
	title?: string
): Promise<WriteNoteResponse> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/capture`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ text: content, title })
	});
	if (!res.ok) throw new Error(`Failed to capture note: ${res.status}`);
	return res.json();
}
