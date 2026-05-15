import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const listTemplates = vi.fn();
const inputPaletteOpen = vi.fn();
const toastAdd = vi.fn();

vi.mock('./api', () => ({
	capture: vi.fn(),
	createNote: vi.fn(),
	ensureDaily: vi.fn(),
	getNoteHtmlInline: vi.fn(),
	instantiateTemplate: vi.fn(),
	listTemplates,
	routeApply: vi.fn()
}));

vi.mock('./input-palette.svelte', () => ({
	inputPalette: {
		open: inputPaletteOpen
	}
}));

vi.mock('./toast-store.svelte', () => ({
	toastStore: {
		add: toastAdd
	}
}));

vi.mock('./tab-store.svelte', () => ({
	tabStore: {
		selectedPath: '',
		toggleViewMode: vi.fn()
	}
}));

vi.mock('./stores.svelte', () => ({
	vaultStore: {
		loadNotes: vi.fn()
	}
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	base: ''
}));

beforeEach(() => {
	listTemplates.mockReset();
	inputPaletteOpen.mockReset();
	toastAdd.mockReset();
});

afterEach(() => {
	vi.resetModules();
});

describe('buildCommands', () => {
	it('opens the sequential input palette for new note creation', async () => {
		const { buildCommands } = await import('./commands.ts');
		const commands = buildCommands('vault-a', vi.fn());

		await commands.find((command) => command.id === 'new-note')?.execute();

		expect(inputPaletteOpen).toHaveBeenCalledOnce();
		expect(inputPaletteOpen).toHaveBeenCalledWith(
			expect.objectContaining({
				steps: [
					expect.objectContaining({
						mode: 'text',
						label: 'Note title',
						required: true
					}),
					expect.objectContaining({
						mode: 'text',
						label: 'Folder',
						defaultValue: 'Inbox'
					})
				]
			})
		);
	});

	it('shows a warning toast instead of opening input when no vault is selected', async () => {
		const { buildCommands } = await import('./commands.ts');
		const commands = buildCommands('', vi.fn());

		await commands.find((command) => command.id === 'new-note')?.execute();

		expect(inputPaletteOpen).not.toHaveBeenCalled();
		expect(toastAdd).toHaveBeenCalledWith('Select a vault first.', 'warning');
	});

	it('shows a warning toast when no templates are available', async () => {
		listTemplates.mockResolvedValue([]);
		const { buildCommands } = await import('./commands.ts');
		const commands = buildCommands('vault-a', vi.fn());

		await commands.find((command) => command.id === 'new-from-template')?.execute();

		expect(listTemplates).toHaveBeenCalledWith('vault-a');
		expect(inputPaletteOpen).not.toHaveBeenCalled();
		expect(toastAdd).toHaveBeenCalledWith('No templates available.', 'warning');
	});
});
