import { describe, expect, it } from 'vitest';
import {
	deriveEmbeddingStatusView,
	isIndexingInProgress,
	relativeTimeFromUnixSeconds
} from './embeddings-status.ts';
import type { EmbeddingStats } from './api/embeddings.ts';

function stats(overrides: Partial<EmbeddingStats> = {}): EmbeddingStats {
	return {
		vector_count: 0,
		db_bytes: 0,
		dim: null,
		embedder_id: null,
		p50_ms: 0,
		p95_ms: 0,
		sample_count: 0,
		last_ingest_at: null,
		...overrides
	};
}

describe('relativeTimeFromUnixSeconds', () => {
	it('formats sub-5s deltas as "just now"', () => {
		expect(relativeTimeFromUnixSeconds(1000, 1000 * 1000 + 2000)).toBe('just now');
	});

	it('formats seconds', () => {
		expect(relativeTimeFromUnixSeconds(1000, 1000 * 1000 + 30_000)).toBe('30s ago');
	});

	it('formats minutes', () => {
		expect(relativeTimeFromUnixSeconds(1000, 1000 * 1000 + 5 * 60_000)).toBe('5m ago');
	});

	it('formats hours', () => {
		expect(relativeTimeFromUnixSeconds(1000, 1000 * 1000 + 3 * 3_600_000)).toBe('3h ago');
	});

	it('formats days', () => {
		expect(relativeTimeFromUnixSeconds(1000, 1000 * 1000 + 2 * 86_400_000)).toBe('2d ago');
	});
});

describe('isIndexingInProgress', () => {
	it('is false with no previous poll', () => {
		expect(isIndexingInProgress(null, stats({ vector_count: 10 }))).toBe(false);
	});

	it('is true when vector count climbs', () => {
		const prev = stats({ vector_count: 10 });
		const current = stats({ vector_count: 15 });
		expect(isIndexingInProgress(prev, current)).toBe(true);
	});

	it('is true when last_ingest_at moves forward', () => {
		const prev = stats({ vector_count: 10, last_ingest_at: 100 });
		const current = stats({ vector_count: 10, last_ingest_at: 105 });
		expect(isIndexingInProgress(prev, current)).toBe(true);
	});

	it('is false when nothing changed', () => {
		const prev = stats({ vector_count: 10, last_ingest_at: 100 });
		const current = stats({ vector_count: 10, last_ingest_at: 100 });
		expect(isIndexingInProgress(prev, current)).toBe(false);
	});

	it('prefers the authoritative running flag over inference', () => {
		// running:false wins even when the vector count climbed between polls.
		const prev = stats({ vector_count: 10 });
		const current = stats({ vector_count: 20, running: false });
		expect(isIndexingInProgress(prev, current)).toBe(false);
	});

	it('reports running:true on the first poll (no inference needed)', () => {
		expect(isIndexingInProgress(null, stats({ running: true }))).toBe(true);
	});
});

describe('deriveEmbeddingStatusView', () => {
	it('reports disabled when embeddings are off', () => {
		const view = deriveEmbeddingStatusView(false, stats({ vector_count: 10 }), false);
		expect(view.state).toBe('disabled');
	});

	it('reports disabled when stats are not yet loaded', () => {
		const view = deriveEmbeddingStatusView(true, null, false);
		expect(view.state).toBe('disabled');
	});

	it('reports never-indexed for a zero-vector, never-ingested vault', () => {
		const view = deriveEmbeddingStatusView(true, stats(), false);
		expect(view.state).toBe('never-indexed');
	});

	it('reports indexing while a (re)build is detected as running', () => {
		const view = deriveEmbeddingStatusView(
			true,
			stats({ vector_count: 20, last_ingest_at: 100 }),
			true
		);
		expect(view.state).toBe('indexing');
	});

	it('reports ready with vector count, relative time, and model info', () => {
		const nowMs = 1_000_000_000_000;
		const view = deriveEmbeddingStatusView(
			true,
			stats({
				vector_count: 42,
				last_ingest_at: Math.floor(nowMs / 1000) - 90,
				embedder_id: 'bge-small',
				dim: 384,
				p50_ms: 12.5,
				p95_ms: 30.1,
				sample_count: 5
			}),
			false,
			nowMs
		);
		expect(view.state).toBe('ready');
		expect(view.vectorCount).toBe(42);
		expect(view.lastIndexedLabel).toBe('1m ago');
		expect(view.embedderId).toBe('bge-small');
		expect(view.dim).toBe(384);
		expect(view.p50Ms).toBe(12.5);
		expect(view.p95Ms).toBe(30.1);
	});

	it('hides latency stats when there is no sample data yet', () => {
		const view = deriveEmbeddingStatusView(
			true,
			stats({ vector_count: 5, last_ingest_at: 100, sample_count: 0 }),
			false
		);
		expect(view.p50Ms).toBeNull();
		expect(view.p95Ms).toBeNull();
	});

	it('exposes a determinate progress bar while indexing with known totals', () => {
		const view = deriveEmbeddingStatusView(
			true,
			stats({ vector_count: 120, notes_total: 1340, notes_done: 612, running: true }),
			true
		);
		expect(view.state).toBe('indexing');
		expect(view.determinate).toBe(true);
		expect(view.notesTotal).toBe(1340);
		expect(view.notesDone).toBe(612);
		expect(view.percent).toBe(46);
	});

	it('falls back to indeterminate when totals are absent (old daemon)', () => {
		const view = deriveEmbeddingStatusView(
			true,
			stats({ vector_count: 120, running: true }),
			true
		);
		expect(view.state).toBe('indexing');
		expect(view.determinate).toBe(false);
		expect(view.percent).toBeNull();
		expect(view.notesTotal).toBeNull();
	});

	it('treats a running first pass over an empty index as indexing, not never-indexed', () => {
		const view = deriveEmbeddingStatusView(
			true,
			stats({ vector_count: 0, last_ingest_at: null, notes_total: 50, notes_done: 3, running: true }),
			true
		);
		expect(view.state).toBe('indexing');
		expect(view.percent).toBe(6);
	});

	it('clears progress fields once the pass is no longer indexing', () => {
		const view = deriveEmbeddingStatusView(
			true,
			stats({ vector_count: 200, last_ingest_at: 100, notes_total: 50, notes_done: 50 }),
			false
		);
		expect(view.state).toBe('ready');
		expect(view.notesTotal).toBeNull();
		expect(view.percent).toBeNull();
		expect(view.determinate).toBe(false);
	});
});
