import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const listTemplates = vi.fn();
const createNote = vi.fn();
const inputPaletteOpen = vi.fn();
const toastAdd = vi.fn();
const loadNotes = vi.fn();
let vaultNotes: unknown[] = [];
let vaultTree: unknown = { name: '', path: '', children: [], notes: [] };

vi.mock('./api', () => ({
	capture: vi.fn(),
	createNote,
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
		get notes() {
			return vaultNotes;
		},
		get tree() {
			return vaultTree;
		},
		loadNotes
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
	createNote.mockReset();
	inputPaletteOpen.mockReset();
	toastAdd.mockReset();
	loadNotes.mockReset();
	vaultNotes = [];
	vaultTree = { name: '', path: '', children: [], notes: [] };
});

afterEach(() => {
	vi.resetModules();
});

describe('buildCommands', () => {
	it('registers a change theme command for the command palette', async () => {
		const { buildCommands } = await import('./commands.ts');
		const commands = buildCommands('vault-a', vi.fn());
		const command = commands.find((entry) => entry.id === 'change-theme');

		expect(command).toMatchObject({
			id: 'change-theme',
			label: 'Change Theme',
			category: 'Appearance'
		});
	});

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

	it('creates a folder note from a folder picker and opens it', async () => {
		vaultTree = {
			name: '',
			path: '',
			notes: [],
			children: [{ name: 'Acme', path: 'Customers/Acme', notes: [], children: [] }]
		};
		createNote.mockResolvedValue({ path: 'Customers/Acme/Acme.md', hash: 'hash-a' });
		const onNavigate = vi.fn();
		const { buildCommands } = await import('./commands.ts');
		const commands = buildCommands('vault-a', onNavigate);

		await commands.find((command) => command.id === 'create-folder-note')?.execute();

		expect(inputPaletteOpen).toHaveBeenCalledWith(
			expect.objectContaining({
				steps: [
					expect.objectContaining({
						mode: 'list',
						label: 'Choose a folder',
						items: [
							{
								id: 'Customers/Acme',
								label: 'Acme',
								description: 'Customers/Acme'
							}
						]
					})
				]
			})
		);

		await inputPaletteOpen.mock.calls[0][0].onComplete(['Customers/Acme']);

		expect(createNote).toHaveBeenCalledWith('vault-a', 'Acme', '# Acme\n', 'Customers/Acme');
		expect(loadNotes).toHaveBeenCalledOnce();
		expect(onNavigate).toHaveBeenCalledWith('Customers/Acme/Acme.md');
	});

	it('opens an existing folder note from the command without overwriting it', async () => {
		vaultTree = {
			name: '',
			path: '',
			notes: [],
			children: [{ name: 'Acme', path: 'Customers/Acme', notes: [], children: [] }]
		};
		vaultNotes = [{ path: 'Customers/Acme/Acme.md', title: '', tags: [] }];
		const onNavigate = vi.fn();
		const { buildCommands } = await import('./commands.ts');
		const commands = buildCommands('vault-a', onNavigate);

		await commands.find((command) => command.id === 'create-folder-note')?.execute();
		await inputPaletteOpen.mock.calls[0][0].onComplete(['Customers/Acme']);

		expect(createNote).not.toHaveBeenCalled();
		expect(toastAdd).toHaveBeenCalledWith('Folder note already exists.', 'success');
		expect(loadNotes).toHaveBeenCalledOnce();
		expect(onNavigate).toHaveBeenCalledWith('Customers/Acme/Acme.md');
	});
});
