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
	/** Total notes the active/most-recent pass will visit, if the daemon reports it. */
	notesTotal: number | null;
	/** Notes visited so far in that pass, if the daemon reports it. */
	notesDone: number | null;
	/** Whole-integer percent 0–100 when a determinate total is available, else null. */
	percent: number | null;
	/** True when a real `N / M` bar can be drawn; false → indeterminate spinner. */
	determinate: boolean;
}

/**
 * Whether an embed (re)build appears to be in progress.
 *
 * Prefers the authoritative `running` flag from the exact-progress extension
 * (#260). Falls back to inference — a climbing vector count or a freshly moved
 * `last_ingest_at` between two polls — when talking to an older daemon that
 * doesn't report `running`.
 */
export function isIndexingInProgress(
	previous: EmbeddingStats | null,
	current: EmbeddingStats
): boolean {
	if (current.running !== undefined) return current.running;
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
			p95Ms: null,
			notesTotal: null,
			notesDone: null,
			percent: null,
			determinate: false
		};
	}

	const neverIndexed = stats.vector_count === 0 && stats.last_ingest_at === null;

	const notesTotal = stats.notes_total ?? null;
	const notesDone = stats.notes_done ?? null;
	const determinate = notesTotal !== null && notesTotal > 0;
	const percent =
		determinate && notesDone !== null
			? Math.min(100, Math.max(0, Math.round((notesDone / (notesTotal as number)) * 100)))
			: null;

	return {
		// A running first pass over a still-empty index is "indexing", not
		// "never-indexed": the authoritative flag wins over the zero-vector heuristic.
		state: indexing ? 'indexing' : neverIndexed ? 'never-indexed' : 'ready',
		vectorCount: stats.vector_count,
		lastIndexedLabel:
			stats.last_ingest_at !== null ? relativeTimeFromUnixSeconds(stats.last_ingest_at, nowMs) : null,
		embedderId: stats.embedder_id,
		dim: stats.dim,
		p50Ms: stats.sample_count > 0 ? stats.p50_ms : null,
		p95Ms: stats.sample_count > 0 ? stats.p95_ms : null,
		notesTotal: indexing ? notesTotal : null,
		notesDone: indexing ? notesDone : null,
		percent: indexing ? percent : null,
		determinate: indexing ? determinate : false
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
