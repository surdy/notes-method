import type { EmbeddingStats } from './api/embeddings.ts';

/** Derived, display-ready view of a vault's embedding index state (#260). */
export type EmbeddingIndexState = 'disabled' | 'never-indexed' | 'indexing' | 'ready';

export interface EmbeddingStatusView {
	state: EmbeddingIndexState;
	vectorCount: number;
	lastIndexedLabel: string | null;
	embedderId: string | null;
	dim: number | null;
	p50Ms: number | null;
	p95Ms: number | null;
}

/**
 * Determines whether a (re)build appears to be in progress by comparing two
 * consecutive polls of `/embeddings/stats`. The endpoint has no `running`
 * flag, so progress is inferred: a climbing vector count, or a `last_ingest_at`
 * that just moved forward, means the worker is actively writing.
 */
export function isIndexingInProgress(
	previous: EmbeddingStats | null,
	current: EmbeddingStats
): boolean {
	if (!previous) return false;
	if (current.vector_count !== previous.vector_count) return true;
	if (
		current.last_ingest_at !== null &&
		current.last_ingest_at !== previous.last_ingest_at
	) {
		return true;
	}
	return false;
}

export function deriveEmbeddingStatusView(
	enabled: boolean,
	stats: EmbeddingStats | null,
	indexing: boolean,
	nowMs: number = Date.now()
): EmbeddingStatusView {
	if (!enabled || !stats) {
		return {
			state: 'disabled',
			vectorCount: 0,
			lastIndexedLabel: null,
			embedderId: null,
			dim: null,
			p50Ms: null,
			p95Ms: null
		};
	}

	const neverIndexed = stats.vector_count === 0 && stats.last_ingest_at === null;

	return {
		state: neverIndexed ? 'never-indexed' : indexing ? 'indexing' : 'ready',
		vectorCount: stats.vector_count,
		lastIndexedLabel:
			stats.last_ingest_at !== null ? relativeTimeFromUnixSeconds(stats.last_ingest_at, nowMs) : null,
		embedderId: stats.embedder_id,
		dim: stats.dim,
		p50Ms: stats.sample_count > 0 ? stats.p50_ms : null,
		p95Ms: stats.sample_count > 0 ? stats.p95_ms : null
	};
}

/** Formats a unix-seconds timestamp as a short relative time (e.g. "5m ago"). */
export function relativeTimeFromUnixSeconds(unixSeconds: number, nowMs: number = Date.now()): string {
	const deltaSeconds = Math.max(0, Math.round(nowMs / 1000 - unixSeconds));

	if (deltaSeconds < 5) return 'just now';
	if (deltaSeconds < 60) return `${deltaSeconds}s ago`;

	const minutes = Math.floor(deltaSeconds / 60);
	if (minutes < 60) return `${minutes}m ago`;

	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h ago`;

	const days = Math.floor(hours / 24);
	return `${days}d ago`;
}
