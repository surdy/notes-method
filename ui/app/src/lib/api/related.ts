import { API_BASE, apiFetch, encodePath } from './core.ts';

/** A note related to the active note, with its blended score and signals. */
export interface RelatedNote {
	path: string;
	title: string;
	score: number;
	embedding_similarity: number | null;
	directly_linked: boolean;
	shared_neighbors: number;
}

export interface RelatedNotesResult {
	path: string;
	embeddings_used: boolean;
	related: RelatedNote[];
}

/**
 * Fetch notes related to `path`, ranked by embedding similarity blended with
 * link-graph proximity (issue #201). Degrades to graph-only ranking when the
 * vault has no usable embeddings (`embeddings_used: false`).
 */
export async function getRelatedNotes(
	vault: string,
	path: string,
	limit = 10
): Promise<RelatedNotesResult> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/related/${encodePath(path)}?limit=${limit}`
	);
	if (!res.ok) throw new Error(`Related notes request failed: ${res.status}`);
	return res.json();
}
