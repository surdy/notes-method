import {
	capture,
	createNote,
	ensureDaily,
	getNoteHtmlInline,
	instantiateTemplate,
	listTemplates,
	routeApply
} from './api';
import { goto } from '$app/navigation';
import { base } from '$app/paths';
import { createOrOpenFolderNote, listFolderPickerItems } from './folder-notes';
import { activeEditorStore } from './editor/active-editor.svelte';
import { activeSession } from './agent/active-session.svelte';
import {
	applyModeFor,
	instructionFor,
	INLINE_COMMANDS,
	type InlineCommandId
} from './agent/inline-commands';
import { inputPalette } from './input-palette.svelte';
import { tabStore } from './tab-store.svelte';
import { toastStore } from './toast-store.svelte';
import { vaultStore } from './stores.svelte';

export interface Command {
id: string;
label: string;
category:
	| 'Notes'
	| 'Tasks'
	| 'Templates'
	| 'Navigation'
	| 'Vault'
	| 'Settings'
	| 'Appearance'
	| 'AI';
shortcut?: string;
execute: () => void | Promise<void>;
}

function notifyError(message: string, cause: unknown) {
console.error(message, cause);
toastStore.add(message, 'error');
}

/**
 * Run an inline editor command (issue #195) against the active note's selection.
 * Reads the selection from the active editor and dispatches it to the shared
 * chat agent session — no chat panel expansion required. Missing editor,
 * selection, or agent degrade to a warning toast rather than throwing.
 *
 * The `custom` command first opens the input palette to collect a free-form
 * instruction. Shared by the command palette and the editor's right-click menu.
 */
export async function runInlineEditorCommand(id: InlineCommandId): Promise<void> {
if (id === 'custom') {
	inputPalette.open({
		steps: [
			{
				mode: 'text',
				label: 'Instruction',
				placeholder: 'e.g. Make this more formal',
				required: true
			}
		],
		onComplete: async ([prompt]) => {
			if (!prompt?.trim()) return;
			await dispatchInlineCommand('custom', prompt.trim());
		}
	});
	return;
}
await dispatchInlineCommand(id);
}

async function dispatchInlineCommand(id: InlineCommandId, customPrompt?: string): Promise<void> {
const view = activeEditorStore.view;
if (!view) {
	toastStore.add('Open a note first.', 'warning');
	return;
}
const sel = view.state.selection.main;
const selection = view.state.sliceDoc(sel.from, sel.to);
if (!selection.trim()) {
	toastStore.add('Select some text first.', 'warning');
	return;
}
const store = activeSession.current;
if (!store || !store.selectedAgent) {
	toastStore.add('Start the agent panel first.', 'warning');
	return;
}
try {
	await store.runInlineCommand({
		instruction: instructionFor(id, customPrompt),
		selection,
		applyMode: applyModeFor(id),
		activeNote: tabStore.selectedPath
	});
} catch (cause) {
	notifyError('Failed to run the AI command.', cause);
}
}

async function reloadAndNavigate(path?: string, onNavigate?: (path: string) => void) {
await vaultStore.loadNotes();
if (path && onNavigate) {
onNavigate(path);
}
}

