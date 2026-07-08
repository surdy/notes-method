import { API_BASE, apiFetch } from './core.ts';

export interface EmbeddingStats {
	vector_count: number;
	db_bytes: number;
	dim: number | null;
	embedder_id: string | null;
	p50_ms: number;
	p95_ms: number;
	sample_count: number;
	last_ingest_at: number | null;
}

export async function getEmbeddingStats(vault: string): Promise<EmbeddingStats> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/embeddings/stats`);
	if (!res.ok) throw new Error(`Failed to load embedding stats: ${res.status}`);
	return res.json();
}
