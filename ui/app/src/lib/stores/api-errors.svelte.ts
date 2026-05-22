import { writable } from 'svelte/store';

import { classifyError, type ClassifiedError } from '$lib/api/error-classify';

export const lastApiError = writable<ClassifiedError | null>(null);

let clearTimer: ReturnType<typeof setTimeout> | null = null;

export function reportApiError(error: unknown, endpointHint?: string): ClassifiedError {
	if (clearTimer) {
		clearTimeout(clearTimer);
		clearTimer = null;
	}

	const classified = classifyError(error, endpointHint);
	lastApiError.set(classified);

	if (classified.retryable) {
		clearTimer = setTimeout(() => {
			clearTimer = null;
			lastApiError.set(null);
		}, 10_000);
	}

	return classified;
}

export function clearApiError() {
	if (clearTimer) {
		clearTimeout(clearTimer);
		clearTimer = null;
	}
	lastApiError.set(null);
}
