import { describe, expect, it, vi } from 'vitest';
import { buildPromptSteps } from './template-prompts';

const textPrompt = { name: 'title', type: 'text', required: true };
const pickerPrompt = {
	name: 'customer',
	type: 'field-picker',
	field: 'customers',
	required: true
};

describe('buildPromptSteps', () => {
	it('maps plain prompts to text steps', async () => {
		const fetchSuggestions = vi.fn();

		const steps = await buildPromptSteps([textPrompt], fetchSuggestions);

		expect(steps).toEqual([
			{ mode: 'text', label: 'title', placeholder: 'Enter title...', required: true }
		]);
		expect(fetchSuggestions).not.toHaveBeenCalled();
	});

	it('marks optional prompts in the label', async () => {
		const steps = await buildPromptSteps(
			[{ name: 'stream', type: 'text', required: false }],
			vi.fn()
		);

		expect(steps[0].label).toBe('stream (optional)');
	});

	it('queries the declared field, not the prompt name', async () => {
		const fetchSuggestions = vi.fn().mockResolvedValue(['[[Acme Corp]]']);

		await buildPromptSteps([pickerPrompt], fetchSuggestions);

		expect(fetchSuggestions).toHaveBeenCalledWith('customers');
	});

	it('falls back to the prompt name when no field is declared', async () => {
		const fetchSuggestions = vi.fn().mockResolvedValue(['P1']);

		await buildPromptSteps(
			[{ name: 'priority', type: 'field-picker', required: false }],
			fetchSuggestions
		);

		expect(fetchSuggestions).toHaveBeenCalledWith('priority');
	});

	it('unwraps wikilink suggestions so templates can re-wrap them', async () => {
		const fetchSuggestions = vi.fn().mockResolvedValue(['[[Acme Corp]]', '[[Globex]]']);

		const [step] = await buildPromptSteps([pickerPrompt], fetchSuggestions);

		expect(step).toMatchObject({ mode: 'list', allowCustom: true });
		expect(step.mode === 'list' && step.items).toEqual([
			{ id: 'Acme Corp', label: 'Acme Corp' },
			{ id: 'Globex', label: 'Globex' }
		]);
	});

	it('leaves non-wikilink values alone', async () => {
		const fetchSuggestions = vi.fn().mockResolvedValue(['P0', 'P1']);

		const [step] = await buildPromptSteps(
			[{ name: 'priority', type: 'field-picker', required: false }],
			fetchSuggestions
		);

		expect(step.mode === 'list' && step.items).toEqual([
			{ id: 'P0', label: 'P0' },
			{ id: 'P1', label: 'P1' }
		]);
	});

	it('accepts values the vault has not seen yet', async () => {
		const [step] = await buildPromptSteps([pickerPrompt], vi.fn().mockResolvedValue(['[[Acme]]']));

		// Without this a brand-new customer could never be captured.
		expect(step.mode === 'list' && step.allowCustom).toBe(true);
	});

	it('degrades to text when there are no suggestions yet', async () => {
		const [step] = await buildPromptSteps([pickerPrompt], vi.fn().mockResolvedValue([]));

		expect(step.mode).toBe('text');
		expect(step.mode === 'text' && step.required).toBe(true);
	});

	it('degrades to text when the lookup fails', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const fetchSuggestions = vi.fn().mockRejectedValue(new Error('daemon down'));

		const [step] = await buildPromptSteps([pickerPrompt], fetchSuggestions);

		expect(step.mode).toBe('text');
		expect(warn).toHaveBeenCalled();
		warn.mockRestore();
	});

	it('preserves prompt order when suggestions resolve out of order', async () => {
		const fetchSuggestions = vi.fn().mockImplementation(async (field: string) => {
			if (field === 'customers') {
				await new Promise((resolve) => setTimeout(resolve, 5));
				return ['[[Acme Corp]]'];
			}
			return ['[[Migration to v2]]'];
		});

		const steps = await buildPromptSteps(
			[
				textPrompt,
				pickerPrompt,
				{ name: 'stream', type: 'field-picker', field: 'streams', required: false }
			],
			fetchSuggestions
		);

		expect(steps.map((step) => step.label)).toEqual(['title', 'customer', 'stream (optional)']);
	});
});
