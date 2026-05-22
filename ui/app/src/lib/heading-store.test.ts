import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

beforeEach(() => {
	vi.stubGlobal('$state', <T>(value: T) => value);
});

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('headingStore', () => {
	it('tracks heading updates and active heading selection', async () => {
		const { headingStore } = await import('./heading-store.svelte.ts');

		headingStore.update([{ level: 2, text: 'Section', from: 10 }]);
		headingStore.setActive(0);

		expect(headingStore.headings).toEqual([{ level: 2, text: 'Section', from: 10 }]);
		expect(headingStore.activeIndex).toBe(0);

		headingStore.clear();

		expect(headingStore.headings).toEqual([]);
		expect(headingStore.activeIndex).toBe(-1);
	});
});
