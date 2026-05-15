import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

beforeEach(() => {
	vi.useFakeTimers();
	vi.stubGlobal('$state', <T>(value: T) => value);
	vi.stubGlobal('crypto', {
		randomUUID: vi.fn(() => 'toast-id')
	});
});

afterEach(() => {
	vi.runOnlyPendingTimers();
	vi.useRealTimers();
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('toastStore', () => {
	it('adds a toast and auto-dismisses it after four seconds', async () => {
		const { toastStore } = await import('./toast-store.svelte.ts');

		toastStore.add('Copied note as HTML.');

		expect(toastStore.toasts).toEqual([
			{ id: 'toast-id', message: 'Copied note as HTML.', type: 'success' }
		]);

		await vi.advanceTimersByTimeAsync(4000);

		expect(toastStore.toasts).toEqual([]);
	});

	it('dismisses a toast immediately when requested', async () => {
		const { toastStore } = await import('./toast-store.svelte.ts');

		toastStore.add('Select a vault first.', 'warning');
		toastStore.dismiss('toast-id');

		expect(toastStore.toasts).toEqual([]);
	});
});
