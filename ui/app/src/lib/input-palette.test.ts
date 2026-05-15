import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

beforeEach(() => {
	vi.stubGlobal('$state', <T>(value: T) => value);
});

afterEach(() => {
	vi.unstubAllGlobals();
	vi.resetModules();
});

describe('inputPalette', () => {
	it('opens a request and advances through each step before completing', async () => {
		const onComplete = vi.fn();
		const { inputPalette } = await import('./input-palette.svelte.ts');

		inputPalette.open({
			steps: [
				{ mode: 'text', label: 'Note title', required: true },
				{
					mode: 'list',
					label: 'Folder',
					items: [{ id: 'Inbox', label: 'Inbox' }]
				}
			],
			onComplete
		});

		expect(inputPalette.request?.steps).toHaveLength(2);
		expect(inputPalette.currentStep).toBe(0);
		expect(inputPalette.values).toEqual([null, null]);

		inputPalette.submitStep('Quarterly plan');

		expect(inputPalette.currentStep).toBe(1);
		expect(inputPalette.values).toEqual(['Quarterly plan', null]);

		await inputPalette.submitStep('Inbox');

		expect(onComplete).toHaveBeenCalledWith(['Quarterly plan', 'Inbox']);
		expect(inputPalette.request).toBeNull();
		expect(inputPalette.currentStep).toBe(0);
		expect(inputPalette.values).toEqual([]);
	});

	it('keeps required text steps open when submitted blank', async () => {
		const onComplete = vi.fn();
		const { inputPalette } = await import('./input-palette.svelte.ts');

		inputPalette.open({
			steps: [{ mode: 'text', label: 'Capture text', required: true }],
			onComplete
		});

		await inputPalette.submitStep('   ');

		expect(inputPalette.currentStep).toBe(0);
		expect(inputPalette.request).not.toBeNull();
		expect(onComplete).not.toHaveBeenCalled();
	});

	it('cancels the active request and calls onCancel', async () => {
		const onCancel = vi.fn();
		const { inputPalette } = await import('./input-palette.svelte.ts');

		inputPalette.open({
			steps: [{ mode: 'text', label: 'Note title' }],
			onComplete: vi.fn(),
			onCancel
		});

		inputPalette.cancel();

		expect(onCancel).toHaveBeenCalledOnce();
		expect(inputPalette.request).toBeNull();
		expect(inputPalette.values).toEqual([]);
	});
});