export function buildCommands(vault: string, onNavigate: (path: string) => void): Command[] {
const requireVault = (): string | null => {
if (!vault) {
toastStore.add('Select a vault first.', 'warning');
return null;
}
return vault;
};

return [
{
id: 'new-note',
label: 'New Note',
category: 'Notes',
shortcut: '⌘N',
execute: () => {
const currentVault = requireVault();
if (!currentVault) return;

inputPalette.open({
steps: [
	{
		mode: 'text',
		label: 'Note title',
		placeholder: 'Enter a title...',
		required: true
	},
	{
		mode: 'text',
		label: 'Folder',
		placeholder: 'Inbox',
		defaultValue: 'Inbox'
	}
],
onComplete: async ([title, folder]) => {
	if (!title?.trim()) return;

	try {
		const created = await createNote(
			currentVault,
			title.trim(),
			'',
			folder?.trim() || undefined
		);
		await reloadAndNavigate(created.path, onNavigate);
	} catch (cause) {
		notifyError('Failed to create note.', cause);
	}
}
});
}
},
{
id: 'capture',
label: 'Quick Capture',
category: 'Notes',
shortcut: '⌘⇧N',
execute: () => {
const currentVault = requireVault();
if (!currentVault) return;

inputPalette.open({
steps: [
	{
		mode: 'text',
		label: 'Capture text',
		placeholder: "What's on your mind?",
		required: true
	},
	{
		mode: 'text',
		label: 'Title (optional)',
		placeholder: 'Auto-generated if empty'
	}
],
onComplete: async ([content, title]) => {
	if (!content?.trim()) return;

	try {
		const created = await capture(currentVault, content.trim(), title?.trim() || undefined);
		await reloadAndNavigate(created.path, onNavigate);
	} catch (cause) {
		notifyError('Failed to capture note.', cause);
	}
}
});
}
},
{
id: 'create-folder-note',
label: 'Create Folder Note',
category: 'Notes',
execute: () => {
const currentVault = requireVault();
if (!currentVault) return;

const folders = listFolderPickerItems(vaultStore.tree);
if (folders.length === 0) {
toastStore.add('No folders available.', 'warning');
return;
}

inputPalette.open({
steps: [
	{
		mode: 'list',
		label: 'Choose a folder',
		items: folders,
		placeholder: 'Search folders...'
	}
],
onComplete: async ([folderPath]) => {
	if (!folderPath) return;

	try {
		const result = await createOrOpenFolderNote({
			vault: currentVault,
			folderPath,
			notes: vaultStore.notes,
			createNote
		});
		if (!result.created) {
			toastStore.add('Folder note already exists.', 'success');
		}
		await reloadAndNavigate(result.path, onNavigate);
	} catch (cause) {
		notifyError('Failed to create folder note.', cause);
	}
}
});
}
},
{
id: 'copy-as-html',
label: 'Copy as HTML',
category: 'Notes',
execute: async () => {
	const currentVault = requireVault();
	if (!currentVault) return;
	if (!tabStore.selectedPath) {
		toastStore.add('Select a note first.', 'warning');
		return;
	}

	try {
		const html = await getNoteHtmlInline(currentVault, tabStore.selectedPath);
		await navigator.clipboard.write([
			new ClipboardItem({
				'text/html': new Blob([html], { type: 'text/html' }),
				'text/plain': new Blob([html], { type: 'text/plain' })
			})
		]);
		toastStore.add('Copied note as HTML.', 'success');
	} catch (cause) {
		notifyError('Failed to copy as HTML.', cause);
	}
}
},
{
id: 'archive-current',
label: 'Archive Current Note',
category: 'Notes',
shortcut: '⌘⇧A',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;
if (!tabStore.selectedPath) {
toastStore.add('Select a note to archive.', 'warning');
return;
}

try {
const response = await routeApply(currentVault, [tabStore.selectedPath]);
await reloadAndNavigate(response.results[0]?.to, onNavigate);
} catch (cause) {
notifyError('Failed to archive the current note.', cause);
}
}
},
{
id: 'open-daily',
label: "Open Today's Daily Note",
category: 'Navigation',
shortcut: '⌘D',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
const daily = await ensureDaily(currentVault);
await reloadAndNavigate(daily.path, onNavigate);
} catch (cause) {
notifyError('Failed to open today\'s daily note.', cause);
}
}
},
{
id: 'new-from-template',
label: 'New Note from Template',
category: 'Templates',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
const templates = await listTemplates(currentVault);
if (templates.length === 0) {
toastStore.add('No templates available.', 'warning');
return;
}

inputPalette.open({
steps: [
{
	mode: 'list',
	label: 'Choose a template',
	items: templates.map((template) => ({
		id: template.name,
		label: template.name,
		description: template.description
	})),
	placeholder: 'Search templates...'
}
],
onComplete: async ([selectedName]) => {
if (!selectedName) return;

const template = templates.find((candidate) => candidate.name === selectedName);
if (!template) return;

if (template.prompts.length > 0) {
	inputPalette.open({
		steps: template.prompts.map((prompt) => ({
			mode: 'text' as const,
			label: `${prompt.name}${prompt.required ? '' : ' (optional)'}`,
			placeholder: `Enter ${prompt.name}...`,
			required: prompt.required
		})),
		onComplete: async (promptValues) => {
			const values: Record<string, string> = {};
			template.prompts.forEach((prompt, index) => {
				const value = promptValues[index]?.trim();
				if (value) {
					values[prompt.name] = value;
				}
			});

			try {
				const created = await instantiateTemplate(
					currentVault,
					template.name,
					values
				);
				await reloadAndNavigate(created.path, onNavigate);
			} catch (cause) {
				notifyError('Failed to create note from template.', cause);
			}
		}
	});
	return;
}

try {
	const created = await instantiateTemplate(currentVault, template.name, {});
	await reloadAndNavigate(created.path, onNavigate);
} catch (cause) {
	notifyError('Failed to create note from template.', cause);
}
}
});
} catch (cause) {
notifyError('Failed to create note from template.', cause);
}
}
},
{
id: 'focus-search',
label: 'Search Notes',
category: 'Navigation',
shortcut: '⌘O',
execute: () => {
// Handled by hotkey — opens unified palette in file mode
}
},
{
id: 'reload-vault',
label: 'Reload Vault',
category: 'Vault',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
await vaultStore.loadNotes();
} catch (cause) {
notifyError('Failed to reload the vault.', cause);
}
}
},
{
id: 'toggle-view',
label: 'Toggle View Mode',
category: 'Navigation',
shortcut: '⌘E',
execute: () => {
tabStore.toggleViewMode();
}
},
{
id: 'change-theme',
label: 'Change Theme',
category: 'Appearance',
execute: () => {
// Handled by the unified palette theme picker mode.
}
},
{
id: 'open-settings',
label: 'Settings',
category: 'Settings',
shortcut: '⌘,',
execute: () => {
void goto(`${base}/settings?vault=${encodeURIComponent(vault)}`);
}
},
...INLINE_COMMANDS.map(
(cmd): Command => ({
id: `ai-${cmd.id}`,
label: cmd.label,
category: 'AI',
execute: () => runInlineEditorCommand(cmd.id)
})
)
];
}
