import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const putVaultConfig = vi.fn();
const getVaultConfig = vi.fn();
const getCapabilities = vi.fn();

vi.mock('./api', () => ({
	putVaultConfig: (...args: unknown[]) => putVaultConfig(...args),
	getVaultConfig: (...args: unknown[]) => getVaultConfig(...args),
	getCapabilities: (...args: unknown[]) => getCapabilities(...args),
	// ApiError is referenced with `instanceof` in the store.
	ApiError: class ApiError extends Error {
		status: number;
		constructor(status: number) {
			super('api error');
			this.status = status;
		}
	}
}));

async function loadStore() {
	vi.stubGlobal('$state', <T>(value: T) => value);
	const mod = await import('./settings.svelte.ts');
	return mod.settingsStore;
}

beforeEach(() => {
	vi.useFakeTimers();
	putVaultConfig.mockResolvedValue({ config: {}, etag: 'etag-2', warnings: {}, path: '/x' });
});

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
	vi.resetModules();
	vi.clearAllMocks();
});

describe('SettingsStore save feedback', () => {
	it('transitions to a transient "saved" state on success, then back to idle', async () => {
		const store = await loadStore();
		store.draftConfig = {} as never;
		store.etag = 'etag-1';

		const promise = store.saveConfig('demo');
		expect(store.status).toBe('saving');

		const ok = await promise;
		expect(ok).toBe(true);
		expect(store.status).toBe('saved');

		// The confirmation is transient and self-clears back to idle.
		vi.advanceTimersByTime(2500);
		expect(store.status).toBe('idle');
	});

	it('does not enter "saved" when the save fails', async () => {
		const store = await loadStore();
		store.draftConfig = {} as never;
		store.etag = 'etag-1';
		putVaultConfig.mockRejectedValueOnce(new Error('boom'));

		const ok = await store.saveConfig('demo');
		expect(ok).toBe(false);
		expect(store.status).toBe('error');

		vi.advanceTimersByTime(2500);
		expect(store.status).toBe('error');
	});
});
