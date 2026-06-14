import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const BREAK_GLASS_STORAGE_KEY = 'notesmith:agent-break-glass';

beforeEach(() => {
	vi.stubGlobal('$state', <T>(value: T) => value);
	const store = new Map<string, string>();
	vi.stubGlobal('localStorage', {
		getItem: (k: string) => store.get(k) ?? null,
		setItem: (k: string, v: string) => store.set(k, v),
		removeItem: (k: string) => store.delete(k),
		clear: () => store.clear()
	});
});

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('breakGlassStore', () => {
	it('defaults to off', async () => {
		const { breakGlassStore } = await import('./break-glass.svelte.ts');
		expect(breakGlassStore.enabled).toBe(false);
	});

	it('persists and reloads the enabled flag', async () => {
		const { breakGlassStore } = await import('./break-glass.svelte.ts');
		breakGlassStore.set(true);
		expect(localStorage.getItem(BREAK_GLASS_STORAGE_KEY)).toBe('true');

		breakGlassStore.enabled = false;
		breakGlassStore.load();
		expect(breakGlassStore.enabled).toBe(true);
	});

	it('toggles', async () => {
		const { breakGlassStore } = await import('./break-glass.svelte.ts');
		breakGlassStore.set(false);
		breakGlassStore.toggle();
		expect(breakGlassStore.enabled).toBe(true);
		breakGlassStore.toggle();
		expect(breakGlassStore.enabled).toBe(false);
	});

	it('survives a throwing localStorage', async () => {
		vi.stubGlobal('localStorage', {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			}
		});
		const { breakGlassStore } = await import('./break-glass.svelte.ts');
		expect(() => breakGlassStore.set(true)).not.toThrow();
		expect(() => breakGlassStore.load()).not.toThrow();
		expect(breakGlassStore.enabled).toBe(false);
	});
});
