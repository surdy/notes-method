import { afterEach, describe, expect, it } from 'vitest';

import { ApiError, versionMismatch } from './core.ts';
import { classifyError } from './error-classify.ts';

const SAFETY_HINT = 'Your markdown files are safe on disk.';

afterEach(() => {
	versionMismatch.set(null);
});

describe('classifyError', () => {
	it('classifies failed fetch errors as service-not-running', () => {
		const classified = classifyError(new TypeError('Failed to fetch'), 'list-notes');

		expect(classified.category).toBe('service-not-running');
		expect(classified.hint).toBe(SAFETY_HINT);
	});

	it('classifies connection reset errors as connection-lost', () => {
		const classified = classifyError(new TypeError('Network connection was lost'), 'note-detail');

		expect(classified.category).toBe('connection-lost');
		expect(classified.hint).toBe(SAFETY_HINT);
	});

	it('classifies known 404 responses as endpoint-not-found', () => {
		const classified = classifyError(new ApiError('Missing endpoint', 404), 'note-detail');

		expect(classified.category).toBe('endpoint-not-found');
		expect(classified.hint).toBe(SAFETY_HINT);
	});

	it('classifies conflict responses', () => {
		const classified = classifyError(new ApiError('Conflict', 409), 'save-note');

		expect(classified.category).toBe('conflict');
		expect(classified.hint).toBe(SAFETY_HINT);
	});

	it('classifies rate-limited responses', () => {
		const classified = classifyError(new ApiError('Slow down', 429), 'list-notes');

		expect(classified.category).toBe('rate-limited');
		expect(classified.hint).toBe(SAFETY_HINT);
	});

	it('classifies server errors', () => {
		const classified = classifyError(new ApiError('Boom', 500), 'save-note');

		expect(classified.category).toBe('server-error');
		expect(classified.hint).toBe(SAFETY_HINT);
	});

	it('falls back to unknown for unrecognized errors', () => {
		const classified = classifyError(new Error('Unexpected'), 'list-notes');

		expect(classified.category).toBe('unknown');
		expect(classified.hint).toBe(SAFETY_HINT);
	});
});
